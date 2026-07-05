//! Inline selector for regenerating raw presigned URLs for uploaded files.

use std::io::{self, Write};

use anyhow::{bail, Result};
use crossterm::{event::{self, Event, KeyCode}, terminal};

use crate::api::{self, RemoteFile};
use crate::app::human_size;
use crate::config::Config;

fn term_width() -> usize {
    terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let visible = strip_ansi(s);
    if visible.chars().count() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, c) in visible.chars().enumerate() {
        if i + 1 >= max {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

fn redraw(lines: &[String], prev_lines: &mut usize) {
    let width = term_width();
    let mut out = String::new();
    if *prev_lines > 1 {
        out.push_str(&format!("\x1b[{}A", *prev_lines - 1));
    }
    for (i, line) in lines.iter().enumerate() {
        out.push_str("\r\x1b[K");
        out.push_str(&truncate(line, width));
        if i + 1 < lines.len() {
            out.push_str("\r\n");
        }
    }
    print!("{}", out);
    io::stdout().flush().ok();
    *prev_lines = lines.len();
}

fn pad_to(mut lines: Vec<String>, n: usize) -> Vec<String> {
    while lines.len() < n {
        lines.push(String::new());
    }
    lines.truncate(n);
    lines
}

pub fn run(config: &mut Config) -> Result<()> {
    let files = api::list_files(config)?;
    if files.is_empty() {
        bail!("No files found");
    }

    let file = pick_file(&files)?;
    let urls = api::regenerate_urls(config, file.id)?;

    println!();
    println!("Regenerated URLs for #{} ({})", file.id, file.name);
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

fn pick_file(files: &[RemoteFile]) -> Result<RemoteFile> {
    let mut selected = 0usize;
    let mut prev_lines = 0usize;
    let max_rows = 10usize;
    let fixed_rows = max_rows + 4;

    terminal::enable_raw_mode()?;
    let result = (|| -> Result<RemoteFile> {
        loop {
            let lines = pad_to(build_lines(files, selected, max_rows), fixed_rows);
            redraw(&lines, &mut prev_lines);

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => selected = (selected + 1).min(files.len() - 1),
                    KeyCode::Enter => return Ok(files[selected].clone()),
                    KeyCode::Esc | KeyCode::Char('q') => bail!("Cancelled"),
                    _ => {}
                }
            }
        }
    })();
    terminal::disable_raw_mode()?;
    print!("\r\n");
    io::stdout().flush()?;
    result
}

fn build_lines(files: &[RemoteFile], selected: usize, max_rows: usize) -> Vec<String> {
    let mut lines = vec![" Select file to regenerate".to_string(), String::new()];

    let start = selected.saturating_sub(max_rows / 2);
    let end = (start + max_rows).min(files.len());

    for (i, file) in files.iter().enumerate().take(end).skip(start) {
        let prefix = if i == selected { "\x1b[36m▶\x1b[0m" } else { " " };
        let size = human_size(file.size as u64);
        lines.push(format!(" {prefix} #{}  {}  ({})", file.id, file.name, size));
    }

    lines.push(String::new());
    lines.push("\x1b[2m ↑↓ select  Enter regenerate  Esc cancel\x1b[0m".to_string());
    lines
}
