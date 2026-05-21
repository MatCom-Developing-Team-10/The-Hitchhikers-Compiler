//! Tokenizer for the HULK language.
//!
//! Uses [logos](https://docs.rs/logos) to generate a fast lexer from the [`Token`]
//! enum. The [`Lexer`] adapter wraps the logos iterator to produce the
//! `(start, Token, end)` triples that LALRpop expects.

use std::fmt;

use logos::Logos;

// ---------- Token enum ----------

/// All token kinds in the HULK language.
///
/// Logos resolves ambiguity by longest match and then by priority. Fixed
/// `#[token]` patterns have higher priority than `#[regex]`, so keywords
/// always win over identifiers.
#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\f\r]+")]
#[logos(skip(r"//[^\n]*", allow_greedy = true))]
pub enum Token {
    // ---- Keywords ----
    /// `let` keyword (A.4).
    #[token("let")]
    Let,
    /// `in` keyword (A.4).
    #[token("in")]
    In,
    /// `if` keyword (A.5).
    #[token("if")]
    If,
    /// `elif` keyword (A.5.2).
    #[token("elif")]
    Elif,
    /// `else` keyword (A.5).
    #[token("else")]
    Else,
    /// `while` keyword (A.6.1).
    #[token("while")]
    While,
    /// `for` keyword (A.6.2).
    #[token("for")]
    For,
    /// `function` keyword (A.3).
    #[token("function")]
    Function,
    /// `type` keyword (A.7).
    #[token("type")]
    Type,
    /// `inherits` keyword (A.7.3).
    #[token("inherits")]
    Inherits,
    /// `new` keyword (A.7.2).
    #[token("new")]
    New,
    /// `is` keyword (A.8.5).
    #[token("is")]
    Is,
    /// `as` keyword (A.8.6).
    #[token("as")]
    As,
    /// `true` literal.
    #[token("true")]
    True,
    /// `false` literal.
    #[token("false")]
    False,

    // ---- Operators ----
    /// `+` addition.
    #[token("+")]
    Plus,
    /// `-` subtraction / unary negation.
    #[token("-")]
    Minus,
    /// `**` power (alias for `^`, must come before `*`).
    #[token("**")]
    StarStar,
    /// `*` multiplication.
    #[token("*")]
    Star,
    /// `/` division.
    #[token("/")]
    Slash,
    /// `^` power.
    #[token("^")]
    Caret,
    /// `%` modulo.
    #[token("%")]
    Percent,
    /// `@@` string concatenation with space (must come before `@`).
    #[token("@@")]
    AtAt,
    /// `@` string concatenation.
    #[token("@")]
    At,
    /// `==` equality.
    #[token("==")]
    EqEq,
    /// `!=` inequality.
    #[token("!=")]
    BangEq,
    /// `<=` less than or equal.
    #[token("<=")]
    LtEq,
    /// `<` less than.
    #[token("<")]
    Lt,
    /// `>=` greater than or equal.
    #[token(">=")]
    GtEq,
    /// `>` greater than.
    #[token(">")]
    Gt,
    /// `&` logical and.
    #[token("&")]
    Amp,
    /// `|` logical or.
    #[token("|")]
    Pipe,
    /// `!` logical not.
    #[token("!")]
    Bang,
    /// `:=` destructive assignment (A.4.6).
    #[token(":=")]
    ColonEq,
    /// `=>` arrow for inline functions/methods (A.3.1).
    #[token("=>")]
    FatArrow,
    /// `=` binding in `let` and attribute declarations.
    #[token("=")]
    Eq,

    // ---- Punctuation ----
    /// `(` left parenthesis.
    #[token("(")]
    LParen,
    /// `)` right parenthesis.
    #[token(")")]
    RParen,
    /// `{` left brace.
    #[token("{")]
    LBrace,
    /// `}` right brace.
    #[token("}")]
    RBrace,
    /// `;` semicolon.
    #[token(";")]
    Semi,
    /// `,` comma.
    #[token(",")]
    Comma,
    /// `.` dot for member access.
    #[token(".")]
    Dot,
    /// `:` colon for type annotations.
    #[token(":")]
    Colon,

    // ---- Literals ----
    /// Numeric literal (integer or floating-point).
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,

    /// String literal (double-quoted, supports escape sequences).
    #[regex(r#""([^"\\]|\\.)*""#)]
    StringLit,

    // ---- Identifier ----
    /// Identifier: starts with a letter, then letters/digits/underscores.
    /// Per spec A.4.7, identifiers must NOT start with underscore.
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*")]
    Ident,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Let => write!(f, "let"),
            Token::In => write!(f, "in"),
            Token::If => write!(f, "if"),
            Token::Elif => write!(f, "elif"),
            Token::Else => write!(f, "else"),
            Token::While => write!(f, "while"),
            Token::For => write!(f, "for"),
            Token::Function => write!(f, "function"),
            Token::Type => write!(f, "type"),
            Token::Inherits => write!(f, "inherits"),
            Token::New => write!(f, "new"),
            Token::Is => write!(f, "is"),
            Token::As => write!(f, "as"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::StarStar => write!(f, "**"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Caret => write!(f, "^"),
            Token::Percent => write!(f, "%"),
            Token::AtAt => write!(f, "@@"),
            Token::At => write!(f, "@"),
            Token::EqEq => write!(f, "=="),
            Token::BangEq => write!(f, "!="),
            Token::LtEq => write!(f, "<="),
            Token::Lt => write!(f, "<"),
            Token::GtEq => write!(f, ">="),
            Token::Gt => write!(f, ">"),
            Token::Amp => write!(f, "&"),
            Token::Pipe => write!(f, "|"),
            Token::Bang => write!(f, "!"),
            Token::ColonEq => write!(f, ":="),
            Token::FatArrow => write!(f, "=>"),
            Token::Eq => write!(f, "="),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Semi => write!(f, ";"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::Colon => write!(f, ":"),
            Token::Number => write!(f, "<number>"),
            Token::StringLit => write!(f, "<string>"),
            Token::Ident => write!(f, "<identifier>"),
        }
    }
}

// ---------- Value extraction ----------

/// Parse a numeric literal from its source slice.
///
/// The regex guarantees the slice is a valid number, so this cannot fail
/// for well-formed tokens.
pub fn parse_number(slice: &str) -> f64 {
    slice
        .parse()
        .expect("invariant: regex guarantees valid number literal")
}

/// Parse a string literal: strip surrounding quotes and resolve escape sequences.
pub fn parse_string(slice: &str) -> String {
    let inner = &slice[1..slice.len() - 1];
    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ---------- Lexer error ----------

/// Error produced when the lexer encounters an unrecognized character.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    /// Start byte offset of the invalid character(s).
    pub start: usize,
    /// End byte offset.
    pub end: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unexpected character at {}..{}", self.start, self.end)
    }
}

impl std::error::Error for LexError {}

// ---------- LALRpop adapter ----------

/// A token with its byte-offset span, in the format LALRpop expects.
pub type Spanned = (usize, Token, usize);

/// Adapter that bridges logos tokenization to LALRpop's expected iterator
/// protocol: `Iterator<Item = Result<(usize, Token, usize), LexError>>`.
pub struct Lexer<'input> {
    inner: logos::Lexer<'input, Token>,
}

impl<'input> Lexer<'input> {
    /// Create a new lexer for the given source string.
    pub fn new(input: &'input str) -> Self {
        Self {
            inner: Token::lexer(input),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<Spanned, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.inner.next()?;
        let span = self.inner.span();
        match token {
            Ok(tok) => Some(Ok((span.start, tok, span.end))),
            Err(()) => Some(Err(LexError {
                start: span.start,
                end: span.end,
            })),
        }
    }
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Token> {
        Lexer::new(input)
            .map(|r| r.expect("unexpected lex error").1)
            .collect()
    }

    #[test]
    fn keywords_tokenize_correctly() {
        let input = "let in if elif else while for function type inherits new is as true false";
        let tokens = lex(input);
        assert_eq!(
            tokens,
            vec![
                Token::Let,
                Token::In,
                Token::If,
                Token::Elif,
                Token::Else,
                Token::While,
                Token::For,
                Token::Function,
                Token::Type,
                Token::Inherits,
                Token::New,
                Token::Is,
                Token::As,
                Token::True,
                Token::False,
            ]
        );
    }

    #[test]
    fn operators_tokenize_correctly() {
        let input = "+ - * / ^ % @ @@ == != < <= > >= & | ! := = =>";
        let tokens = lex(input);
        assert_eq!(
            tokens,
            vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Caret,
                Token::Percent,
                Token::At,
                Token::AtAt,
                Token::EqEq,
                Token::BangEq,
                Token::Lt,
                Token::LtEq,
                Token::Gt,
                Token::GtEq,
                Token::Amp,
                Token::Pipe,
                Token::Bang,
                Token::ColonEq,
                Token::Eq,
                Token::FatArrow,
            ]
        );
    }

    #[test]
    fn punctuation_tokenizes_correctly() {
        let input = "( ) { } ; , . :";
        let tokens = lex(input);
        assert_eq!(
            tokens,
            vec![
                Token::LParen,
                Token::RParen,
                Token::LBrace,
                Token::RBrace,
                Token::Semi,
                Token::Comma,
                Token::Dot,
                Token::Colon,
            ]
        );
    }

    #[test]
    fn number_literals() {
        assert_eq!(lex("42"), vec![Token::Number]);
        assert_eq!(lex("3.14"), vec![Token::Number]);
        assert_eq!(lex("0.5"), vec![Token::Number]);

        assert_eq!(parse_number("42"), 42.0);
        assert_eq!(parse_number("2.75"), 2.75);
    }

    #[test]
    fn string_literals() {
        assert_eq!(lex(r#""Hello World""#), vec![Token::StringLit]);
        assert_eq!(parse_string(r#""Hello World""#), "Hello World");
        assert_eq!(parse_string(r#""line\nbreak""#), "line\nbreak");
        assert_eq!(parse_string(r#""tab\there""#), "tab\there");
        assert_eq!(parse_string(r#""escaped\"quote""#), "escaped\"quote");
        assert_eq!(parse_string(r#""back\\slash""#), "back\\slash");
    }

    #[test]
    fn identifiers() {
        assert_eq!(lex("x"), vec![Token::Ident]);
        assert_eq!(lex("x0"), vec![Token::Ident]);
        assert_eq!(lex("myVar"), vec![Token::Ident]);
        assert_eq!(lex("TitleCase"), vec![Token::Ident]);
        assert_eq!(lex("snake_case"), vec![Token::Ident]);
    }

    #[test]
    fn self_and_base_are_identifiers() {
        assert_eq!(lex("self"), vec![Token::Ident]);
        assert_eq!(lex("base"), vec![Token::Ident]);
    }

    #[test]
    fn keywords_win_over_identifiers() {
        // "letter" starts with "let" but should be Ident, not Let
        assert_eq!(lex("letter"), vec![Token::Ident]);
        assert_eq!(lex("let"), vec![Token::Let]);
        assert_eq!(lex("infix"), vec![Token::Ident]);
        assert_eq!(lex("in"), vec![Token::In]);
    }

    #[test]
    fn comments_are_skipped() {
        let input = "42 // this is a comment\n7";
        let tokens = lex(input);
        assert_eq!(tokens, vec![Token::Number, Token::Number]);
    }

    #[test]
    fn whitespace_variants_are_skipped() {
        let input = "42\t\n  7\r\n3";
        let tokens = lex(input);
        assert_eq!(tokens, vec![Token::Number, Token::Number, Token::Number]);
    }

    #[test]
    fn multi_char_operators_are_greedy() {
        assert_eq!(lex("@@"), vec![Token::AtAt]);
        assert_eq!(lex(":="), vec![Token::ColonEq]);
        assert_eq!(lex("=>"), vec![Token::FatArrow]);
        assert_eq!(lex("=="), vec![Token::EqEq]);
    }

    #[test]
    fn unknown_character_produces_error() {
        let mut lexer = Lexer::new("42 $ 7");
        assert!(lexer.next().unwrap().is_ok()); // 42
        assert!(lexer.next().unwrap().is_err()); // $
        assert!(lexer.next().unwrap().is_ok()); // 7
    }

    #[test]
    fn span_tracking() {
        let mut lexer = Lexer::new("let x = 42;");
        let (start, tok, end) = lexer.next().unwrap().unwrap();
        assert_eq!(tok, Token::Let);
        assert_eq!(start, 0);
        assert_eq!(end, 3);

        let (start, tok, end) = lexer.next().unwrap().unwrap();
        assert_eq!(tok, Token::Ident);
        assert_eq!(start, 4);
        assert_eq!(end, 5);
    }

    #[test]
    fn complete_hulk_program_tokenizes() {
        let input = r#"function tan(x) => sin(x) / cos(x);
print(tan(PI) ^ 2);"#;
        let tokens = lex(input);
        assert_eq!(
            tokens,
            vec![
                Token::Function,
                Token::Ident, // tan
                Token::LParen,
                Token::Ident, // x
                Token::RParen,
                Token::FatArrow,
                Token::Ident, // sin
                Token::LParen,
                Token::Ident, // x
                Token::RParen,
                Token::Slash,
                Token::Ident, // cos
                Token::LParen,
                Token::Ident, // x
                Token::RParen,
                Token::Semi,
                Token::Ident, // print
                Token::LParen,
                Token::Ident, // tan
                Token::LParen,
                Token::Ident, // PI
                Token::RParen,
                Token::Caret,
                Token::Number, // 2
                Token::RParen,
                Token::Semi,
            ]
        );
    }
}
