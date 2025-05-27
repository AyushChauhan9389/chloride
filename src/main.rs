use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs::{File, remove_file};
use std::path::Path;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "chloride", version = "0.1.0", about = "A simple file management tool")]
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
    /// Remove a file
    Rm {
        /// Path to the file to remove
        filename: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Some(Commands::Touch { filename }) => {
            touch_file(&filename)?;
        }
        Some(Commands::Rm { filename }) => {
            remove_file_cmd(&filename)?;
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

fn remove_file_cmd(filename: &str) -> Result<()> {
    if !Path::new(filename).exists() {
        println!("❌ File '{}' does not exist", filename);
        return Ok(());
    }
    
    print!("🗑️  Are you sure you want to delete '{}'? (y/N): ", filename);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    if input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes" {
        remove_file(filename)?;
        println!("✅ Deleted file '{}'", filename);
    } else {
        println!("❌ Deletion cancelled");
    }
    
    Ok(())
}

fn show_dashboard() -> Result<()> {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                        🧪 CHLORIDE                           ║");
    println!("║                   File Management Tool                      ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║                                                              ║");
    println!("║  📁 Available Commands:                                      ║");
    println!("║                                                              ║");
    println!("║  📄 Create File:                                             ║");
    println!("║     chloride touch <filename>                               ║");
    println!("║     Example: chloride touch myfile.txt                      ║");
    println!("║                                                              ║");
    println!("║  🗑️  Remove File:                                            ║");
    println!("║     chloride rm <filename>                                   ║");
    println!("║     Example: chloride rm oldfile.txt                        ║");
    println!("║                                                              ║");
    println!("║  ❓ Help:                                                     ║");
    println!("║     chloride --help                                          ║");
    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("💡 Tip: Run 'chloride --help' for more detailed information");
    
    Ok(())
}