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
                self.select_name(name);
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
                self.select_name(name);
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

    fn select_name(&mut self, name: &str) {
        if let Some(index) = self.entries.iter().position(|e| e.name == name) {
            self.selected = index;
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
