//! Shared AST types for the HULK compiler.
//!
//! This crate defines the contract between the frontend (lexer/parser) and the
//! later phases (semantic analysis, IR lowering). All other crates depend on
//! the types declared here.
//!
//! Each variant maps to a syntactic construct from the HULK book, sections
//! A.2–A.8. The shape is dictated by what the semantic analyzer must inspect:
//!
//! - **Owned `String` for names** — interning is a frontend optimization
//!   that doesn't change semantics.
//! - **`Box<Expr>` recursion** — no arena. The AST is small for typical programs.
//! - **`Span` on every node** — error messages without source location are unusable.
//! - **`Let` is unary** — multi-binding `let a=1, b=2 in body` must be
//!   desugared by the parser to nested `Let` nodes (spec A.4.1).
//! - **`If` always has an `else` branch** — `if` is always an expression (A.5).
//! - **`SelfExpr` is its own variant**, not `Ident("self")`.

use std::fmt;

// ---------- Spans ----------

/// Source code span: byte offsets into the original input.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct Span {
    /// Start byte offset (inclusive).
    pub lo: u32,
    /// End byte offset (exclusive).
    pub hi: u32,
}

impl Span {
    /// Create a new span from byte offsets.
    pub fn new(lo: usize, hi: usize) -> Self {
        Self {
            lo: u32::try_from(lo).expect("invariant: span lo fits in u32"),
            hi: u32::try_from(hi).expect("invariant: span hi fits in u32"),
        }
    }

    /// Merge two spans into one covering both.
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

/// A complete HULK program: type declarations, function declarations, and a
/// single entry expression.
#[derive(Debug, Clone)]
pub struct Program {
    /// Type declarations (classes).
    pub types: Vec<TypeDecl>,
    /// Global function declarations.
    pub functions: Vec<FunctionDecl>,
    /// The entry-point expression evaluated when the program runs.
    pub entry: Expr,
}

/// A global function declaration (A.3).
#[derive(Debug, Clone)]
pub struct FunctionDecl {
    /// Function name.
    pub name: String,
    /// Parameter list.
    pub params: Vec<Param>,
    /// Optional return type annotation.
    pub return_ty: Option<String>,
    /// Function body expression.
    pub body: Expr,
    /// Source span covering the entire declaration.
    pub span: Span,
}

/// A parameter with optional type annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Optional type annotation (e.g., `x: Number`).
    pub ty: Option<String>,
    /// Source span.
    pub span: Span,
}

/// A type (class) declaration (A.7).
#[derive(Debug, Clone)]
pub struct TypeDecl {
    /// Type name.
    pub name: String,
    /// Constructor parameters (may be empty).
    pub type_params: Vec<Param>,
    /// Optional parent type for inheritance.
    pub parent: Option<ParentSpec>,
    /// Attribute declarations.
    pub attributes: Vec<AttrDecl>,
    /// Method declarations.
    pub methods: Vec<MethodDecl>,
    /// Source span covering the entire declaration.
    pub span: Span,
}

/// Specifies the parent type in an inheritance clause (A.7.3).
#[derive(Debug, Clone)]
pub struct ParentSpec {
    /// Parent type name.
    pub name: String,
    /// `None` means "forward this type's constructor args to the parent"
    /// (A.7.3 default). `Some(args)` is the explicit `inherits P(a, b)` form.
    pub args: Option<Vec<Expr>>,
    /// Source span.
    pub span: Span,
}

/// An attribute declaration inside a type body (A.7.2).
#[derive(Debug, Clone)]
pub struct AttrDecl {
    /// Attribute name.
    pub name: String,
    /// Optional type annotation.
    pub ty: Option<String>,
    /// Initialization expression (sees only constructor params, not `self`).
    pub init: Expr,
    /// Source span.
    pub span: Span,
}

/// A method declaration inside a type body (A.7.1).
#[derive(Debug, Clone)]
pub struct MethodDecl {
    /// Method name.
    pub name: String,
    /// Parameter list (does not include `self`).
    pub params: Vec<Param>,
    /// Optional return type annotation.
    pub return_ty: Option<String>,
    /// Method body expression.
    pub body: Expr,
    /// Source span.
    pub span: Span,
}

// ---------- Expressions ----------

/// An expression node carrying its source span.
#[derive(Debug, Clone)]
pub struct Expr {
    /// Source span.
    pub span: Span,
    /// The kind of expression.
    pub kind: ExprKind,
}

/// All expression variants in HULK (A.2–A.8).
#[derive(Debug, Clone)]
pub enum ExprKind {
    // -- Literals (A.2.1, A.2.2, A.5) --
    /// Numeric literal.
    Number(f64),
    /// String literal.
    String(String),
    /// Boolean literal (`true` or `false`).
    Bool(bool),

    // -- Names --
    /// Variable or constant reference.
    Ident(String),
    /// The implicit `self` reference inside methods (A.7.1).
    SelfExpr,

    // -- Operators --
    /// Binary operation.
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    /// Unary operation.
    UnOp(UnOp, Box<Expr>),

    // -- Calls --
    /// Global function call: `name(args)`.
    Call(String, Vec<Expr>),
    /// Method call: `obj.method(args)`.
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// Parent method call: `base(args)` (A.7.4).
    Base(Vec<Expr>),

    // -- Member access (valid only via `self`, enforced by checker, A.7) --
    /// Field access: `obj.field`.
    GetField(Box<Expr>, String),

    // -- Bindings (A.4) — `Let` is unary; multi-binding desugared by parser --
    /// `let name [: Type] = init in body`.
    Let(String, Option<String>, Box<Expr>, Box<Expr>),
    /// Destructive assignment: `name := value` (A.4.6).
    Assign(String, Box<Expr>),
    /// Field assignment: `self.field := value`.
    AssignField(Box<Expr>, String, Box<Expr>),

    // -- Control flow (all expressions) --
    /// `if (cond) then elif* else` — always has an else branch (A.5).
    /// Fields: condition, then-branch, elif-branches, else-branch.
    If(Box<Expr>, Box<Expr>, Vec<(Expr, Expr)>, Box<Expr>),
    /// `while (cond) body` (A.6.1).
    While(Box<Expr>, Box<Expr>),
    /// `for (var in iterable) body` (A.6.2).
    For(String, Box<Expr>, Box<Expr>),
    /// Expression block: `{ expr; expr; ... }` (A.2.4).
    Block(Vec<Expr>),

    // -- OOP --
    /// Object instantiation: `new TypeName(args)` (A.7.2).
    New(String, Vec<Expr>),

    // -- Type operations (A.8.5, A.8.6) --
    /// Runtime type test: `expr is TypeName`.
    Is(Box<Expr>, String),
    /// Downcast: `expr as TypeName`.
    As(Box<Expr>, String),
}

/// Binary operators (A.2.1, A.2.2, A.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `+` addition.
    Add,
    /// `-` subtraction.
    Sub,
    /// `*` multiplication.
    Mul,
    /// `/` division.
    Div,
    /// `^` power.
    Pow,
    /// `%` modulo.
    Mod,
    /// `@` string concatenation.
    Concat,
    /// `@@` string concatenation with space.
    ConcatWs,
    /// `==` equality.
    Eq,
    /// `!=` inequality.
    Ne,
    /// `<` less than.
    Lt,
    /// `<=` less than or equal.
    Le,
    /// `>` greater than.
    Gt,
    /// `>=` greater than or equal.
    Ge,
    /// `&` logical and.
    And,
    /// `|` logical or.
    Or,
}

impl BinOp {
    /// Returns the operator symbol as a string slice.
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

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// `-` arithmetic negation.
    Neg,
    /// `!` logical negation.
    Not,
}

impl UnOp {
    /// Returns the operator symbol as a string slice.
    pub fn as_str(self) -> &'static str {
        match self {
            UnOp::Neg => "-",
            UnOp::Not => "!",
        }
    }
}
