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

/// A complete HULK program: interface, type, and function declarations,
/// plus a single entry expression.
#[derive(Debug, Clone)]
pub struct Program {
    /// Interface declarations (extension).
    pub interfaces: Vec<InterfaceDecl>,
    /// Type declarations (classes).
    pub types: Vec<TypeDecl>,
    /// Global function declarations.
    pub functions: Vec<FunctionDecl>,
    /// The entry-point expression evaluated when the program runs.
    pub entry: Expr,
}

/// A reference to a type, possibly parameterized by generic arguments.
///
/// Extension (generics): the parser produces this in every position that
/// previously accepted just a type name (`Option<String>`). Examples:
/// - `Number` → `TypeRef::Simple("Number")`
/// - `List[Number]` → `TypeRef::Generic("List", [Simple("Number")])`
/// - `Map[String, List[Number]]` → nested.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    /// A non-parameterized type reference (e.g., `Number`, `Point`).
    Simple(String),
    /// A generic type reference (e.g., `List[T]`, `Map[K, V]`).
    Generic(String, Vec<TypeRef>),
    /// A typed-iterable reference written `T*` (A.11.2): "an iterable whose
    /// elements have type `T`". Sugar for the `Iterable` protocol specialized
    /// to element type `T`.
    Iterable(Box<TypeRef>),
    /// A vector type written `T[]` (A.12.3): a concrete vector whose elements
    /// have type `T`. Unlike `T*`, a `T[]` value also supports indexing and
    /// `size()`.
    Vector(Box<TypeRef>),
}

impl TypeRef {
    /// Returns the base type name (without generic arguments).
    pub fn base_name(&self) -> &str {
        match self {
            TypeRef::Simple(n) | TypeRef::Generic(n, _) => n,
            TypeRef::Iterable(inner) | TypeRef::Vector(inner) => inner.base_name(),
        }
    }
}

impl From<&str> for TypeRef {
    fn from(s: &str) -> Self {
        TypeRef::Simple(s.to_string())
    }
}

impl From<String> for TypeRef {
    fn from(s: String) -> Self {
        TypeRef::Simple(s)
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRef::Simple(n) => write!(f, "{n}"),
            TypeRef::Generic(n, args) => {
                write!(f, "{n}[")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, "]")
            }
            TypeRef::Iterable(inner) => write!(f, "{inner}*"),
            TypeRef::Vector(inner) => write!(f, "{inner}[]"),
        }
    }
}

/// A global function declaration (A.3).
#[derive(Debug, Clone)]
pub struct FunctionDecl {
    /// Function name.
    pub name: String,
    /// Generic type parameters (extension). Empty for non-generic functions.
    pub generic_params: Vec<String>,
    /// Parameter list.
    pub params: Vec<Param>,
    /// Optional return type annotation.
    pub return_ty: Option<TypeRef>,
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
    /// Optional type annotation (e.g., `x: Number`, `xs: List[T]`).
    pub ty: Option<TypeRef>,
    /// Source span.
    pub span: Span,
}

/// A type (class) declaration (A.7).
#[derive(Debug, Clone)]
pub struct TypeDecl {
    /// Type name.
    pub name: String,
    /// Generic type parameters (extension). Empty for non-generic types.
    pub generic_params: Vec<String>,
    /// Constructor parameters (may be empty).
    pub type_params: Vec<Param>,
    /// Optional parent type for inheritance.
    pub parent: Option<ParentSpec>,
    /// Interfaces this type implements (extension). Empty when no `implements`.
    pub implements: Vec<TypeRef>,
    /// Attribute declarations.
    pub attributes: Vec<AttrDecl>,
    /// Method declarations.
    pub methods: Vec<MethodDecl>,
    /// Source span covering the entire declaration.
    pub span: Span,
}

/// An interface declaration (extension): a named contract of method signatures.
#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    /// Interface name.
    pub name: String,
    /// Generic type parameters (empty for non-generic interfaces).
    pub generic_params: Vec<String>,
    /// Parent interfaces (multiple inheritance allowed for interfaces only).
    pub extends: Vec<TypeRef>,
    /// Method signatures (no body).
    pub methods: Vec<InterfaceMethodSig>,
    /// Source span.
    pub span: Span,
}

/// A method signature inside an interface (no body).
#[derive(Debug, Clone)]
pub struct InterfaceMethodSig {
    /// Method name.
    pub name: String,
    /// Parameter list (does not include `self`).
    pub params: Vec<Param>,
    /// Optional return type annotation.
    pub return_ty: Option<TypeRef>,
    /// Source span.
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
    pub ty: Option<TypeRef>,
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
    pub return_ty: Option<TypeRef>,
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
    Let(String, Option<TypeRef>, Box<Expr>, Box<Expr>),
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
    /// Object instantiation: `new TypeName[T1, T2](args)` (A.7.2 + generics).
    ///
    /// Fields: type name, generic type arguments (empty for non-generic types),
    /// constructor arguments.
    New(String, Vec<TypeRef>, Vec<Expr>),

    // -- Type operations (A.8.5, A.8.6) --
    /// Runtime type test: `expr is TypeName`.
    Is(Box<Expr>, TypeRef),
    /// Downcast: `expr as TypeName`.
    As(Box<Expr>, TypeRef),

    // -- Vectors (A.12) --
    /// Explicit vector literal: `[e0, e1, ...]` (A.12.1).
    Vector(Vec<Expr>),
    /// Implicit vector / generator pattern: `[elem | var in iterable]` (A.12.2).
    /// Fields: element expression, bound variable name, source iterable.
    VectorComp(Box<Expr>, String, Box<Expr>),
    /// Indexing: `vector[index]` (A.12.1).
    Index(Box<Expr>, Box<Expr>),
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
