//! Shared AST types for the HULK compiler.
//!
//! This crate defines the contract between the frontend (lexer/parser) and the
//! later phases (semantic analysis, IR lowering). All other crates depend on
//! the types declared here.

/// Source code span: byte offsets into the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// HULK expression — placeholder, expanded incrementally.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    String(String),
}
