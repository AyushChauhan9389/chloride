//! The inline result picker behind `cl find`.
//!
//! Draws a fixed-height block in place (see [`crate::inline`]) while results
//! stream in from a background walk, so the terminal keeps one block instead of
//! a screenful of paths. Selecting collapses the block to a single line and
//! prints the choice to stdout, which is what makes `cl upload "$(cl find x)"`
//! work.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;

use crate::inline::{fit, finish, pad_to, redraw, term_width, visible_width};
use crate::search::{Hit, Query, Search};

/// Content matches shown before collapsing behind a "… N more" row. Content
/// hits outnumber name hits by orders of magnitude; without a cap a common word
/// buries the file list under thousands of lines.
const CONTENT_CAP: usize = 50;

/// Visible result rows. The block also spends rows on the prompt, the section
/// headers and the footer.
const WINDOW: usize = 8;

/// Body rows, counting group headings and blank spacers.
const BODY_ROWS: usize = 12;

/// Terminal columns below which the preview is dropped — two columns need room
/// before either is readable.
const PREVIEW_MIN_WIDTH: usize = 96;

/// Width of the results column when the preview sits beside it.
const LEFT_WIDTH: usize = 54;

/// How long to block waiting for a keystroke before repainting. Results arrive
/// on a channel, so the loop has to wake up regularly to show them.
const TICK: Duration = Duration::from_millis(60);

const ACCENT: &str = "\x1b[36m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";
/// Selection highlight: a filled rail rather than a background block, which
/// survives terminals with unusual colour schemes.
/// Matched term inside a line.
const HOT: &str = "\x1b[1;33m";

/// One selectable row.
struct Row {
    hit: Hit,
    size: u64,
    mtime: SystemTime,
}

impl Row {
    fn is_file(&self) -> bool {
        matches!(self.hit, Hit::File { .. })
    }

    /// What gets printed on Enter. Content hits carry `:line` so the caller can
    /// jump straight to it.
    fn selection(&self, root: &Path) -> String {
        let path = self.hit.path();
        let shown = path.strip_prefix(root).unwrap_or(path).display();
        match &self.hit {
            Hit::File { .. } => format!("{shown}"),
            Hit::Line { line, .. } => format!("{shown}:{line}"),
        }
    }
}

/// Outcome of running the picker.
pub enum Outcome {
    /// User chose a row; the string is what should go to stdout.
    Selected(String),
    /// User pressed Esc / q, or there was nothing to choose.
    Cancelled,
}

pub fn run(query: Query, search: Search, auto_kind: bool) -> Result<Outcome> {
    let mut state = State::new(query.pattern.clone());
    state.mode_names = query.kind == crate::search::Kind::Files;

    terminal::enable_raw_mode()?;
    let result = drive(&mut state, search, query, auto_kind);
    // Always restore the terminal, even if the loop failed.
    let _ = terminal::disable_raw_mode();
    finish(state.prev_lines);
    result
}

struct State {
    pattern: String,
    rows: Vec<Row>,
    /// Cursor into the currently visible (possibly capped) row list.
    selected: usize,
    /// First visible row, for scrolling.
    offset: usize,
    expanded: bool,
    preview: bool,
    /// True while `/` has the query open for editing.
    editing: bool,
    /// Whether the current walk is matching names rather than contents.
    mode_names: bool,
    done: bool,
    /// Until the user moves, the cursor stays pinned to the top row; after
    /// that it follows the item it was on as results stream in around it.
    user_moved: bool,
    prev_lines: usize,
    /// One stat per path, shared by every content hit in the same file.
    meta: HashMap<PathBuf, (u64, SystemTime)>,
}

impl State {
    fn reset(&mut self) {
        self.rows.clear();
        self.meta.clear();
        self.selected = 0;
        self.offset = 0;
        self.user_moved = false;
        self.done = false;
    }

    fn new(pattern: String) -> Self {
        Self {
            pattern,
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            expanded: false,
            preview: true,
            editing: false,
            mode_names: false,
            done: false,
            user_moved: false,
            prev_lines: 0,
            meta: HashMap::new(),
        }
    }

    /// Insert keeping the list sorted: files before content, then newest first.
    /// Sorting on insert matters because a parallel walk yields results in
    /// nondeterministic order — appending would make the list visibly reshuffle
    /// while the user is trying to read it.
    fn insert(&mut self, hit: Hit) {
        let path = hit.path().to_path_buf();
        let (size, mtime) = *self.meta.entry(path).or_insert_with_key(|p| {
            std::fs::metadata(p)
                .map(|m| (m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
                .unwrap_or((0, SystemTime::UNIX_EPOCH))
        });

        let row = Row { hit, size, mtime };
        let pos = self
            .rows
            .binary_search_by(|probe| {
                probe
                    .is_file()
                    .cmp(&row.is_file())
                    .reverse()
                    .then_with(|| probe.mtime.cmp(&row.mtime).reverse())
            })
            .unwrap_or_else(|e| e);
        self.rows.insert(pos, row);

        // Only chase the item once the user has taken control of the cursor;
        // otherwise a streaming result inserted above row 0 would drag the
        // selection down while they are still reading.
        if self.user_moved && pos <= self.selected && self.rows.len() > 1 {
            self.selected += 1;
        }
    }

    fn file_count(&self) -> usize {
        self.rows.iter().filter(|r| r.is_file()).count()
    }

    fn content_count(&self) -> usize {
        self.rows.len() - self.file_count()
    }

    /// Indices of rows that are actually selectable, honouring the content cap.
    fn visible_rows(&self) -> Vec<usize> {
        let files = self.file_count();
        let content_shown = if self.expanded {
            self.content_count()
        } else {
            self.content_count().min(CONTENT_CAP)
        };
        (0..files + content_shown).collect()
    }

    fn clamp(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = self.selected.min(len - 1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + WINDOW {
            self.offset = self.selected + 1 - WINDOW;
        }
    }
}

/// Re-run the walk for the current query. Dropping the old `Search` trips its
/// cancel flag, so a stale walk stops the moment the query changes instead of
/// burning threads in the background.
fn restart(state: &mut State, base: &Query, auto_kind: bool) -> Option<Search> {
    state.reset();
    if state.pattern.is_empty() {
        return None;
    }
    let mut q = base.clone();
    q.pattern = state.pattern.clone();
    if auto_kind {
        // Same rule as the CLI: a pattern that cannot be a regex is a glob
        // aimed at file names.
        q.kind = if crate::search::is_valid_regex(&q.pattern) {
            crate::search::Kind::Content
        } else {
            crate::search::Kind::Files
        };
        state.mode_names = q.kind == crate::search::Kind::Files;
    }
    // A half-typed regex is not an error worth showing; just no results yet.
    crate::search::spawn(q).ok()
}

fn drive(
    state: &mut State,
    search: Search,
    base: Query,
    auto_kind: bool,
) -> Result<Outcome> {
    let root = base.root.clone();
    let root = root.as_path();
    let mut search = Some(search);
    loop {
        // Drain whatever the walk has produced since the last repaint.
        if let Some(active) = search.as_ref() {
            let mut drained = 0;
            while let Ok(hit) = active.hits.try_recv() {
                state.insert(hit);
                drained += 1;
                // Don't starve the UI on a firehose; repaint and come back.
                if drained >= 512 {
                    break;
                }
            }
            if drained == 0
                && !state.done
                && matches!(
                    active.hits.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Disconnected)
                )
            {
                state.done = true;
            }
        } else {
            state.done = true;
        }

        state.clamp();
        let lines = render(state, root);
        let height = lines.len();
        redraw(&pad_to(lines, height), &mut state.prev_lines);

        if !event::poll(TICK)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        if std::env::var_os("CL_PICKER_DEBUG").is_some() {
            eprintln!("\r[dbg] key={:?} mods={:?} kind={:?} sel={} rows={} vis={}\r",
                key.code, key.modifiers, key.kind, state.selected,
                state.rows.len(), state.visible_rows().len());
        }
        // While editing, printable keys go into the query rather than acting.
        if state.editing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => state.editing = false,
                KeyCode::Backspace => {
                    state.pattern.pop();
                    search = restart(state, &base, auto_kind);
                }
                KeyCode::Char(c) => {
                    state.pattern.push(c);
                    search = restart(state, &base, auto_kind);
                }
                KeyCode::Up => state.selected = state.selected.saturating_sub(1),
                KeyCode::Down => state.selected = state.selected.saturating_add(1),
                _ => {}
            }
            continue;
        }

        match action(key) {
            Action::Quit => return Ok(Outcome::Cancelled),
            Action::Down => {
                state.user_moved = true;
                state.selected = state.selected.saturating_add(1);
            }
            Action::Up => {
                state.user_moved = true;
                state.selected = state.selected.saturating_sub(1);
            }
            Action::Expand => state.expanded = !state.expanded,
            Action::Preview => state.preview = !state.preview,
            Action::EditQuery => state.editing = true,
            Action::Select => {
                let visible = state.visible_rows();
                return Ok(match visible.get(state.selected) {
                    Some(&i) => Outcome::Selected(state.rows[i].selection(root)),
                    None => Outcome::Cancelled,
                });
            }
            Action::Edit => {
                let visible = state.visible_rows();
                if let Some(&i) = visible.get(state.selected) {
                    edit(&state.rows[i])?;
                    // The editor repainted the screen; start the block fresh.
                    state.prev_lines = 0;
                }
            }
            Action::None => {}
        }
    }
}

enum Action {
    Up,
    Down,
    Select,
    Edit,
    Expand,
    Preview,
    EditQuery,
    Quit,
    None,
}

fn action(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => Action::Quit,
        (KeyCode::Char('c' | 'd'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => Action::Down,
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => Action::Up,
        (KeyCode::Enter, _) => Action::Select,
        (KeyCode::Char('E' | 'e'), _) => Action::Edit,
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => Action::Preview,
        (KeyCode::Char('/'), _) => Action::EditQuery,
        (KeyCode::Tab, _) => Action::Expand,
        _ => Action::None,
    }
}

/// Hand the terminal to `$VISUAL`/`$EDITOR`, jumping to the line for a content
/// hit, then take it back.
fn edit(row: &Row) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vim".into() });

    let path = row.hit.path().to_string_lossy().into_owned();
    let line = match &row.hit {
        Hit::Line { line, .. } => Some(*line),
        Hit::File { .. } => None,
    };

    // Editors disagree about how to be told a line number.
    let base = Path::new(&editor)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| editor.clone());
    let args: Vec<String> = match (line, base.as_str()) {
        (Some(n), "vim" | "nvim" | "vi" | "nano") => vec![format!("+{n}"), path],
        (Some(n), "hx" | "helix" | "subl" | "code" | "codium") => {
            if base.starts_with("code") || base == "codium" {
                vec!["-g".into(), format!("{path}:{n}")]
            } else {
                vec![format!("{path}:{n}")]
            }
        }
        _ => vec![path],
    };

    // The child needs the raw terminal back, and full control of the tty.
    terminal::disable_raw_mode()?;
    println!();
    let status = std::process::Command::new(&editor).args(&args).status();
    terminal::enable_raw_mode()?;

    if let Err(e) = status {
        // Not fatal — just tell the user and stay in the picker.
        eprintln!("\r\ncould not launch {editor}: {e}\r");
    }
    Ok(())
}

/// Emoji for a file, by extension. Purely decorative, but it makes a long list
/// scannable by shape before you read a single word.
fn icon(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "rs" => "🦀",
        "ts" | "tsx" | "js" | "jsx" | "mjs" => "📜",
        "json" | "toml" | "yaml" | "yml" | "lock" => "⚙️ ",
        "md" | "txt" => "📄",
        "sh" | "bash" | "ps1" | "nsi" => "🐚",
        "png" | "jpg" | "jpeg" | "gif" | "svg" => "🖼️ ",
        "zip" | "gz" | "tar" | "apk" => "📦",
        _ => "•",
    }
}

/// Bold the matched term inside a line so the eye lands on it. Literal,
/// case-insensitive: close enough visually, and it cannot fail on a regex.
fn highlight(text: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }
    let hay = text.to_lowercase();
    let needle = pattern.to_lowercase();
    match hay.find(&needle) {
        Some(i) if needle.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') => {
            format!(
                "{}{HOT}{}{RESET}{}",
                &text[..i],
                &text[i..i + needle.len()],
                &text[i + needle.len()..]
            )
        }
        _ => text.to_string(),
    }
}

fn render(state: &State, root: &Path) -> Vec<String> {
    let term = term_width();
    let visible = state.visible_rows();
    let show_preview = state.preview && term >= PREVIEW_MIN_WIDTH;
    // Results get the whole width when there is no preview beside them.
    let list_width = if show_preview { LEFT_WIDTH } else { term.saturating_sub(2) };

    let mut lines = Vec::new();

    // ── query line: pattern, mode badge, totals ──────────────────────────
    let mode = if state.mode_names { "names" } else { "content" };
    let caret = if state.editing {
        format!("{ACCENT}▏{RESET}")
    } else {
        format!("{DIM}▏{RESET}")
    };
    let left_head = format!("  {ACCENT}▍{RESET} {BOLD}{}{RESET}{caret}", state.pattern);
    let right_head = format!(
        "{DIM}◆ {mode}   {} match{}{RESET}",
        visible.len(),
        if visible.len() == 1 { "" } else { "es" }
    );
    let gap = term
        .saturating_sub(visible_width(&left_head) + visible_width(&right_head) + 2);
    lines.push(String::new());
    lines.push(format!("{left_head}{}{right_head}", " ".repeat(gap)));
    lines.push(String::new());

    // ── results, grouped under their file ────────────────────────────────
    let mut body: Vec<String> = Vec::new();
    if visible.is_empty() {
        body.push(format!(
            "   {DIM}{}{RESET}",
            if state.done { "nothing found" } else { "searching…" }
        ));
    }

    let mut shown_last = state.offset;
    let mut cur: Option<&Path> = None;
    for (slot, &i) in visible.iter().enumerate().skip(state.offset) {
        if body.len() >= BODY_ROWS.saturating_sub(1) {
            break;
        }
        let row = &state.rows[i];
        let path = row.hit.path();

        // Group heading whenever the file changes — and always for the first
        // visible row, so scrolling mid-file never hides which file you are in.
        if cur != Some(path) {
            if cur.is_some() {
                body.push(String::new());
            }
            let rel = path.strip_prefix(root).unwrap_or(path);
            let hits = visible
                .iter()
                .filter(|&&j| state.rows[j].hit.path() == path)
                .count();
            let head = format!("  {} {ACCENT}{BOLD}{}{RESET}", icon(path), rel.display());
            let chip = format!(
                "{DIM}{hits} · {}{RESET}",
                crate::app::human_size(row.size)
            );
            let gap = list_width
                .saturating_sub(visible_width(&head) + visible_width(&chip));
            body.push(format!("{head}{}{chip}", " ".repeat(gap)));
            cur = Some(path);
        }

        let selected = slot == state.selected;
        let marker = if selected {
            format!("{ACCENT}▸{RESET}")
        } else {
            " ".into()
        };
        match &row.hit {
            Hit::Line { line, text, .. } => {
                let t = highlight(text.trim_end(), &state.pattern);
                let t = if selected { t } else { format!("{DIM}{t}{RESET}") };
                body.push(format!("  {marker} {DIM}{line:>5}{RESET} {DIM}│{RESET} {t}"));
            }
            Hit::File { .. } => body.push(format!(
                "  {marker} {DIM}{:>5}   {}{RESET}",
                "",
                ago(row.mtime)
            )),
        }
        shown_last = slot + 1;
    }

    if shown_last < visible.len() {
        let next = visible[shown_last..]
            .iter()
            .map(|&i| state.rows[i].hit.path())
            .find(|p| Some(*p) != cur)
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let more = visible.len() - shown_last;
        body.push(match next {
            Some(n) => format!("   {DIM}↓ {more} more · next {n}{RESET}"),
            None => format!("   {DIM}↓ {more} more{RESET}"),
        });
    }
    body.truncate(BODY_ROWS);
    while body.len() < BODY_ROWS {
        body.push(String::new());
    }

    // ── preview beside the list, following the cursor ────────────────────
    let right = if show_preview {
        preview_lines(
            visible.get(state.selected).map(|&i| &state.rows[i]),
            term.saturating_sub(LEFT_WIDTH + 4),
            BODY_ROWS,
        )
    } else {
        Vec::new()
    };

    for (r, l) in body.into_iter().enumerate() {
        if !show_preview {
            lines.push(l);
            continue;
        }
        let p = right.get(r).cloned().unwrap_or_default();
        lines.push(format!("{} {DIM}│{RESET} {p}", fit(&l, LEFT_WIDTH)));
    }

    // ── key hints ────────────────────────────────────────────────────────
    let key = |k: &str, d: &str| format!("{ACCENT}{k}{RESET}{DIM} {d}{RESET}");
    lines.push(String::new());
    lines.push(if state.editing {
        format!(
            "   {}   {}   {}",
            key("type", "refine"),
            key("↵", "done"),
            key("↑↓", "move")
        )
    } else {
        format!(
            "   {}   {}   {}   {}   {}",
            key("/", "refine"),
            key("↵", "open"),
            key("e", "edit"),
            key("^p", "preview"),
            key("esc", "")
        )
    });
    lines
}

/// `rows` lines of the selected file centred on its match, cut to `width`.
/// Always returns exactly `rows` entries so the block height stays fixed.
fn preview_lines(row: Option<&Row>, width: usize, rows: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(row) = row else {
        return vec![String::new(); rows];
    };

    let focus = match &row.hit {
        Hit::Line { line, .. } => *line as usize,
        Hit::File { .. } => 1,
    };
    // Centre on the match without running off the top of the file.
    let start = focus.saturating_sub(rows / 2).max(1);

    match std::fs::read_to_string(row.hit.path()) {
        Ok(text) => {
            for (n, l) in text
                .lines()
                .enumerate()
                .map(|(i, l)| (i + 1, l))
                .skip(start - 1)
                .take(rows)
            {
                let body = l.trim_end();
                out.push(if n == focus && matches!(row.hit, Hit::Line { .. }) {
                    format!("{ACCENT}›{RESET}{DIM}{n:>5}{RESET}  {body}")
                } else {
                    format!("{DIM} {n:>5}  {body}{RESET}")
                });
            }
        }
        // Binary or unreadable: say so rather than showing nothing.
        Err(_) => out.push(format!("{DIM} (cannot preview){RESET}")),
    }

    out = out
        .into_iter()
        .map(|l| crate::inline::truncate(&l, width))
        .collect();
    out.resize(rows, String::new());
    out
}

/// Compact relative time: `now`, `4m`, `3h`, `2d`, else a date.
fn ago(t: SystemTime) -> String {
    let Ok(elapsed) = t.elapsed() else {
        return "now".into();
    };
    let s = elapsed.as_secs();
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        86400..=2591999 => format!("{}d", s / 86400),
        _ => format!("{}mo", s / 2592000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(is_file: bool, secs_ago: u64) -> Row {
        let hit = if is_file {
            Hit::File {
                path: PathBuf::from("a"),
            }
        } else {
            Hit::Line {
                path: PathBuf::from("a"),
                line: 1,
                text: "x".into(),
                context: false,
            }
        };
        Row {
            hit,
            size: 0,
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 - secs_ago),
        }
    }

    #[test]
    fn ago_formats_each_bucket() {
        let now = SystemTime::now();
        assert_eq!(ago(now), "now");
        assert_eq!(ago(now - Duration::from_secs(300)), "5m");
        assert_eq!(ago(now - Duration::from_secs(7200)), "2h");
        assert_eq!(ago(now - Duration::from_secs(172800)), "2d");
        // A future mtime (clock skew, or a file written this instant) must not
        // panic — elapsed() errors and we fall back to "now".
        assert_eq!(ago(now + Duration::from_secs(600)), "now");
    }

    #[test]
    fn insert_orders_files_first_then_newest() {
        let mut s = State::new("q".into());
        // Arrive deliberately out of order, as a parallel walk would.
        s.rows.push(row(false, 0));
        s.rows.push(row(true, 100));
        s.rows.sort_by(|a, b| {
            a.is_file()
                .cmp(&b.is_file())
                .reverse()
                .then_with(|| a.mtime.cmp(&b.mtime).reverse())
        });
        assert!(s.rows[0].is_file(), "files must sort ahead of content");
    }

    #[test]
    fn content_cap_hides_the_tail_until_expanded() {
        let mut s = State::new("q".into());
        for _ in 0..CONTENT_CAP + 20 {
            s.rows.push(row(false, 0));
        }
        assert_eq!(s.visible_rows().len(), CONTENT_CAP);
        s.expanded = true;
        assert_eq!(s.visible_rows().len(), CONTENT_CAP + 20);
    }

    #[test]
    fn clamp_scrolls_the_window_to_follow_the_cursor() {
        let mut s = State::new("q".into());
        for _ in 0..30 {
            s.rows.push(row(true, 0));
        }
        s.selected = 20;
        s.clamp();
        assert!(s.offset <= s.selected, "cursor must be at or after offset");
        assert!(
            s.selected < s.offset + WINDOW,
            "cursor must be inside the window"
        );

        s.selected = 0;
        s.clamp();
        assert_eq!(s.offset, 0, "scrolling back to the top resets the offset");
    }

    #[test]
    fn cursor_stays_pinned_to_the_top_until_the_user_moves() {
        let mut s = State::new("q".into());
        // Results streaming in must not drag the selection off row 0.
        for _ in 0..20 {
            s.insert(Hit::File { path: PathBuf::from("a") });
        }
        assert_eq!(s.selected, 0, "cursor drifted before the user moved it");

        // Once moved, it follows the item it was on. A File hit always sorts
        // ahead of Content hits, so its insert position is deterministic even
        // though these fixtures share an mtime.
        let mut s = State::new("q".into());
        for _ in 0..5 {
            s.insert(Hit::Line { path: PathBuf::from("a"), line: 1, text: "x".into(), context: false });
        }
        s.user_moved = true;
        s.selected = 3;
        s.insert(Hit::File { path: PathBuf::from("a") });
        assert_eq!(s.selected, 4, "cursor should follow its item after a move");
    }

    #[test]
    fn selection_includes_the_line_for_content_hits() {
        let root = Path::new("");
        assert_eq!(row(true, 0).selection(root), "a");
        assert_eq!(row(false, 0).selection(root), "a:1");
    }
}
