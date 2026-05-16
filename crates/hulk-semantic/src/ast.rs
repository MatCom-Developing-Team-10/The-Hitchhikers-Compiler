//! AST contract — what role B (semantic) needs from role A (frontend).
//!
//! This file is **temporary**. It exists so role B can develop and test the
//! semantic analyzer in isolation while role A finishes the design of the
//! `hulk-ast` crate. The types here are the formal proposal B is bringing
//! to the team meeting; once A merges them into `hulk-ast`, this module is
//! removed and every `use crate::ast::*` in the crate becomes `use hulk_ast::*`.
//!
//! ## Why these specific types
//!
//! Each variant maps to a syntactic construct from the HULK book, sections
//! A.2–A.8. The shape is dictated by what the semantic analyzer must inspect:
//!
//! - **Owned `String` for names** — interning is a frontend optimization
//!   that doesn't change semantics. The semantic crate is happy comparing
//!   strings; if A later moves to a `SymbolId`, only the field type changes.
//! - **`Box<Expr>` recursion** — no arena. The AST is small (≤ 10⁴ nodes for
//!   typical programs), and per-node allocation is the idiomatic shape.
//! - **`Span` on every node** — non-negotiable. Error messages without
//!   line:column are unusable for the cátedra tests.
//! - **`Let` is unary** — multi-binding `let a=1, b=2 in body` must be
//!   desugared by A *in the parser* to nested `Let` nodes. Spec A.4.1:
//!   "let associates to the right" and the semantics of sequential binding
//!   guarantee the desugaring is lossless. Keeping `Let` unary in the AST
//!   eliminates an entire class of scoping bugs from the semantic pass.
//! - **`If` always has an `else` branch** — `if` is always an expression
//!   (A.5); an `else`-less form has no value on the false branch and is
//!   not representable.
//! - **`SelfExpr` is its own variant**, not `Ident("self")` — although A.7.1
//!   notes `self` is not a keyword, separating it simplifies the rule
//!   "`self` is not a valid assignment target" and keeps shadowing intact
//!   (a `let self = …` introduces a `Binding` named `"self"` that overrides
//!   the implicit `Self` typing in the environment).
//! - **`Assign` vs `AssignField`** — different validity rules (`Assign` to
//!   local; `AssignField` only via `self.<field>`).

use std::fmt;

// ---------- Spans ----------

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct Span {
    pub lo: u32,
    pub hi: u32,
}

impl Span {
    pub fn new(lo: usize, hi: usize) -> Self {
        Self {
            lo: lo as u32,
            hi: hi as u32,
        }
    }
    pub fn join(self, other: Span) -> Span {
        Span {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.lo, self.hi)
    }
}

// ---------- Top-level ----------

#[derive(Debug, Clone)]
pub struct Program {
    pub types: Vec<TypeDecl>,
    pub functions: Vec<FunctionDecl>,
    pub entry: Expr,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_ty: Option<String>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub type_params: Vec<Param>,
    pub parent: Option<ParentSpec>,
    pub attributes: Vec<AttrDecl>,
    pub methods: Vec<MethodDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParentSpec {
    pub name: String,
    /// `None` ⇒ "forward this type's constructor args to the parent" (A.7.3
    /// default). `Some(args)` is the explicit `inherits P(arg1, arg2, ...)`
    /// form.
    pub args: Option<Vec<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AttrDecl {
    pub name: String,
    pub ty: Option<String>,
    pub init: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_ty: Option<String>,
    pub body: Expr,
    pub span: Span,
}

// ---------- Expressions ----------

#[derive(Debug, Clone)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // Literals (A.2.1, A.2.2, A.5)
    Number(f64),
    String(String),
    Bool(bool),

    // Names
    Ident(String),
    SelfExpr,

    // Operators
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnOp(UnOp, Box<Expr>),

    // Calls
    Call(String, Vec<Expr>),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    Base(Vec<Expr>),

    // Member access (valid only via `self`, enforced by the checker, A.7)
    GetField(Box<Expr>, String),

    // Bindings (A.4) — `Let` is unary; multi-binding desugared by parser
    Let(String, Option<String>, Box<Expr>, Box<Expr>),
    Assign(String, Box<Expr>),
    AssignField(Box<Expr>, String, Box<Expr>),

    // Control flow (all expressions)
    If(Box<Expr>, Box<Expr>, Vec<(Expr, Expr)>, Box<Expr>),
    While(Box<Expr>, Box<Expr>),
    For(String, Box<Expr>, Box<Expr>),
    Block(Vec<Expr>),

    // OOP
    New(String, Vec<Expr>),

    // Type ops (A.8.5, A.8.6)
    Is(Box<Expr>, String),
    As(Box<Expr>, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
    Concat,
    ConcatWs,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
            BinOp::Mod => "%",
            BinOp::Concat => "@",
            BinOp::ConcatWs => "@@",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&",
            BinOp::Or => "|",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}
