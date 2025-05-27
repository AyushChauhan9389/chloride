use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use std::env;
use std::fs::{File, remove_file, create_dir, create_dir_all, remove_dir_all, metadata};
use std::io::{self, Write};
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "chloride",
    version = "0.1.0",
    about = "A simple file management tool with a Linux-like feel for Windows"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new file (like touch)
    Touch {
        /// Path to the file to create
        filename: String,
    },
    /// Remove files or directories (like rm)
    Rm {
        /// Path to the file or directory to remove
        path: String,
        /// Recursively remove directories and their contents
        #[arg(short, long)]
        recursive: bool,
        /// Force removal without prompting and ignore non-existent paths
        #[arg(short, long)]
        force: bool,
    },
    /// Print working directory (like pwd)
    Pwd,
    /// Create a directory (like mkdir)
    Mkdir {
        /// Path to the directory to create
        dirname: String,
        /// Create parent directories as needed (like mkdir -p)
        #[arg(short, long)]
        parents: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Touch { filename }) => {
            touch_file(&filename)?;
        }
        Some(Commands::Rm { path, recursive, force }) => {
            remove_path_cmd(&path, recursive, force)?;
        }
        Some(Commands::Pwd) => {
            print_working_directory()?;
        }
        Some(Commands::Mkdir { dirname, parents }) => {
            create_directory_cmd(&dirname, parents)?;
        }
        None => {
            show_dashboard()?;
        }
    }

    Ok(())
}

fn touch_file(filename: &str) -> Result<()> {
    if Path::new(filename).exists() {
        println!("📄 File '{}' already exists", filename);
    } else {
        File::create(filename)?;
        println!("✅ Created file '{}'", filename);
    }
    Ok(())
}

fn remove_path_cmd(path_str: &str, recursive: bool, force: bool) -> Result<()> {
    let path = Path::new(path_str);

    if !path.exists() {
        if force {
            return Ok(()); 
        } else {
            // Using eprintln for errors is good practice for CLI tools
            eprintln!("chloride: cannot remove '{}': No such file or directory", path_str);
            // To mimic shell command failure, you might want to exit with a non-zero status.
            // For now, returning Ok(()) after printing error to keep things simple at this level.
            // std::process::exit(1); could be used if main returned ()
            return Ok(()); 
        }
    }

    let path_metadata = match metadata(path) {
        Ok(meta) => meta,
        Err(e) => {
            if force {
                return Ok(()); 
            } else {
                eprintln!("chloride: cannot access '{}': {}", path_str, e);
                return Err(e.into());
            }
        }
    };
    
    if path_metadata.is_dir() {
        if !recursive {
            eprintln!("chloride: cannot remove '{}': Is a directory. Use -r or --recursive.", path_str);
            return Ok(());
        }

        if !force {
            print!(
                "🗑️  Are you sure you want to recursively delete directory '{}'? (y/N): ",
                path_str
            );
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !(input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes") {
                println!("❌ Deletion cancelled by user.");
                return Ok(());
            }
        }
        match remove_dir_all(path) {
            Ok(_) => println!("✅ Recursively deleted directory '{}'", path_str),
            Err(e) => {
                eprintln!("chloride: error deleting directory '{}': {}", path_str, e);
                return Err(e.into());
            }
        }
    } else { // It's a file (or symlink, etc.)
        if !force {
            print!(
                "🗑️  Are you sure you want to delete file '{}'? (y/N): ",
                path_str
            );
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !(input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes") {
                println!("❌ Deletion cancelled by user.");
                return Ok(());
            }
        }
        match remove_file(path) {
            Ok(_) => println!("✅ Deleted file '{}'", path_str),
            Err(e) => {
                eprintln!("chloride: error deleting file '{}': {}", path_str, e);
                return Err(e.into());
            }
        }
    }

    Ok(())
}


fn print_working_directory() -> Result<()> {
    match env::current_dir() {
        Ok(path) => {
            println!("{}", path.display());
        }
        Err(e) => {
            eprintln!("chloride: error getting current directory: {}", e);
            return Err(anyhow!("Failed to get current directory: {}", e));
        }
    }
    Ok(())
}

fn create_directory_cmd(dirname: &str, parents: bool) -> Result<()> {
    let path = Path::new(dirname);
    if path.exists() {
        if path.is_dir() {
            // Don't print if it's a part of a -p chain that already exists
            // However, for a direct `mkdir existing_dir`, this message is fine.
            // `create_dir_all` handles existing parent directories silently.
            // `create_dir` will error if it exists. This check is more for user feedback.
            println!("📁 Directory '{}' already exists", dirname);
        } else {
            eprintln!("chloride: cannot create directory '{}': File exists", dirname);
        }
        return Ok(()); // Or an error if File exists to mimic `mkdir` more closely
    }

    if parents {
        match create_dir_all(dirname) {
            Ok(_) => println!("✅ Created directory (and any parents) '{}'", dirname),
            Err(e) => {
                eprintln!("chloride: error creating directory '{}' with parents: {}", dirname, e);
                return Err(e.into());
            }
        }
    } else {
        match create_dir(dirname) {
            Ok(_) => println!("✅ Created directory '{}'", dirname),
            Err(e) => {
                eprintln!("chloride: error creating directory '{}': {}", dirname, e);
                return Err(e.into());
            }
        }
    }
    Ok(())
}

fn show_dashboard() -> Result<()> {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                        🧪 CHLORIDE                           ║");
    println!("║          File Management Tool (Linux Style on Win)           ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║                                                              ║");
    println!("║  📁 Available Commands:                                      ║");
    println!("║                                                              ║");
    println!("║  📄 Create File (touch):                                     ║");
    println!("║     chloride touch <filename>                               ║");
    println!("║     Example: chloride touch myfile.txt                      ║");
    println!("║                                                              ║");
    println!("║  🗑️  Remove Files/Directories (rm):                           ║");
    println!("║     chloride rm <path> [-r | --recursive] [-f | --force]    ║");
    println!("║     Example: chloride rm oldfile.txt                        ║");
    println!("║     Example: chloride rm -r old_directory                   ║");
    println!("║     Example: chloride rm -rf path/to/something              ║");
    println!("║                                                              ║");
    println!("║  🧭 Print Working Directory (pwd):                           ║");
    println!("║     chloride pwd                                            ║");
    println!("║                                                              ║");
    println!("║  ➕ Create Directory (mkdir):                                 ║");
    println!("║     chloride mkdir <dirname> [-p | --parents]               ║");
    println!("║     Example: chloride mkdir myfolder                        ║");
    println!("║     Example: chloride mkdir -p path/to/myfolder             ║");
    println!("║                                                              ║");
    println!("║  ❓ Help:                                                     ║");
    println!("║     chloride --help                                          ║");
    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("💡 Tip: Run 'chloride --help' for more detailed information");

    Ok(())
}