//! `hulkc` — command-line driver for the HULK compiler.

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hulkc", version, about = "HULK compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile and execute a HULK source file.
    Run {
        /// Path to a `.hulk` source file.
        file: String,
    },
    /// Parse a HULK source file and dump the AST.
    Parse {
        /// Path to a `.hulk` source file.
        file: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Run { file } => {
            println!("[skeleton] would run: {file}");
        }
        Command::Parse { file } => {
            println!("[skeleton] would parse: {file}");
        }
    }

    Ok(())
}
