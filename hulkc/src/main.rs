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
        Command::Run { file } => cmd_run(&file),
        Command::Parse { file } => cmd_parse(&file),
    }
}

fn cmd_run(file: &str) -> Result<()> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("cannot read '{file}': {e}"))?;

    let ast = hulk_parser::parse(&source)
        .map_err(|e| anyhow::anyhow!("parse error: {e}"))?;

    hulk_semantic::analyze(&ast).map_err(|errs| {
        let msg = errs.iter().map(|e| format!("  {e}")).collect::<Vec<_>>().join("\n");
        anyhow::anyhow!("semantic errors:\n{msg}")
    })?;

    let ir = hulk_ir::lower_program(&ast);

    hulk_vm::Vm::run_program(ir)
        .map_err(|e| anyhow::anyhow!("runtime error: {e}"))
}

fn cmd_parse(file: &str) -> Result<()> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("cannot read '{file}': {e}"))?;

    let ast = hulk_parser::parse(&source)
        .map_err(|e| anyhow::anyhow!("parse error: {e}"))?;

    println!("{ast:#?}");
    Ok(())
}
