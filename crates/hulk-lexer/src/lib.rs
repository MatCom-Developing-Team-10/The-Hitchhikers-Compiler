//! Tokenizer for the HULK language, built on top of `logos`.
//!
//! Skeleton — tokens will be added incrementally as the grammar grows.

use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\f\r]+")]
pub enum Token {
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,

    #[regex(r#""([^"\\]|\\.)*""#)]
    String,

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
}
