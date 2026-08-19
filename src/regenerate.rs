//! Inline selector for regenerating raw presigned URLs for uploaded files.

use anyhow::{bail, Result};
use crossterm::{event::{self, Event, KeyCode}, terminal};

use crate::api::{self, RemoteFile};
use crate::app::human_size;
use crate::config::Config;
use crate::inline::{finish, pad_to, redraw};

pub fn run(config: &mut Config) -> Result<()> {
    let files = api::list_files(config)?;
    if files.is_empty() {
        bail!("No files found");
    }

    let file = pick_file(&files)?;
    let urls = api::regenerate_urls(config, file.id)?;

    println!();
    println!("Regenerated URLs for #{} ({})", file.id, file.name);
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
    // Close the block on stderr, where it was drawn — stdout may be a pipe.
    finish(prev_lines);
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
