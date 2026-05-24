//! `hulkc` — command-line driver for the HULK compiler.

use anyhow::Context;
use anyhow::Result;
use clap::{Parser, Subcommand};
use hulk_parser::ParseError;

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
            let source = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read source file: {file}"))?;
            let program = hulk_parser::parse(&source)
                .map_err(|e| anyhow::anyhow!(format_parse_error(&source, e)))?;

            match hulk_semantic::analyze(&program) {
                Ok(_ctx) => {
                    // Backend not yet implemented.
                    eprintln!("[ok] semantic analysis passed");
                    eprintln!("[todo] IR lowering + VM execution not implemented yet");
                }
                Err(errs) => {
                    for err in errs {
                        eprintln!("{err} @ {}", err.span());
                    }
                    anyhow::bail!("semantic analysis failed");
                }
            }
        }
        Command::Parse { file } => {
            let source = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read source file: {file}"))?;
            let program = hulk_parser::parse(&source)
                .map_err(|e| anyhow::anyhow!(format_parse_error(&source, e)))?;
            println!("{program:#?}");
        }
    }

    Ok(())
}

fn format_parse_error(source: &str, err: ParseError<'_>) -> String {
    // Minimal formatting: include the parse error and, when possible, a span.
    // We keep spans as byte offsets; caller can map to line/col later.
    match &err {
        lalrpop_util::ParseError::InvalidToken { location }
        | lalrpop_util::ParseError::UnrecognizedEof { location, .. } => {
            format!("parse error at {location}: {err:?}")
        }
        lalrpop_util::ParseError::UnrecognizedToken { token, .. } => {
            let (lo, _tok, hi) = token;
            let snippet = source.get(*lo..(*hi).min(source.len())).unwrap_or("");
            format!("parse error at {lo}..{hi}: {err:?}\n  near: {snippet:?}")
        }
        lalrpop_util::ParseError::ExtraToken { token } => {
            let (lo, _tok, hi) = token;
            let snippet = source.get(*lo..(*hi).min(source.len())).unwrap_or("");
            format!("extra token at {lo}..{hi}: {err:?}\n  near: {snippet:?}")
        }
        lalrpop_util::ParseError::User { error } => {
            format!("lex error at {}..{}: {error}", error.start, error.end)
        }
    }
}
