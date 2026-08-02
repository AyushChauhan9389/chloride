//! Self-update against GitHub Releases.
//!
//! Release assets are plain, uncompressed binaries named after their target
//! (see `.github/workflows/release.yml`), so updating is: GET the asset, write
//! it next to the running binary, rename it over the top. No archives, no
//! extraction, no extra dependencies.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::config::Config;

const REPO: &str = "ayushChauhan9389/chloride";
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The release asset for the platform this binary was built for. Must match
/// the `asset` column of the release workflow's build matrix.
#[cfg(windows)]
const ASSET: &str = "cl-x86_64-pc-windows-msvc.exe";
#[cfg(not(windows))]
const ASSET: &str = "cl-x86_64-unknown-linux-musl";

// --- Public entry points ---

/// `cl update` — always checks, reports either way, surfaces errors.
pub fn run_update() -> Result<()> {
    let (version, url) = latest_release(Duration::from_secs(20))?;
    if !is_newer(&version, VERSION) {
        println!("✅ cl {VERSION} is already up to date");
        return Ok(());
    }
    println!("⬇️  updating {VERSION} → {version}…");
    let path = download_and_install(&url, Duration::from_secs(300))?;
    println!("✅ updated to {version} ({})", path.display());
    Ok(())
}

/// Best-effort background update, run before every command except `update`.
///
/// Deliberately silent about its own failures beyond one line: a broken
/// network must never stop `cl pwd` from working. Skips entirely when stderr
/// is not a terminal, so scripts and CI never get a binary swapped underneath
/// them, and rate-limits itself to one check per day.
pub fn auto_update() {
    if std::env::var_os("CL_NO_UPDATE").is_some() || !std::io::stderr().is_terminal() {
        return;
    }

    clean_leftovers();

    let Ok(stamp) = stamp_path() else { return };
    if !check_due(&stamp) {
        return;
    }
    // Stamp *before* the network call, so an offline machine retries tomorrow
    // rather than on every single invocation.
    let _ = std::fs::write(&stamp, now_secs().to_string());

    let Ok((version, url)) = latest_release(Duration::from_secs(5)) else {
        return;
    };
    if !is_newer(&version, VERSION) {
        return;
    }

    match download_and_install(&url, Duration::from_secs(120)) {
        Ok(_) => eprintln!("✨ cl updated {VERSION} → {version} (active on your next run)"),
        Err(e) => eprintln!(
            "⚠️  auto-update to {version} failed: {e}\n   Run `cl update` to retry, or set CL_NO_UPDATE=1 to turn this off."
        ),
    }
}

// --- Release lookup ---

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        // The GitHub API rejects requests without a User-Agent.
        .user_agent(concat!("chloride-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build HTTP client")
}

/// Returns `(version_without_v_prefix, asset_download_url)`.
fn latest_release(timeout: Duration) -> Result<(String, String)> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client(timeout)?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("Failed to reach the GitHub releases API")?;

    let status = resp.status();
    if !status.is_success() {
        bail!("GitHub releases API returned {status}");
    }

    let release: Release = resp.json().context("Invalid response from GitHub")?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == ASSET)
        .with_context(|| format!("release {} has no `{ASSET}` asset", release.tag_name))?;

    Ok((
        release.tag_name.trim_start_matches('v').to_string(),
        asset.browser_download_url.clone(),
    ))
}

/// Compare `major.minor.patch`, ignoring any `-pre`/`+build` suffix. Anything
/// unparseable counts as 0, so a malformed tag never triggers an update.
fn version_parts(v: &str) -> (u64, u64, u64) {
    let core = v
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or_default();
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn is_newer(remote: &str, local: &str) -> bool {
    version_parts(remote) > version_parts(local)
}

// --- Download and self-replace ---

fn download_and_install(url: &str, timeout: Duration) -> Result<PathBuf> {
    let resp = client(timeout)?
        .get(url)
        .send()
        .context("Failed to download the new binary")?
        .error_for_status()
        .context("Failed to download the new binary")?;
    let bytes = resp.bytes().context("Failed to read the new binary")?;

    if bytes.is_empty() {
        bail!("downloaded binary was empty");
    }
    install(&bytes)
}

/// Replace the running executable with `bytes`.
///
/// Staged in the executable's own directory so the final step is a same-
/// filesystem rename, which is atomic. On Unix a running binary can be renamed
/// over freely; Windows refuses to overwrite one but does allow renaming it
/// out of the way first.
fn install(bytes: &[u8]) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Cannot locate the running binary")?;
    // Resolve symlinks so we replace the real file, not a link to it.
    let exe = exe.canonicalize().unwrap_or(exe);
    let dir = exe
        .parent()
        .context("Running binary has no parent directory")?
        .to_path_buf();

    let staged = dir.join(".cl-update-new");
    std::fs::write(&staged, bytes).with_context(|| {
        format!(
            "Cannot write to {} — no permission to update this install",
            dir.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .context("Failed to mark the new binary executable")?;
    }

    // On Windows the running .exe must be moved aside before its name is free.
    #[cfg(windows)]
    let backup = {
        let backup = dir.join(".cl-update-old");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&exe, &backup).with_context(|| {
            format!("Cannot replace {} — is another `cl` running?", exe.display())
        })?;
        backup
    };

    if let Err(e) = std::fs::rename(&staged, &exe) {
        let _ = std::fs::remove_file(&staged);
        // Put the old binary back rather than leaving nothing on PATH.
        #[cfg(windows)]
        let _ = std::fs::rename(&backup, &exe);
        return Err(e).context("Failed to swap in the new binary");
    }

    Ok(exe)
}

/// Remove the previous binary Windows made us rename instead of delete. It is
/// still locked while that process runs, so this only succeeds on a later run.
fn clean_leftovers() {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let _ = std::fs::remove_file(dir.join(".cl-update-old"));
    }
}

// --- Check throttling ---

fn stamp_path() -> Result<PathBuf> {
    Ok(Config::config_dir()?.join("update-check"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn check_due(stamp: &std::path::Path) -> bool {
    let Ok(last) = std::fs::read_to_string(stamp) else {
        return true; // never checked
    };
    let Ok(last) = last.trim().parse::<u64>() else {
        return true; // corrupt stamp
    };
    now_secs().saturating_sub(last) >= CHECK_INTERVAL.as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.2.0", "1.1.9"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        // Unparseable tags must never trigger an update off a real version.
        assert!(!is_newer("nightly", "1.0.0"));
        assert!(!is_newer("", "1.0.0"));
        // Prerelease suffixes compare on the core version only.
        assert!(!is_newer("1.0.0-rc1", "1.0.0"));
        assert!(is_newer("1.0.1-rc1", "1.0.0"));
    }

    #[test]
    fn stamp_throttles_checks() {
        let dir = std::env::temp_dir().join("cl-update-stamp-test");
        std::fs::create_dir_all(&dir).unwrap();
        let stamp = dir.join("update-check");

        let _ = std::fs::remove_file(&stamp);
        assert!(check_due(&stamp), "missing stamp should check");

        std::fs::write(&stamp, now_secs().to_string()).unwrap();
        assert!(!check_due(&stamp), "just-checked should not re-check");

        std::fs::write(&stamp, (now_secs() - CHECK_INTERVAL.as_secs() - 1).to_string()).unwrap();
        assert!(check_due(&stamp), "day-old stamp should check");

        std::fs::write(&stamp, "garbage").unwrap();
        assert!(check_due(&stamp), "corrupt stamp should check");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
