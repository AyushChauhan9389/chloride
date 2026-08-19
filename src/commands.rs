//! Inline (non-TUI) command implementations, e.g. `cl rm file.txt`.

use anyhow::{bail, Result};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::api;
use crate::config::Config;

pub fn touch_file(filename: &str) -> Result<()> {
    if Path::new(filename).exists() {
        println!("📄 File '{filename}' already exists");
    } else {
        fs::File::create(filename)?;
        println!("✅ Created file '{filename}'");
    }
    Ok(())
}

pub fn create_directory(dirname: &str, parents: bool) -> Result<()> {
    let path = Path::new(dirname);
    if path.exists() {
        if path.is_dir() {
            println!("📁 Directory '{dirname}' already exists");
        } else {
            eprintln!("cl: cannot create directory '{dirname}': File exists");
        }
        return Ok(());
    }

    let result = if parents {
        fs::create_dir_all(dirname)
    } else {
        fs::create_dir(dirname)
    };

    match result {
        Ok(_) => println!("✅ Created directory '{dirname}'"),
        Err(e) => {
            eprintln!("cl: error creating directory '{dirname}': {e}");
            return Err(e.into());
        }
    }
    Ok(())
}

pub fn print_working_directory() -> Result<()> {
    let path = env::current_dir()?;
    println!("{}", path.display());
    Ok(())
}

pub fn remove_path(path_str: &str, recursive: bool, force: bool) -> Result<()> {
    let path = Path::new(path_str);

    if !path.exists() {
        if force {
            return Ok(());
        }
        eprintln!("cl: cannot remove '{path_str}': No such file or directory");
        return Ok(());
    }

    let is_dir = path.is_dir();

    if is_dir && !recursive {
        eprintln!("cl: cannot remove '{path_str}': Is a directory. Use -r/--recursive.");
        return Ok(());
    }

    if !force {
        let what = if is_dir { "directory" } else { "file" };
        print!("🗑️  Delete {what} '{path_str}'? (y/N): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let answer = input.trim().to_lowercase();
        if answer != "y" && answer != "yes" {
            println!("❌ Cancelled");
            return Ok(());
        }
    }

    let result = if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };

    match result {
        Ok(_) => println!("✅ Deleted '{path_str}'"),
        Err(e) => {
            eprintln!("cl: error deleting '{path_str}': {e}");
            return Err(e.into());
        }
    }
    Ok(())
}

pub fn logout() -> Result<()> {
    let mut config = Config::load()?;
    if config.is_logged_in() {
        let email = config
            .user
            .as_ref()
            .map(|u| u.email.clone())
            .unwrap_or_default();
        config.logout();
        config.save()?;
        println!("✅ Logged out{}", if email.is_empty() { String::new() } else { format!(" ({email})") });
    } else {
        println!("ℹ️  Not logged in");
    }
    Ok(())
}

pub fn whoami() -> Result<()> {
    let config = Config::load()?;
    if let Some(user) = &config.user {
        println!(
            "{} (id: {}, role: {}, plan: {})",
            user.email,
            user.id,
            user.role.as_deref().unwrap_or("none"),
            user.plan.as_deref().unwrap_or("none"),
        );
        println!("  server: {}", config.base_url);
        println!("  config: {}", Config::config_path()?.display());
    } else {
        println!("Not logged in. Run `cl login` to authenticate.");
    }
    Ok(())
}

/// `cl logo`. The colour form needs a truecolor-capable terminal; pipes,
/// NO_COLOR, and dumb terminals get the monochrome braille rendering instead
/// (which is also what survives a copy-paste into an issue or a README).
pub fn logo() -> Result<()> {
    use std::io::IsTerminal;
    let color = io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);
    print!(
        "{}",
        if color {
            crate::logo::LOGO_COLOR
        } else {
            crate::logo::LOGO_PLAIN
        }
    );
    println!();
    println!(
        "  \u{1F9EA} Chloride v{} \u{2014} https://chloride.carbonkit.tech",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

pub fn quota() -> Result<()> {
    let mut config = Config::load()?;
    if !config.is_logged_in() && config.refresh_token.is_none() {
        bail!("Not logged in. Run `cl login` to authenticate.");
    }

    let storage = api::get_storage(&mut config)?;

    if let Some(user) = &config.user {
        println!("📧 {}", user.email);
    }
    if storage.is_unlimited() {
        println!("📦 Plan: Unlimited");
        let used = storage.used_formatted.as_deref().unwrap_or("?");
        println!("📊 Used: {used}");
        println!("📈 Quota: ∞ (no limit)");
    } else {
        let used = storage.used_formatted.as_deref().unwrap_or("?");
        let left = storage.left_formatted.as_deref().unwrap_or("?");
        let limit = storage.limit_formatted.as_deref().unwrap_or("?");
        println!("📊 Used:  {used}");
        println!("🪫 Left:  {left}");
        println!("📦 Limit: {limit}");
        let pct = storage.percentageUsed;
        let bar_width = 20usize;
        let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
        let filled = filled.min(bar_width);
        let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
        println!("📈 [{bar}] {pct:.1}%");
    }
    Ok(())
}

pub fn upload(files: Vec<String>, expires_in: Option<i64>) -> Result<()> {
    let config = Config::load()?;
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    crate::upload::run(&config, &paths, expires_in)
}

/// `cl find`. Matches file names and file contents in one walk.
///
/// Streams results as they are found rather than collecting, so the first hit
/// lands immediately on a large tree.
pub fn search(
    pattern: String,
    path: Option<String>,
    files_only: bool,
    content_only: bool,
    names_only: bool,
    opts: crate::SearchOpts,
) -> Result<()> {
    use crate::search::{Hit, Kind, Query};
    use std::io::IsTerminal;

    let root = PathBuf::from(path.unwrap_or_else(|| ".".into()));
    let color = io::stdout().is_terminal();

    // The picker needs a terminal to read keys from and to draw on, but NOT on
    // stdout — that stays free for the selection, so the picker still appears
    // when stdout is a pipe or a file. Without a tty at all (CI, cron) it would
    // block forever on a keypress that can never arrive, so stream instead.
    let interactive = !opts.no_input && io::stdin().is_terminal() && io::stderr().is_terminal();

    // Contents are the default. A pattern that cannot compile as a regex is
    // almost always a glob aimed at file names ('*.zip'), so fall back rather
    // than erroring — but an explicit -c still reports the bad pattern.
    // Mode was inferred rather than demanded, so the picker may re-infer it as
    // the user edits the query.
    let auto_kind = !files_only && !content_only && !names_only;
    let kind = if files_only {
        Kind::Files
    } else if content_only || names_only || crate::search::is_valid_regex(&pattern) {
        Kind::Content
    } else {
        Kind::Files
    };

    let mut query = Query::new(pattern, &root, kind);
    query.hidden = !opts.no_hidden;
    query.no_ignore = opts.no_ignore;
    query.limit = opts.limit;
    query.ext = opts.ext.map(|e| e.trim_start_matches('.').to_lowercase());
    // Context only makes sense for the rendered diff view: the picker shows one
    // compact row per hit, and the machine form is line-per-match.
    let rendered = color && !names_only && kind != Kind::Files;
    query.context = if rendered { opts.context } else { 0 };

    if interactive {
        let search = crate::search::spawn(query.clone())?;
        return match crate::picker::run(query, search, auto_kind)? {
            crate::picker::Outcome::Selected(s) => {
                println!("{s}");
                Ok(())
            }
            crate::picker::Outcome::Cancelled => std::process::exit(1),
        };
    }

    let search = crate::search::spawn(query)?;

    let (dim, cyan, reset) = if color {
        ("\x1b[2m", "\x1b[36m", "\x1b[0m")
    } else {
        ("", "", "")
    };

    if rendered {
        return render_diff(search, &root, opts.context);
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut seen_files: Vec<PathBuf> = Vec::new();
    let mut count = 0usize;

    for hit in search.hits.iter() {
        // `-l`: one line per file, regardless of how many lines matched.
        if names_only {
            if seen_files.iter().any(|p| p == hit.path()) {
                continue;
            }
            seen_files.push(hit.path().to_path_buf());
        }

        let display = hit.path().strip_prefix(&root).unwrap_or(hit.path()).display();
        let sep = if opts.print0 { '\0' } else { '\n' };
        let line = match (&hit, names_only) {
            (_, true) => format!("{display}{sep}"),
            (Hit::File { .. }, _) => format!("{display}{sep}"),
            (Hit::Line { line, text, .. }, _) => {
                format!("{cyan}{display}{reset}{dim}:{line}:{reset}{text}{sep}")
            }
        };

        // A closed pipe (`cl find x | head`) is a normal exit, not an error.
        if out.write_all(line.as_bytes()).is_err() {
            return Ok(());
        }
        count += 1;
    }

    if out.flush().is_err() {
        return Ok(());
    }
    if count == 0 {
        eprintln!("no matches");
        std::process::exit(1);
    }
    Ok(())
}

pub fn regenerate(file_id: Option<i64>) -> Result<()> {
    let mut config = Config::load()?;
    let file_id = match file_id {
        Some(id) => id,
        None => return crate::regenerate::run(&mut config),
    };
    let urls = api::regenerate_urls(&mut config, file_id)?;

    println!("Regenerated URLs for file #{file_id}");
    if let Some(short) = urls.short_download_url {
        println!("short download: {short}");
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    #[test]
    fn logo_art_is_terminal_safe() {
        // Both renderings must fit a standard 80-column terminal.
        for art in [crate::logo::LOGO_COLOR, crate::logo::LOGO_PLAIN] {
            assert!(!art.is_empty());
            for line in art.lines() {
                assert!(
                    crate::inline::visible_width(line) <= 80,
                    "row too wide: {line:?}"
                );
            }
        }
        // Colour rows close their attributes so nothing bleeds into the
        // shell prompt that follows.
        for line in crate::logo::LOGO_COLOR.lines() {
            assert!(line.ends_with("\u{1b}[0m"), "unterminated row: {line:?}");
        }
        // The plain form is what lands in pipes and pasted text: it must
        // carry no escape sequences at all.
        assert!(!crate::logo::LOGO_PLAIN.contains('\u{1b}'));
    }
}

/// Git-diff style content output: a heading per file, a line-number gutter,
/// dimmed context, and `⋮` wherever the matches are not contiguous.
///
/// Unlike every other output path this one buffers. Grouping by file is the
/// whole point of the view, and a parallel walk interleaves files — streaming
/// it directly re-emits a heading every time the thread pool switches file.
fn render_diff(search: crate::search::Search, root: &Path, _context: usize) -> Result<()> {
    use crate::search::Hit;
    use std::collections::HashMap;

    const ACCENT: &str = "\x1b[36m";
    const DIM: &str = "\x1b[2m";
    const BOLD: &str = "\x1b[1m";
    const RESET: &str = "\x1b[0m";

    // Keep first-seen file order, but gather every line for a file together.
    let mut order: Vec<PathBuf> = Vec::new();
    let mut grouped: HashMap<PathBuf, Vec<(u64, String, bool)>> = HashMap::new();
    let mut matches = 0usize;

    for hit in search.hits.iter() {
        let Hit::Line { path, line, text, context: is_ctx } = hit else {
            continue;
        };
        if !is_ctx {
            matches += 1;
        }
        let entry = grouped.entry(path.clone()).or_insert_with(|| {
            order.push(path.clone());
            Vec::new()
        });
        entry.push((line, text, is_ctx));
    }

    if matches == 0 {
        eprintln!("no matches");
        std::process::exit(1);
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let width = crate::inline::term_width();
    let rule = "─".repeat(width.saturating_sub(2).min(72));
    let files = order.len();

    for (n, path) in order.iter().enumerate() {
        let mut lines = grouped.remove(path).unwrap_or_default();
        // Threads can emit a file's lines out of order; dedupe keeps a line
        // that is both a match and another match's context from doubling up.
        lines.sort_by_key(|(line, _, is_ctx)| (*line, *is_ctx));
        lines.dedup_by_key(|(line, _, _)| *line);

        let shown = path.strip_prefix(root).unwrap_or(path).display();
        let hits = lines.iter().filter(|(_, _, c)| !c).count();
        if n > 0 && writeln!(out).is_err() {
            return Ok(());
        }
        if writeln!(
            out,
            "  {ACCENT}{BOLD}{shown}{RESET}{DIM}   {hits} match{}{RESET}\n  {DIM}{rule}{RESET}",
            if hits == 1 { "" } else { "es" }
        )
        .is_err()
        {
            return Ok(());
        }

        let mut last = 0u64;
        for (line, text, is_ctx) in &lines {
            // A gap in line numbers means these are separate hunks.
            if last != 0 && *line > last + 1 && writeln!(out, "  {DIM}    ⋮{RESET}").is_err() {
                return Ok(());
            }
            last = *line;
            let row = if *is_ctx {
                format!("{DIM}  {line:>5} │ {text}{RESET}")
            } else {
                format!("{ACCENT}›{RESET} {DIM}{line:>5} │{RESET} {text}")
            };
            if writeln!(out, "{}", crate::inline::truncate(&row, width)).is_err() {
                return Ok(());
            }
        }
    }

    let _ = writeln!(
        out,
        "\n  {DIM}{matches} match{} in {files} file{}{RESET}",
        if matches == 1 { "" } else { "es" },
        if files == 1 { "" } else { "s" }
    );
    let _ = out.flush();
    Ok(())
}
