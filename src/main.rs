mod api;
mod app;
mod commands;
mod config;
mod tui;
mod ui;
mod update;
mod upload;
mod regenerate;
mod inline;
mod logo;
mod picker;
mod search;

use anyhow::Result;
use app::AuthKind;
use clap::{Args, Parser, Subcommand};

/// Flags shared by every search entry point.
#[derive(Args)]
pub struct SearchOpts {
    /// Skip dotfiles and dot-directories (they are searched by default)
    #[arg(long)]
    pub no_hidden: bool,
    /// Ignore .gitignore/.ignore rules
    #[arg(short = 'I', long)]
    pub no_ignore: bool,
    /// Stop after this many results
    #[arg(short = 'm', long)]
    pub limit: Option<usize>,
    /// Only files with this extension, e.g. -e rs
    #[arg(short = 'e', long)]
    pub ext: Option<String>,
    /// Separate results with NUL instead of newline (for xargs -0)
    #[arg(short = '0', long = "print0")]
    pub print0: bool,
    /// Lines of context around each content match
    #[arg(short = 'C', long, default_value_t = 1)]
    pub context: usize,
    /// Print results instead of opening the interactive picker
    #[arg(long)]
    pub no_input: bool,
}

#[derive(Parser)]
#[command(name = "cl", version, about = "🧪 Chloride — all-in-one DevOps utils CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the interactive file-manager TUI
    Tui,
    /// Create file(s)
    Touch { filenames: Vec<String> },
    /// Create a directory
    Mkdir {
        dirname: String,
        /// Create parent directories as needed
        #[arg(short, long)]
        parents: bool,
    },
    /// Remove a file or directory
    Rm {
        path: String,
        /// Remove directories and their contents recursively
        #[arg(short, long)]
        recursive: bool,
        /// Never prompt for confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Print working directory
    Pwd,
    /// Log in to Chloride (stores credentials in config)
    Login,
    /// Register a new Chloride account (auto-logs-in)
    Register,
    /// Log out and clear stored credentials
    Logout,
    /// Show current login status
    Whoami,
    /// Show your storage quota and usage
    Quota,
    /// Regenerate raw presigned URLs for an uploaded file
    Regenerate { file_id: Option<i64> },
    /// Find files by name and content
    ///
    /// Searches file contents (regex) by default. Use -f to match file names
    /// instead (substring, or a glob like '*.zip'). A pattern that cannot be a
    /// regex, such as '*.zip', falls back to a name search automatically.
    #[command(visible_alias = "f")]
    Find {
        /// Omit to open the file-manager TUI
        pattern: Option<String>,
        /// Directory to search (default: current)
        path: Option<String>,
        /// Match file names instead of contents
        #[arg(short = 'f', long, conflicts_with = "content")]
        files: bool,
        /// Match file contents (the default; use to force it for a glob-like pattern)
        #[arg(short = 'c', long)]
        content: bool,
        /// With -c: print only the names of files containing matches
        #[arg(short = 'l', long)]
        files_with_matches: bool,
        #[command(flatten)]
        opts: SearchOpts,
    },
    /// Print the Chloride logo
    Logo,
    /// Update cl to the latest release
    Update,
    /// Upload file(s) to Chloride
    Upload {
        /// Files to upload
        files: Vec<String>,
        /// Link expiry in seconds (default: interactive picker)
        #[arg(short, long)]
        expires_in: Option<i64>,
    },
}

fn main() -> Result<()> {
    let command = Cli::parse().command;

    // `cl update` does its own, louder check — everything else gets the
    // once-a-day silent one.
    if !matches!(command, Some(Command::Update)) {
        update::auto_update();
    }

    match command {
        None | Some(Command::Tui) => tui::launch(None, None),
        Some(Command::Touch { filenames }) => {
            for filename in &filenames {
                commands::touch_file(filename)?;
            }
            Ok(())
        }
        Some(Command::Mkdir { dirname, parents }) => commands::create_directory(&dirname, parents),
        Some(Command::Rm {
            path,
            recursive,
            force,
        }) => commands::remove_path(&path, recursive, force),
        Some(Command::Pwd) => commands::print_working_directory(),
        Some(Command::Login) => tui::launch(Some(AuthKind::Login), None),
        Some(Command::Register) => tui::launch(Some(AuthKind::Register), None),
        Some(Command::Logout) => commands::logout(),
        Some(Command::Whoami) => commands::whoami(),
        Some(Command::Quota) => commands::quota(),
        Some(Command::Regenerate { file_id }) => commands::regenerate(file_id),
        Some(Command::Find {
            pattern,
            path,
            files,
            content,
            files_with_matches,
            opts,
        }) => match pattern {
            None => tui::launch(None, Some(app::View::FileManager)),
            Some(pattern) => commands::search(
                pattern,
                path,
                files,
                content,
                files_with_matches,
                opts,
            ),
        },
        Some(Command::Logo) => commands::logo(),
        Some(Command::Update) => update::run_update(),
        Some(Command::Upload { files, expires_in }) => commands::upload(files, expires_in),
    }
}
