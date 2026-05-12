//! Semantic analysis for the HULK compiler.
//!
//! Skeleton — to be filled with symbol tables, scope resolution and a
//! Hindley-Milner-ish type inference pass.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("undefined name: {0}")]
    UndefinedName(String),
}
