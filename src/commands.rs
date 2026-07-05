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

pub fn regenerate(file_id: Option<i64>) -> Result<()> {
    let mut config = Config::load()?;
    let file_id = match file_id {
        Some(id) => id,
        None => return crate::regenerate::run(&mut config),
    };
    let urls = api::regenerate_urls(&mut config, file_id)?;

    println!("Regenerated URLs for file #{file_id}");
    if let Some(raw) = urls.view_url {
        println!("raw view: {raw}");
    }
    if let Some(short) = urls.short_view_url {
        println!("short view: {short}");
    }
    if let Some(raw) = urls.download_url {
        println!("raw download: {raw}");
    }
    if let Some(short) = urls.short_download_url {
        println!("short download: {short}");
    }
    Ok(())
}

