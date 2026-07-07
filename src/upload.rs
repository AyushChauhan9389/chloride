//! Upload: inline expiry picker + live progress bar (no fullscreen TUI).

use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use crate::api::{self, UploadResult};
use crate::app::human_size;
use crate::config::Config;

const EXPIRY_OPTIONS: &[(i64, &str)] = &[
    (60 * 60 * 24 * 7, "7 days"),
    (60 * 60 * 24 * 30, "30 days"),
    (60 * 60 * 24 * 365, "365 days"),
];

struct FileSpec {
    path: std::path::PathBuf,
    name: String,
    content_type: String,
}

#[derive(Clone, Default)]
struct UploadState {
    sent: u64,
    total: u64,
    started: Option<Instant>,
    finished: bool,
    error: Option<String>,
    result: Option<UploadResult>,
}

/// Return the terminal width in columns, or 80 if it can't be detected.
fn term_width() -> usize {
    crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
}

/// Truncate a string to `max` visible characters, accounting for Unicode
/// width (roughly — ignores ANSI escape codes in the count).
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut len = 0;
    for c in s.chars() {
        if len + 1 > max.saturating_sub(1) {
            out.push('…');
            break;
        }
        out.push(c);
        len += 1;
    }
    out
}

/// Redraw `lines` in place using per-line clears. Never uses `\x1b[J`
/// (clear-to-end-of-screen) which would eat content below our block.
/// Instead moves up, then for each row: `\r\x1b[K` (clear line only) + content.
/// Caller must pass a FIXED number of lines every time so `prev_lines`
/// never grows (pad with empty strings if needed).
fn redraw(lines: &[String], prev_lines: &mut usize) {
    let width = term_width();
    let mut out = String::new();

    if *prev_lines > 1 {
        // After drawing N lines, the cursor is already on line N. Moving up
        // N rows jumps above our block; move up N-1 to return to line 1.
        out.push_str(&format!("\x1b[{}A", *prev_lines - 1));
    }

    for (i, line) in lines.iter().enumerate() {
        out.push_str("\r\x1b[K"); // clear THIS line only, cursor stays at col 0
        if i < lines.len() {
            out.push_str(&truncate(line, width));
        }
        if i + 1 < lines.len() {
            out.push_str("\r\n");
        }
    }

    print!("{}", out);
    io::stdout().flush().ok();
    *prev_lines = lines.len();
}

/// Pad `lines` with empty strings up to `n` rows, so the line count is
/// stable across redraws (prevents cursor drift).
fn pad_to(lines: Vec<String>, n: usize) -> Vec<String> {
    let mut v = lines;
    while v.len() < n {
        v.push(String::new());
    }
    v.truncate(n);
    v
}

pub fn run(config: &Config, files: &[std::path::PathBuf], expires_in: Option<i64>) -> Result<()> {
    if files.is_empty() {
        bail!("No files given");
    }
    if !config.is_logged_in() && config.refresh_token.is_none() {
        bail!("Not logged in. Run `cl login`.");
    }

    let mut specs: Vec<FileSpec> = Vec::new();
    for p in files {
        let meta = std::fs::metadata(p)?;
        if !meta.is_file() {
            bail!("'{}' is not a regular file", p.display());
        }
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        specs.push(FileSpec {
            path: p.clone(),
            name,
            content_type: mime_for(&p.to_string_lossy()),
        });
    }

    let total_size: u64 = specs
        .iter()
        .map(|s| std::fs::metadata(&s.path).map(|m| m.len()).unwrap_or(0))
        .sum();
    let file_count = specs.len();

    let chosen_expiry = match expires_in {
        Some(e) => e,
        None => pick_expiry()?,
    };

    let shared_config = Arc::new(Mutex::new(config.clone()));
    let results = run_uploads(shared_config.clone(), specs, chosen_expiry, total_size, file_count)?;

    if let Ok(cfg) = shared_config.lock() {
        let _ = cfg.save();
    }

    for r in &results {
        println!();
        println!("✅ file #{}", r.fileId);
        println!("   short download: {}", r.short_download_url);
    }
    Ok(())
}

fn mime_for(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with(".png") {
        "image/png".into()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".into()
    } else if lower.ends_with(".gif") {
        "image/gif".into()
    } else if lower.ends_with(".pdf") {
        "application/pdf".into()
    } else if lower.ends_with(".txt") || lower.ends_with(".md") {
        "text/plain".into()
    } else if lower.ends_with(".zip") {
        "application/zip".into()
    } else if lower.ends_with(".mp4") {
        "video/mp4".into()
    } else if lower.ends_with(".mp3") {
        "audio/mpeg".into()
    } else {
        "application/octet-stream".into()
    }
}

// --- Inline expiry picker ---

fn pick_expiry() -> Result<i64> {
    use crossterm::event::{self, Event, KeyCode};
    use crossterm::terminal;

    let mut selected: usize = 0;
    let mut prev_lines = 0usize;
    const PICKER_ROWS: usize = 7;

    terminal::enable_raw_mode()?;
    let result = (|| -> Result<i64> {
        loop {
            let lines = pad_to(build_picker_lines(selected), PICKER_ROWS);
            redraw(&lines, &mut prev_lines);

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1).min(EXPIRY_OPTIONS.len() - 1);
                    }
                    KeyCode::Enter => return Ok(EXPIRY_OPTIONS[selected].0),
                    KeyCode::Esc | KeyCode::Char('q') => bail!("Cancelled"),
                    _ => {}
                }
            }
        }
    })();

    terminal::disable_raw_mode()?;
    // Cursor is already on the picker's last line. One newline starts the next
    // output below it without jumping extra rows or clearing user scrollback.
    if prev_lines > 0 {
        print!("\r\n");
        io::stdout().flush()?;
    }
    result
}

fn build_picker_lines(selected: usize) -> Vec<String> {
    let mut lines = vec![" Link expiry".to_string(), String::new()];
    for (i, (_, label)) in EXPIRY_OPTIONS.iter().enumerate() {
        if i == selected {
            lines.push(format!(" \x1b[36m▶ {}\x1b[0m", label));
        } else {
            lines.push(format!("   {}", label));
        }
    }
    lines.push(String::new());
    lines.push("\x1b[2m ↑↓ select  Enter confirm  Esc cancel\x1b[0m".to_string());
    lines
}

// --- Upload progress loop ---

fn run_uploads(
    config: Arc<Mutex<Config>>,
    specs: Vec<FileSpec>,
    expires_in: i64,
    total_size: u64,
    file_count: usize,
) -> Result<Vec<UploadResult>> {
    let rt = tokio::runtime::Runtime::new()?;

    let mut results: Vec<UploadResult> = Vec::new();
    let mut global_sent: u64 = 0;

    for (idx, spec) in specs.into_iter().enumerate() {
        let file_size = std::fs::metadata(&spec.path).map(|m| m.len()).unwrap_or(0);

        let state = Arc::new(Mutex::new(UploadState {
            total: file_size,
            started: Some(Instant::now()),
            ..Default::default()
        }));

        let progress_state = state.clone();
        let on_progress = Arc::new(move |sent: u64, total: u64| {
            if let Ok(mut s) = progress_state.lock() {
                s.sent = sent;
                s.total = total;
            }
        }) as api::ProgressFn;

        let done_state = state.clone();
        let shared_config = config.clone();
        let path = spec.path.clone();
        let name = spec.name.clone();
        let ct = spec.content_type.clone();

        let handle = rt.spawn(async move {
            let result =
                api::upload_file(&shared_config, &path, &name, &ct, expires_in, on_progress).await;
            if let Ok(mut s) = done_state.lock() {
                match &result {
                    Ok(r) => s.result = Some(r.clone()),
                    Err(e) => s.error = Some(e.to_string()),
                }
                s.finished = true;
            }
            result
        });

        // Inline progress rendering loop with a fixed row count so
        // cursor-up never drifts.
        const PROG_ROWS: usize = 5;
        let mut prev_lines = 0usize;
        loop {
            let s = state.lock().unwrap().clone();
            let lines = pad_to(
                build_progress_lines(&s, &spec.name, idx, file_count, global_sent, total_size),
                PROG_ROWS,
            );
            redraw(&lines, &mut prev_lines);

            if s.finished {
                break;
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        // Cursor is already on the progress block's last line. One newline
        // leaves the final progress visible and continues below it.
        if prev_lines > 0 {
            print!("\r\n");
            io::stdout().flush()?;
        }

        // Print final status for this file.
        let s = state.lock().unwrap().clone();
        if let Some(err) = &s.error {
            println!(" ✖ {}: {}", spec.name, err);
        } else if s.result.is_some() {
            println!(" ✓ {} uploaded", spec.name);
        }

        let outcome = rt.block_on(async { handle.await })??;
        let sent = state.lock().unwrap().sent;
        global_sent += sent.max(file_size);
        results.push(outcome);
    }

    Ok(results)
}

#[allow(clippy::too_many_arguments)]
fn build_progress_lines(
    state: &UploadState,
    name: &str,
    idx: usize,
    file_count: usize,
    global_sent_before_file: u64,
    total_size: u64,
) -> Vec<String> {
    let pct = if state.total > 0 {
        (state.sent as f64 / state.total as f64) * 100.0
    } else {
        0.0
    };

    // Responsive bar: fill available width minus label/prefix padding.
    let width = term_width();
    // "  ████░░░░ 40.0%" → 2 spaces + bar + 1 space + "~5 chars for pct
    let bar_width = width.saturating_sub(10).max(10).min(60);

    let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
    let filled = filled.min(bar_width);
    let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);

    let mut lines = Vec::new();

    lines.push(format!(" File {}/{}: {}", idx + 1, file_count, name));
    lines.push(format!(" {} {:.1}%", bar, pct));

    // Speed + ETA
    if let Some(started) = state.started {
        let elapsed = started.elapsed().as_secs_f64();
        if elapsed > 0.0 && state.sent > 0 {
            let speed = state.sent as f64 / elapsed;
            let remaining = state.total.saturating_sub(state.sent);
            let eta = if speed > 0.0 { remaining as f64 / speed } else { 0.0 };
            lines.push(format!(
                "  {}/s · ETA {} · {}/{}",
                human_size(speed as u64),
                fmt_duration(eta),
                human_size(state.sent),
                human_size(state.total),
            ));
        } else {
            lines.push("  Starting…".to_string());
        }
    }

    // Overall progress across all files
    if total_size > 0 {
        let current_global = global_sent_before_file.saturating_add(state.sent);
        let g_pct = (current_global as f64 / total_size as f64) * 100.0;
        lines.push(format!(
            " Overall: {:.1}% ({}/{})",
            g_pct,
            human_size(current_global),
            human_size(total_size),
        ));
    }

    if let Some(err) = &state.error {
        lines.push(format!(" ✖ {}", err));
    }

    lines
}

fn fmt_duration(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "—".into();
    }
    let s = secs as u64;
    if s < 60 {
        format!("{}s", s)
    } else if s < 3600 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    }
}
