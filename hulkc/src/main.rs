//! `hulk` — command-line driver for the HULK compiler.
//!
//! Implements the `matcom/compilers` interface contract:
//!
//! * `hulk <file.hulk>` validates the program (lexing, parsing, semantic
//!   analysis). On success it emits an executable `./output` and exits `0`.
//!   On failure it prints diagnostics to stderr, one per line, in the format
//!   `(line,col) TYPE: message` (TYPE ∈ `LEXICAL` | `SYNTACTIC` | `SEMANTIC`)
//!   and exits with the code of the most fundamental error type
//!   (1 = lexical, 2 = syntactic, 3 = semantic).
//! * `hulk exec <file|->` runs the full pipeline (including the VM) on a
//!   program read from a path or stdin. This is the internal entry point that
//!   the generated `./output` wrapper re-invokes; it is not part of the public
//!   interface.
//! * `hulk parse <file>` dumps the AST (development aid).

use std::io::Read;
use std::process::exit;

use hulk_parser::ParseError;

/// Exit code reported for a lexical error.
const EXIT_LEXICAL: i32 = 1;
/// Exit code reported for a syntactic error.
const EXIT_SYNTACTIC: i32 = 2;
/// Exit code reported for a semantic error.
const EXIT_SEMANTIC: i32 = 3;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        // Subcommands (check these first to avoid confusion with filenames)
        Some("run") => match args.get(2) {
            Some(file) => cmd_compile(file),
            None => usage_error(),
        },
        Some("exec") => {
            let target = args.get(2).map(String::as_str).unwrap_or("-");
            cmd_exec(target);
        }
        Some("parse") => match args.get(2) {
            Some(file) => cmd_parse(file),
            None => usage_error(),
        },
        // Public interface (matcom/compilers): ./hulk <file>
        Some(file) if !file.starts_with('-') => cmd_compile(file),
        _ => usage_error(),
    }
}

/// Print a usage message to stderr and exit with the conventional code 64.
fn usage_error() -> ! {
    eprintln!("usage: hulk <file.hulk>");
    exit(64);
}

/// Read a source file or exit with an I/O diagnostic.
fn read_source(file: &str) -> String {
    match std::fs::read_to_string(file) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("(0,0) SEMANTIC: cannot read source file `{file}`: {err}");
            exit(EXIT_SEMANTIC);
        }
    }
}

/// Compile mode: validate `file`, and on success emit `./output`.
fn cmd_compile(file: &str) {
    let source = read_source(file);

    let ast = match hulk_parser::parse(&source) {
        Ok(ast) => ast,
        Err(err) => report_parse_error(&source, err),
    };

    if let Err(errors) = hulk_semantic::analyze(&ast) {
        for error in &errors {
            let (line, col) = line_col(&source, error.span().lo as usize);
            eprintln!("({line},{col}) SEMANTIC: {error}");
        }
        exit(EXIT_SEMANTIC);
    }

    write_output(&source);
}

/// Internal exec mode: run the full pipeline (VM included) on a program read
/// from `target` (a path, or `-` for stdin). Used by the generated `./output`.
fn cmd_exec(target: &str) {
    let source = if target == "-" {
        let mut buffer = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut buffer) {
            eprintln!("(0,0) SEMANTIC: cannot read program from stdin: {err}");
            exit(EXIT_SEMANTIC);
        }
        buffer
    } else {
        read_source(target)
    };

    let ast = match hulk_parser::parse(&source) {
        Ok(ast) => ast,
        Err(err) => report_parse_error(&source, err),
    };

    if let Err(errors) = hulk_semantic::analyze(&ast) {
        for error in &errors {
            let (line, col) = line_col(&source, error.span().lo as usize);
            eprintln!("({line},{col}) SEMANTIC: {error}");
        }
        exit(EXIT_SEMANTIC);
    }

    let ir = hulk_ir::lower_program(&ast);
    if let Err(err) = hulk_vm::Vm::run_program(ir) {
        eprintln!("(0,0) SEMANTIC: runtime error: {err}");
        exit(EXIT_SEMANTIC);
    }
}

/// Development aid: dump the parsed AST.
fn cmd_parse(file: &str) {
    let source = read_source(file);
    match hulk_parser::parse(&source) {
        Ok(ast) => println!("{ast:#?}"),
        Err(err) => report_parse_error(&source, err),
    }
}

/// Write a self-contained `./output` executable that re-runs the validated
/// program through the interpreter. The script embeds the program source via a
/// quoted here-document and invokes this binary in `exec` mode.
fn write_output(source: &str) {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("(0,0) SEMANTIC: cannot locate compiler binary: {err}");
            exit(EXIT_SEMANTIC);
        }
    };

    let script = format!(
        "#!/bin/sh\n'{}' exec - <<'__HULK_SOURCE_EOF__'\n{}\n__HULK_SOURCE_EOF__\n",
        exe.display(),
        source,
    );

    if let Err(err) = std::fs::write("output", script) {
        eprintln!("(0,0) SEMANTIC: cannot write ./output: {err}");
        exit(EXIT_SEMANTIC);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = std::fs::set_permissions("output", std::fs::Permissions::from_mode(0o755))
        {
            eprintln!("(0,0) SEMANTIC: cannot make ./output executable: {err}");
            exit(EXIT_SEMANTIC);
        }
    }
}

/// Convert a byte offset into a 1-based `(line, column)` pair for diagnostics.
/// Columns count Unicode scalar values within the line, not bytes.
fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 1;
    let mut col = 1;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Print a parse/lex diagnostic in the interface format and exit with the code
/// for its category. A `ParseError::User` carries a lexer error (LEXICAL);
/// every other variant is a grammar error (SYNTACTIC).
fn report_parse_error(source: &str, err: ParseError<'_>) -> ! {
    let (line, col, kind, code, message) = match &err {
        ParseError::User { error } => {
            let (line, col) = line_col(source, error.start);
            let snippet = source.get(error.start..error.end.min(source.len()));
            let message = match snippet {
                Some(text) if !text.is_empty() => format!("unexpected character {text:?}"),
                _ => "unexpected character".to_string(),
            };
            (line, col, "LEXICAL", EXIT_LEXICAL, message)
        }
        ParseError::InvalidToken { location } => {
            let (line, col) = line_col(source, *location);
            (line, col, "SYNTACTIC", EXIT_SYNTACTIC, "invalid token".to_string())
        }
        ParseError::UnrecognizedEof { location, expected } => {
            let (line, col) = line_col(source, *location);
            let message = format!("unexpected end of input{}", expected_suffix(expected));
            (line, col, "SYNTACTIC", EXIT_SYNTACTIC, message)
        }
        ParseError::UnrecognizedToken { token, expected } => {
            let (lo, _tok, hi) = token;
            let (line, col) = line_col(source, *lo);
            let snippet = source.get(*lo..(*hi).min(source.len())).unwrap_or("");
            let message =
                format!("unexpected token {snippet:?}{}", expected_suffix(expected));
            (line, col, "SYNTACTIC", EXIT_SYNTACTIC, message)
        }
        ParseError::ExtraToken { token } => {
            let (lo, _tok, hi) = token;
            let (line, col) = line_col(source, *lo);
            let snippet = source.get(*lo..(*hi).min(source.len())).unwrap_or("");
            (line, col, "SYNTACTIC", EXIT_SYNTACTIC, format!("extra token {snippet:?}"))
        }
    };

    eprintln!("({line},{col}) {kind}: {message}");
    exit(code);
}

/// Render an `expected one of ...` suffix when the parser reported candidates.
fn expected_suffix(expected: &[String]) -> String {
    if expected.is_empty() {
        String::new()
    } else {
        format!(", expected one of {}", expected.join(", "))
    }
}
