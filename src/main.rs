mod api;
mod app;
mod commands;
mod config;
mod tui;
mod ui;
mod upload;
mod regenerate;

use anyhow::Result;
use app::AuthKind;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cl", about = "🧪 Chloride — a tiny file manager")]
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
    match Cli::parse().command {
        None | Some(Command::Tui) => tui::launch(None),
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
        Some(Command::Login) => tui::launch(Some(AuthKind::Login)),
        Some(Command::Register) => tui::launch(Some(AuthKind::Register)),
        Some(Command::Logout) => commands::logout(),
        Some(Command::Whoami) => commands::whoami(),
        Some(Command::Quota) => commands::quota(),
        Some(Command::Regenerate { file_id }) => commands::regenerate(file_id),
        Some(Command::Upload { files, expires_in }) => commands::upload(files, expires_in),
    }
}
