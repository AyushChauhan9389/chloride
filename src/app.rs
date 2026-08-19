use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::api::RemoteFile;
use crate::config::Config;

const DEFAULT_UPLOAD_EXPIRES_IN: i64 = 60 * 60 * 24 * 7;

/// Which list the TUI is showing.
#[derive(Clone, Copy, PartialEq)]
pub enum View {
    /// Remote files from the Chloride API.
    Files,
    /// Local file manager.
    FileManager,
}

/// What the UI is currently doing.
pub enum Mode {
    Browse,
    /// Live search over the current directory. State lives in `App::search`
    /// because the running walk owns a channel that cannot be cloned around
    /// the way `Mode` values are.
    Search,
    Input { kind: InputKind, buffer: String },
    Confirm { name: String, is_dir: bool },
    ConfirmRemoteDelete { id: i64, name: String },
    Message { title: String, text: String },
    Auth(AuthForm),
    Quota(Option<Result<crate::api::StorageInfo, String>>),
}

#[derive(Clone, Copy)]
pub enum InputKind {
    Touch,
    Mkdir,
}

impl InputKind {
    pub fn title(self) -> &'static str {
        match self {
            InputKind::Touch => " New file (touch) ",
            InputKind::Mkdir => " New directory (mkdir) ",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum AuthKind {
    Login,
    Register,
}

impl AuthKind {
    pub fn title(self) -> &'static str {
        match self {
            AuthKind::Login => " Login ",
            AuthKind::Register => " Register ",
        }
    }
}

pub struct AuthForm {
    pub kind: AuthKind,
    pub email: String,
    pub password: String,
    pub confirm: String,
    pub active: usize,
    pub error: Option<String>,
}

impl AuthForm {
    pub fn new(kind: AuthKind) -> Self {
        Self {
            kind,
            email: String::new(),
            password: String::new(),
            confirm: String::new(),
            active: 0,
            error: None,
        }
    }

    pub fn field_count(&self) -> usize {
        match self.kind {
            AuthKind::Login => 2,
            AuthKind::Register => 3,
        }
    }

    pub fn active_buffer(&mut self) -> &mut String {
        match self.active {
            0 => &mut self.email,
            1 => &mut self.password,
            _ => &mut self.confirm,
        }
    }
}

/// A single row in the local directory listing.
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// True for the synthetic ".." entry.
    pub parent: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

pub struct Status {
    pub message: String,
    pub kind: StatusKind,
}

/// A live search driven from the TUI.
pub struct SearchState {
    pub query: String,
    pub hits: Vec<crate::search::Hit>,
    pub selected: usize,
    /// The running walk. Dropping it cancels the worker threads, which is how
    /// a stale search is stopped the instant the query changes — otherwise
    /// every keystroke would leave a walk burning CPU in the background.
    pub running: Option<crate::search::Search>,
    /// True once the walk has finished, so the header can stop spinning.
    pub done: bool,
    /// Whether this walk is matching names rather than contents. Inferred from
    /// the pattern, exactly as the inline picker and the CLI do.
    pub mode_names: bool,
    /// Show content hits past [`crate::picker::CONTENT_CAP`].
    pub expanded: bool,
    /// Until the user moves, the cursor stays pinned to the top row; after
    /// that it follows the item it was on as results stream in around it.
    pub user_moved: bool,
    /// One stat per path, shared by every content hit in the same file. Keeps
    /// the sort stable without re-stat-ing a file per hit.
    meta: std::collections::HashMap<PathBuf, std::time::SystemTime>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            hits: Vec::new(),
            selected: 0,
            running: None,
            done: false,
            mode_names: false,
            expanded: false,
            user_moved: false,
            meta: std::collections::HashMap::new(),
        }
    }

    /// Insert keeping the list in [`crate::search::compare_hits`] order, which
    /// is the same order the inline picker shows: name hits first, newest file
    /// first, then path and line. A parallel walk arrives out of order, so
    /// appending would make the list reshuffle as it is read.
    fn insert(&mut self, hit: crate::search::Hit) {
        let mtime = *self
            .meta
            .entry(hit.path().to_path_buf())
            .or_insert_with_key(|p| {
                fs::metadata(p)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });
        let pos = self
            .hits
            .binary_search_by(|probe| {
                let probe_mtime = self
                    .meta
                    .get(probe.path())
                    .copied()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                crate::search::compare_hits((probe, probe_mtime), (&hit, mtime))
            })
            .unwrap_or_else(|e| e);
        self.hits.insert(pos, hit);

        // Only chase the item once the user has taken control of the cursor;
        // otherwise a streaming result inserted above row 0 would drag the
        // selection down while they are still reading.
        if self.user_moved && pos <= self.selected && self.hits.len() > 1 {
            self.selected += 1;
        }
    }

    fn file_count(&self) -> usize {
        self.hits
            .iter()
            .filter(|h| matches!(h, crate::search::Hit::File { .. }))
            .count()
    }

    pub fn content_count(&self) -> usize {
        self.hits.len() - self.file_count()
    }

    /// How many rows are actually selectable, honouring the content cap.
    pub fn visible_len(&self) -> usize {
        let content = if self.expanded {
            self.content_count()
        } else {
            self.content_count().min(crate::picker::CONTENT_CAP)
        };
        self.file_count() + content
    }

    /// The rows to draw, honouring the content cap.
    pub fn visible(&self) -> &[crate::search::Hit] {
        &self.hits[..self.visible_len()]
    }

    /// The selected hit, if any.
    pub fn current(&self) -> Option<&crate::search::Hit> {
        self.visible().get(self.selected)
    }

    pub fn move_down(&mut self) {
        self.user_moved = true;
        self.selected = (self.selected + 1).min(self.visible_len().saturating_sub(1));
    }

    pub fn move_up(&mut self) {
        self.user_moved = true;
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move to the first hit of the next (`forward`) or previous distinct file,
    /// skipping every remaining match in the current one — the same gesture the
    /// inline picker binds to the plain arrow keys.
    pub fn move_file(&mut self, forward: bool) {
        self.user_moved = true;
        let visible = self.visible_len();
        let Some(current) = self.hits.get(self.selected).map(|h| h.path().to_path_buf()) else {
            return;
        };
        let mut next = if forward {
            (self.selected + 1..visible).find(|&i| self.hits[i].path() != current)
        } else {
            (0..self.selected).rev().find(|&i| self.hits[i].path() != current)
        };
        // A reverse search lands on the previous file's last hit; walk back to
        // its first so either direction selects the start of a file group.
        if !forward {
            while let Some(i) = next {
                if i > 0 && self.hits[i - 1].path() == self.hits[i].path() {
                    next = Some(i - 1);
                } else {
                    break;
                }
            }
        }
        if let Some(i) = next {
            self.selected = i;
        }
    }
}

pub struct App {
    pub running: bool,
    pub view: View,
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub remote_files: Vec<RemoteFile>,
    pub selected: usize,
    pub mode: Mode,
    pub status: Status,
    pub config: Config,
    pub search: Option<SearchState>,
}

impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let config = Config::load().unwrap_or_default();
        let mut app = Self {
            running: true,
            view: View::Files,
            cwd,
            entries: Vec::new(),
            remote_files: Vec::new(),
            selected: 0,
            mode: Mode::Browse,
            status: Status {
                message: "Welcome to Chloride".into(),
                kind: StatusKind::Info,
            },
            config,
            search: None,
        };
        app.refresh();
        app
    }

    // --- Listing ---

    fn active_len(&self) -> usize {
        match self.view {
            View::FileManager => self.entries.len(),
            View::Files => self.remote_files.len(),
        }
    }

    pub fn refresh(&mut self) {
        match self.view {
            View::FileManager => self.refresh_local(),
            View::Files => self.refresh_remote(),
        }
        if self.selected >= self.active_len() {
            self.selected = self.active_len().saturating_sub(1);
        }
    }

    fn refresh_local(&mut self) {
        let mut entries: Vec<Entry> = Vec::new();

        if self.cwd.parent().is_some() {
            entries.push(Entry {
                name: "..".into(),
                is_dir: true,
                size: 0,
                parent: true,
            });
        }

        match fs::read_dir(&self.cwd) {
            Ok(read) => {
                let mut items: Vec<Entry> = read
                    .filter_map(|res| res.ok())
                    .map(|dir_entry| {
                        let meta = dir_entry.metadata().ok();
                        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                        Entry {
                            name: dir_entry.file_name().to_string_lossy().into_owned(),
                            is_dir,
                            size: meta.map(|m| m.len()).unwrap_or(0),
                            parent: false,
                        }
                    })
                    .collect();

                items.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                entries.extend(items);
            }
            Err(e) => {
                self.set_error(format!("Cannot read directory: {e}"));
            }
        }

        self.entries = entries;
    }

    fn refresh_remote(&mut self) {
        // list_files auto-refreshes the access token on 401, so we no longer
        // need to handle expiry here — only surface hard errors.
        match crate::api::list_files(&mut self.config) {
            Ok(files) => {
                let n = files.len();
                self.remote_files = files;
                self.set_info(format!("{n} file(s)"));
            }
            Err(e) => {
                self.remote_files.clear();
                let msg = e.to_string();
                if msg.contains("Not logged in") || msg.contains("Session expired") {
                    self.set_error(format!("{msg} Press L to log in."));
                } else {
                    self.set_error(format!("Failed to load files: {e}"));
                }
            }
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn selected_file(&self) -> Option<&RemoteFile> {
        self.remote_files.get(self.selected)
    }

    pub fn switch_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.selected = 0;
            self.refresh();
        }
    }

    // --- Navigation ---

    pub fn move_down(&mut self) {
        let len = self.active_len();
        if len > 0 {
            self.selected = (self.selected + 1).min(len - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        self.selected = self.active_len().saturating_sub(1);
    }

    pub fn enter(&mut self) {
        match self.view {
            View::FileManager => self.enter_local(),
            View::Files => self.enter_remote(),
        }
    }

    fn enter_local(&mut self) {
        let target = self
            .selected_entry()
            .map(|e| (e.parent, e.is_dir, e.name.clone()));

        if let Some((parent, is_dir, name)) = target {
            if parent {
                self.go_up();
            } else if is_dir {
                let path = self.cwd.join(&name);
                self.change_dir(path);
            } else {
                self.set_info(format!("'{name}' is a file"));
            }
        }
    }

    fn enter_remote(&mut self) {
        if let Some(f) = self.selected_file() {
            self.set_info(format!(
                "{} · {} · {}",
                f.name,
                human_size(f.size as u64),
                f.created_at
            ));
        }
    }

    pub fn regenerate_selected_file_url(&mut self) {
        if self.view != View::Files {
            return;
        }
        let Some(file) = self.selected_file().cloned() else {
            self.set_error("No file selected");
            return;
        };

        match crate::api::regenerate_urls(&mut self.config, file.id) {
            Ok(urls) => {
                if let Some(selected) = self.remote_files.get_mut(self.selected) {
                    selected.short_download_url = urls.short_download_url.clone();
                    selected.short_view_url = urls.short_view_url.clone();
                    selected.original_view_url = urls.view_url.clone();
                    selected.original_download_url = urls.download_url.clone();
                }
                self.set_success(format!("Regenerated short download URL for '{}'", file.name));
            }
            Err(e) => self.set_error(format!("Failed to regenerate URL: {e}")),
        }
    }

    pub fn copy_selected_short_download_url(&mut self) {
        let Some(url) = self.selected_short_download_url() else {
            self.set_error("No short download URL. Press R to regenerate.");
            return;
        };

        match copy_to_clipboard(&url) {
            Ok(()) => self.set_success("Copied short download URL"),
            Err(_) => {
                self.mode = Mode::Message {
                    title: " Short Download URL ".into(),
                    text: url,
                };
            }
        }
    }

    pub fn download_selected_file(&mut self) {
        let Some(file) = self.selected_file().cloned() else {
            self.set_error("No file selected");
            return;
        };
        let Some(url) = file.short_download_url.clone() else {
            self.set_error("No short download URL. Press R to regenerate.");
            return;
        };

        match download_to_current_dir(&url, &file.name) {
            Ok(path) => self.set_success(format!("Downloaded to {}", path.display())),
            Err(e) => self.set_error(format!("Failed to download: {e}")),
        }
    }

    pub fn confirm_delete_selected_remote_file(&mut self) {
        if self.view != View::Files {
            return;
        }
        let Some(file) = self.selected_file() else {
            self.set_error("No file selected");
            return;
        };
        self.mode = Mode::ConfirmRemoteDelete {
            id: file.id,
            name: file.name.clone(),
        };
    }

    pub fn delete_remote_file(&mut self, id: i64, name: &str) {
        match crate::api::delete_file(&mut self.config, id) {
            Ok(()) => {
                self.remote_files.retain(|file| file.id != id);
                if self.selected >= self.remote_files.len() {
                    self.selected = self.remote_files.len().saturating_sub(1);
                }
                self.set_success(format!("Deleted remote file '{name}'"));
            }
            Err(e) => self.set_error(format!("Failed to delete remote file: {e}")),
        }
    }

    pub fn upload_selected_local_file(&mut self) {
        if self.view != View::FileManager {
            return;
        }

        let Some(entry) = self.selected_entry() else {
            self.set_error("No file selected");
            return;
        };
        if entry.parent || entry.is_dir {
            self.set_error("Select a file to upload");
            return;
        }

        let name = entry.name.clone();
        let path = self.cwd.join(&name);
        let content_type = mime_for(&name);
        let config = Arc::new(Mutex::new(self.config.clone()));
        let progress = Arc::new(|_, _| {}) as crate::api::ProgressFn;

        let result = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime.block_on(crate::api::upload_file(
                &config,
                &path,
                &name,
                &content_type,
                DEFAULT_UPLOAD_EXPIRES_IN,
                progress,
            )),
            Err(e) => Err(e.into()),
        };

        if let Ok(cfg) = config.lock() {
            self.config = cfg.clone();
            let _ = self.config.save();
        }

        match result {
            Ok(upload) => {
                self.mode = Mode::Message {
                    title: " Uploaded ".into(),
                    text: upload.short_download_url,
                };
                self.set_success(format!("Uploaded '{name}'"));
            }
            Err(e) => self.set_error(format!("Failed to upload '{name}': {e}")),
        }
    }

    fn selected_short_download_url(&self) -> Option<String> {
        if self.view != View::Files {
            return None;
        }
        self.selected_file()?.short_download_url.clone()
    }

    pub fn go_up(&mut self) {
        if self.view != View::FileManager {
            return;
        }
        if let Some(parent) = self.cwd.parent() {
            let parent = parent.to_path_buf();
            self.change_dir(parent);
        }
    }

    fn change_dir(&mut self, path: PathBuf) {
        self.cwd = path;
        self.selected = 0;
        self.refresh();
    }

    // --- File operations (local file manager only) ---

    pub fn create_file(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let path = self.cwd.join(name);
        if path.exists() {
            self.set_info(format!("'{name}' already exists"));
            return;
        }
        match fs::File::create(&path) {
            Ok(_) => {
                self.set_success(format!("Created file '{name}'"));
                self.refresh();
                self.select_by_name(name);
            }
            Err(e) => self.set_error(format!("Failed to create file: {e}")),
        }
    }

    pub fn create_dir(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let path = self.cwd.join(name);
        if path.exists() {
            self.set_info(format!("'{name}' already exists"));
            return;
        }
        match fs::create_dir_all(&path) {
            Ok(_) => {
                self.set_success(format!("Created directory '{name}'"));
                self.refresh();
                self.select_by_name(name);
            }
            Err(e) => self.set_error(format!("Failed to create directory: {e}")),
        }
    }

    pub fn delete(&mut self, name: &str, is_dir: bool) {
        let path = self.cwd.join(name);
        let result = if is_dir {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Ok(_) => {
                self.set_success(format!("Deleted '{name}'"));
                self.refresh();
            }
            Err(e) => self.set_error(format!("Failed to delete '{name}': {e}")),
        }
    }

    pub fn select_by_name(&mut self, name: &str) {
        if let Some(index) = self.entries.iter().position(|e| e.name == name) {
            self.selected = index;
        }
    }

    // --- Search ---

    /// Restart the walk for the current query, cancelling any previous one.
    pub fn restart_search(&mut self) {
        let Some(state) = self.search.as_mut() else {
            return;
        };
        // Dropping the old Search sets its cancel flag.
        state.running = None;
        state.hits.clear();
        state.meta.clear();
        state.selected = 0;
        state.user_moved = false;
        state.expanded = false;
        state.done = false;

        if state.query.is_empty() {
            return;
        }
        // Same plan the inline picker and the CLI use: contents by default,
        // names for a glob, capped at search::DEFAULT_LIMIT.
        let query = crate::search::plan(state.query.clone(), self.cwd.clone());
        state.mode_names = query.kind == crate::search::Kind::Files;
        match crate::search::spawn(query) {
            Ok(search) => state.running = Some(search),
            // An in-progress regex like "a(" is not an error worth shouting
            // about; the user is still typing.
            Err(_) => state.running = None,
        }
    }

    /// Move any results the walk has produced into the visible list.
    pub fn drain_search(&mut self) {
        let Some(state) = self.search.as_mut() else {
            return;
        };
        let Some(search) = state.running.as_ref() else {
            return;
        };
        // Bounded per frame so a firehose cannot starve the redraw. The drain
        // itself detects the closed channel; probing again afterwards would
        // race a late hit and drop it.
        let mut drained = Vec::new();
        for _ in 0..512 {
            match search.hits.try_recv() {
                Ok(hit) => drained.push(hit),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    state.done = true;
                    break;
                }
            }
        }
        for hit in drained {
            state.insert(hit);
        }
    }

    // --- Status helpers ---

    pub fn set_info(&mut self, message: impl Into<String>) {
        self.status = Status {
            message: message.into(),
            kind: StatusKind::Info,
        };
    }

    pub fn set_success(&mut self, message: impl Into<String>) {
        self.status = Status {
            message: message.into(),
            kind: StatusKind::Success,
        };
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = Status {
            message: message.into(),
            kind: StatusKind::Error,
        };
    }
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
    } else if lower.ends_with(".apk") {
        "application/vnd.android.package-archive".into()
    } else if lower.ends_with(".mp4") {
        "video/mp4".into()
    } else if lower.ends_with(".mp3") {
        "audio/mpeg".into()
    } else {
        "application/octet-stream".into()
    }
}

fn download_to_current_dir(url: &str, name: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = unique_download_path(std::env::current_dir()?.join(name));
    let mut response = reqwest::blocking::get(url)?.error_for_status()?;
    let mut file = std::fs::File::create(&path)?;
    std::io::copy(&mut response, &mut file)?;
    Ok(path)
}

fn unique_download_path(path: std::path::PathBuf) -> std::path::PathBuf {
    if !path.exists() {
        return path;
    }

    let parent = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    let extension = path.extension().map(|s| s.to_string_lossy().into_owned());

    for i in 1.. {
        let file_name = match &extension {
            Some(ext) => format!("{stem} ({i}).{ext}"),
            None => format!("{stem} ({i})"),
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!()
}

#[cfg(target_os = "windows")]
fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("powershell")
        .args(["-NoProfile", "-Command", "Set-Clipboard"])
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "Set-Clipboard failed"))
    }
}

#[cfg(target_os = "macos")]
fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let has_command = |name: &str| {
        Command::new("sh")
            .args(["-c", &format!("command -v {name}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    };

    let mut command = if has_command("wl-copy") {
        Command::new("wl-copy")
    } else if has_command("xclip") {
        let mut command = Command::new("xclip");
        command.args(["-selection", "clipboard"]);
        command
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "install wl-copy or xclip to copy to clipboard",
        ));
    };

    let mut child = command.stdin(Stdio::piped()).spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "clipboard command failed"))
    }
}

/// Human-readable byte size.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::Hit;
    use std::path::Path;

    fn line(path: &str, n: u64) -> Hit {
        Hit::Line {
            path: PathBuf::from(path),
            line: n,
            text: "x".into(),
            context: false,
        }
    }

    /// Feed hits in the order given, as a parallel walk would.
    fn state_with(hits: Vec<Hit>) -> SearchState {
        let mut s = SearchState::new();
        for h in hits {
            s.insert(h);
        }
        s
    }

    #[test]
    fn tui_search_orders_results_like_the_inline_picker() {
        // Content hits arrive out of order; the list must read top to bottom
        // per file, with name hits ahead of them — same as the picker.
        let s = state_with(vec![
            line("b.rs", 5),
            Hit::File { path: PathBuf::from("z.rs") },
            line("b.rs", 1),
            line("a.rs", 2),
        ]);
        let shown: Vec<String> = s
            .hits
            .iter()
            .map(|h| match h {
                Hit::File { path } => path.display().to_string(),
                Hit::Line { path, line, .. } => format!("{}:{line}", path.display()),
            })
            .collect();
        assert_eq!(shown[0], "z.rs", "name hits sort first: {shown:?}");
        let b: Vec<&String> = shown.iter().filter(|s| s.starts_with("b.rs")).collect();
        assert_eq!(b, vec!["b.rs:1", "b.rs:5"], "{shown:?}");
    }

    #[test]
    fn tui_search_caps_content_hits_until_expanded() {
        let hits: Vec<Hit> = (1..=crate::picker::CONTENT_CAP as u64 + 20)
            .map(|n| line("a.rs", n))
            .collect();
        let mut s = state_with(hits);
        assert_eq!(s.visible_len(), crate::picker::CONTENT_CAP);
        assert_eq!(s.visible().len(), crate::picker::CONTENT_CAP);
        s.expanded = true;
        assert_eq!(s.visible_len(), crate::picker::CONTENT_CAP + 20);
    }

    #[test]
    fn tui_search_arrows_hop_files_and_ctrl_moves_lines() {
        let mut s = state_with(vec![
            line("a.rs", 1),
            line("a.rs", 2),
            line("a.rs", 3),
            line("b.rs", 1),
            line("b.rs", 2),
        ]);
        // From a.rs:2, a file hop skips the rest of a.rs.
        s.selected = 1;
        s.move_file(true);
        assert_eq!(s.current().unwrap().path(), Path::new("b.rs"));
        // Back to the *first* hit of the previous file, not its last.
        s.move_file(false);
        assert_eq!(s.selected, 0);
        // Ends are sticky rather than wrapping.
        s.move_file(false);
        assert_eq!(s.selected, 0);
        // Line movement is one row at a time.
        s.move_down();
        assert_eq!(s.selected, 1);
        s.move_up();
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn streaming_results_do_not_drag_the_cursor_until_the_user_moves() {
        let mut s = SearchState::new();
        for n in (1..=10).rev() {
            s.insert(line("a.rs", n));
        }
        assert_eq!(s.selected, 0, "cursor drifted before the user moved it");

        // Once moved, it follows the item it was on.
        let mut s = state_with(vec![line("b.rs", 2), line("b.rs", 3)]);
        s.user_moved = true;
        s.selected = 0;
        s.insert(line("b.rs", 1));
        assert_eq!(s.selected, 1, "cursor should follow its item after a move");
    }
}
