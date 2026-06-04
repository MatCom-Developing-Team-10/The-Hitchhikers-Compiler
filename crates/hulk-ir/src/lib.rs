//! Bytecode IR and lowering for the HULK compiler.
//!
//! See `ir-v2.spec.md` (v0.2) and `ir-v3.spec.md` (v0.3) for the full
//! specification of the instruction set and lowering rules.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use hulk_ast::{AttrDecl, BinOp, Expr, ExprKind, FunctionDecl, Program, TypeDecl, UnOp};

// ── Object / ObjectRef ────────────────────────────────────────────────────────

/// A heap-allocated HULK object with a runtime type tag and mutable fields.
#[derive(Debug, Clone)]
pub struct Object {
    /// Runtime type name (e.g. `"Dog"`).
    pub type_name: String,
    /// Instance fields.
    pub fields: HashMap<String, Value>,
}

/// Shared, mutable reference to a heap-allocated [`Object`].
pub type ObjectRef = Rc<RefCell<Object>>;

// ── Value ────────────────────────────────────────────────────────────────────

/// A runtime value on the VM stack.
#[derive(Debug, Clone)]
pub enum Value {
    /// IEEE-754 double.
    Num(f64),
    /// Boolean.
    Bool(bool),
    /// Owned string.
    Str(String),
    /// Nil — result of a void expression (e.g. a while that never executes).
    Nil,
    /// A reference-counted heap object.
    Object(ObjectRef),
}

impl Value {
    /// Short human-readable type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "Number",
            Value::Bool(_) => "Boolean",
            Value::Str(_) => "String",
            Value::Nil => "Nil",
            Value::Object(_) => "Object",
        }
    }
}

/// Pointer equality for objects; structural equality for primitives.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Nil => write!(f, "nil"),
            Value::Object(r) => write!(f, "<{} object>", r.borrow().type_name),
        }
    }
}

// ── Instr ─────────────────────────────────────────────────────────────────────

/// A single stack-machine instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    // --- Literals ---
    /// Push a numeric literal.
    PushNum(f64),
    /// Push a boolean literal.
    PushBool(bool),
    /// Push a string literal.
    PushStr(String),
    /// Push Nil.
    PushNil,

    // --- Stack manipulation ---
    /// Discard the top of the stack.
    Pop,
    /// Duplicate the top of the stack.
    Dup,

    // --- Arithmetic (pop b, pop a, push result) ---
    Add,
    Sub,
    Mul,
    /// Division — `DivisionByZero` if divisor is 0.
    Div,
    /// Power via `f64::powf`.
    Pow,
    /// Remainder — `DivisionByZero` if divisor is 0.
    Mod,
    /// Arithmetic negation: pop a, push -a.
    Neg,

    // --- Boolean (pop b, pop a, push Bool) ---
    And,
    Or,
    /// Logical negation: pop a, push !a.
    Not,

    // --- Comparison (pop b, pop a, push Bool) ---
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // --- Strings (pop b, pop a, push String) ---
    /// `@` — string concat with auto-coercion via `Display`.
    Concat,
    /// `@@` — same but inserts a space between operands.
    ConcatWs,

    // --- Variables ---
    /// Push the value of a variable (searched from innermost scope outward).
    LoadVar(String),
    /// Pop a value and update an *existing* variable in the nearest scope
    /// where it is defined.
    StoreVar(String),

    // --- Scoping ---
    /// Push a new empty scope layer.
    BeginScope,
    /// Pop a value and create a fresh binding in the *current* scope layer.
    BindVar(String),
    /// Discard the current scope layer.
    EndScope,

    // --- Control flow (jumps by label name) ---
    /// No-op label marker; resolved to an instruction index at VM start-up.
    Label(String),
    /// Unconditional jump to a label.
    Jump(String),
    /// Pop a `Bool`; jump to label if it is `false`.
    JumpIfFalse(String),

    // --- Functions ---
    /// Call a named function using the top `n` stack values as arguments
    /// (last argument is at the top).
    Call(String, usize),
    /// End the current code block (entry point or function body). The return
    /// value is whatever is on top of the stack.
    Ret,

    // --- I/O ---
    /// Pop a value, print it to stdout, then push it back (`print` returns its
    /// argument in HULK).
    Print,

    // --- Math builtins ---
    Sqrt,
    Sin,
    Cos,
    Exp,
    /// Pop `value`, pop `base`, push `value.log(base)`.
    Log,
    /// Push a pseudo-random `f64` in \[0, 1\].
    Rand,

    // --- OOP ---
    /// Create an empty object with the given type tag and push it.
    NewObject(String),
    /// Pop object, push the value of its named field.
    GetField(String),
    /// Pop object (top), pop value; set field on object; push value.
    SetField(String),
    /// Virtual method dispatch: pop self (top) after `argc` args; call the
    /// resolved implementation with `[self, arg₀, …, argₙ₋₁]`.
    CallMethod(String, usize),
    /// Pop value; push `true` if it is an Object whose runtime type conforms
    /// to `type_name` (walking the inheritance chain).
    IsType(String),
    /// Pop value; assert it conforms to `type_name`; push the same value.
    /// Errors with `InvalidCast` if the check fails.
    AsType(String),
}

// ── IR program structures ─────────────────────────────────────────────────────

/// A lowered function ready for the VM.
#[derive(Debug, Clone)]
pub struct IrFunc {
    /// Parameter names in declaration order.
    pub params: Vec<String>,
    /// Instruction body; always ends with `Ret`.
    pub body: Vec<Instr>,
}

/// Runtime type metadata needed by the VM for dispatch and conformance checks.
#[derive(Debug, Clone)]
pub struct IrTypeInfo {
    /// Name of the parent type (`"Object"` if this type has no explicit parent).
    pub parent: String,
    /// Maps method name → name of the IR function that implements it in this type.
    /// Only contains methods declared in *this* type, not inherited ones.
    pub methods: HashMap<String, String>,
}

/// A complete lowered program.
#[derive(Debug)]
pub struct IrProgram {
    /// User-defined and generated functions, keyed by name.
    pub funcs: HashMap<String, IrFunc>,
    /// Type metadata for dispatch and `is`/`as` checks.
    pub types: HashMap<String, IrTypeInfo>,
    /// Entry-point instructions; always ends with `Ret`.
    pub entry: Vec<Instr>,
}

// ── Label counter / lowering context ─────────────────────────────────────────

struct Ctx {
    counter: usize,
    /// Set when lowering a method body, to resolve `Base`.
    current_type: Option<String>,
    /// Set when lowering a method body, to resolve `Base`.
    current_method: Option<String>,
    /// Parent type of `current_type`; set alongside the fields above.
    parent_type: Option<String>,
}

impl Ctx {
    fn new() -> Self {
        Self {
            counter: 0,
            current_type: None,
            current_method: None,
            parent_type: None,
        }
    }

    fn next(&mut self) -> usize {
        let n = self.counter;
        self.counter += 1;
        n
    }
}

// ── Lowering ──────────────────────────────────────────────────────────────────

/// Lower a single expression, appending instructions to `out`.
///
/// **Post-condition:** exactly one net value is added to the runtime stack.
pub fn lower_expr(expr: &Expr, out: &mut Vec<Instr>) {
    let mut ctx = Ctx::new();
    lower_inner(expr, out, &mut ctx);
}

fn lower_inner(expr: &Expr, out: &mut Vec<Instr>, ctx: &mut Ctx) {
    match &expr.kind {
        ExprKind::Number(n) => out.push(Instr::PushNum(*n)),
        ExprKind::Bool(b) => out.push(Instr::PushBool(*b)),
        ExprKind::String(s) => out.push(Instr::PushStr(s.clone())),

        ExprKind::Ident(name) => match name.as_str() {
            "PI" => out.push(Instr::PushNum(std::f64::consts::PI)),
            "E" => out.push(Instr::PushNum(std::f64::consts::E)),
            _ => out.push(Instr::LoadVar(name.clone())),
        },

        ExprKind::BinOp(op, lhs, rhs) => {
            lower_inner(lhs, out, ctx);
            lower_inner(rhs, out, ctx);
            let instr = match op {
                BinOp::Add => Instr::Add,
                BinOp::Sub => Instr::Sub,
                BinOp::Mul => Instr::Mul,
                BinOp::Div => Instr::Div,
                BinOp::Pow => Instr::Pow,
                BinOp::Mod => Instr::Mod,
                BinOp::And => Instr::And,
                BinOp::Or => Instr::Or,
                BinOp::Eq => Instr::Eq,
                BinOp::Ne => Instr::Ne,
                BinOp::Lt => Instr::Lt,
                BinOp::Le => Instr::Le,
                BinOp::Gt => Instr::Gt,
                BinOp::Ge => Instr::Ge,
                BinOp::Concat => Instr::Concat,
                BinOp::ConcatWs => Instr::ConcatWs,
            };
            out.push(instr);
        }

        ExprKind::UnOp(op, inner) => {
            lower_inner(inner, out, ctx);
            match op {
                UnOp::Neg => out.push(Instr::Neg),
                UnOp::Not => out.push(Instr::Not),
            }
        }

        // Destructive assignment returns the assigned value.
        ExprKind::Assign(name, val) => {
            lower_inner(val, out, ctx);
            out.push(Instr::Dup);
            out.push(Instr::StoreVar(name.clone()));
        }

        // Let: open scope, evaluate init, bind, evaluate body, close scope.
        ExprKind::Let(name, _ty, init, body) => {
            out.push(Instr::BeginScope);
            lower_inner(init, out, ctx);
            out.push(Instr::BindVar(name.clone()));
            lower_inner(body, out, ctx);
            out.push(Instr::EndScope);
        }

        // Block: lower each expression; pop all but the last.
        ExprKind::Block(exprs) => {
            if exprs.is_empty() {
                out.push(Instr::PushNil);
                return;
            }
            let last = exprs.len() - 1;
            for (i, e) in exprs.iter().enumerate() {
                lower_inner(e, out, ctx);
                if i < last {
                    out.push(Instr::Pop);
                }
            }
        }

        ExprKind::If(cond, then, elifs, else_) => {
            lower_if(cond, then, elifs, else_, out, ctx);
        }

        ExprKind::While(cond, body) => {
            lower_while(cond, body, out, ctx);
        }

        ExprKind::Call(name, args) => {
            lower_call(name, args, out, ctx);
        }

        ExprKind::For(_, _, _) => {
            panic!(
                "for loops require the Iterable protocol and are not yet supported in this backend"
            );
        }

        // ── OOP ──────────────────────────────────────────────────────────────

        // new Type(args) → lower args, Call("__ctor_Type", n)
        ExprKind::New(type_name, args) => {
            for arg in args {
                lower_inner(arg, out, ctx);
            }
            out.push(Instr::Call(format!("__ctor_{type_name}"), args.len()));
        }

        // self → LoadVar("self")
        ExprKind::SelfExpr => {
            out.push(Instr::LoadVar("self".to_string()));
        }

        // obj.field
        ExprKind::GetField(obj, field) => {
            lower_inner(obj, out, ctx);
            out.push(Instr::GetField(field.clone()));
        }

        // obj.field := val  (returns val)
        // SetField pops obj (top) then val, sets field, pushes val.
        ExprKind::AssignField(obj, field, val) => {
            lower_inner(val, out, ctx); // push val first
            lower_inner(obj, out, ctx); // push obj on top
            out.push(Instr::SetField(field.clone()));
        }

        // obj.method(args)  — self goes on top after args
        ExprKind::MethodCall(obj, method, args) => {
            for arg in args {
                lower_inner(arg, out, ctx);
            }
            lower_inner(obj, out, ctx); // self on top
            out.push(Instr::CallMethod(method.clone(), args.len()));
        }

        // base(args)  — call parent's version of the current method
        ExprKind::Base(args) => {
            let current_method = ctx
                .current_method
                .clone()
                .expect("invariant: Base used outside a method");
            let parent_type = ctx
                .parent_type
                .clone()
                .expect("invariant: parent_type not set");
            for arg in args {
                lower_inner(arg, out, ctx);
            }
            out.push(Instr::LoadVar("self".to_string()));
            out.push(Instr::Call(
                format!("__method_{parent_type}_{current_method}"),
                args.len() + 1,
            ));
        }

        // expr is Type
        ExprKind::Is(expr, type_name) => {
            lower_inner(expr, out, ctx);
            out.push(Instr::IsType(type_name.clone()));
        }

        // expr as Type
        ExprKind::As(expr, type_name) => {
            lower_inner(expr, out, ctx);
            out.push(Instr::AsType(type_name.clone()));
        }
    }
}

fn lower_if(
    cond: &Expr,
    then: &Expr,
    elifs: &[(Expr, Expr)],
    else_: &Expr,
    out: &mut Vec<Instr>,
    ctx: &mut Ctx,
) {
    let id = ctx.next();
    let end_label = format!("end_if_{id}");

    lower_inner(cond, out, ctx);
    let first_else = if elifs.is_empty() {
        format!("else_{id}")
    } else {
        format!("elif_{id}_0")
    };
    out.push(Instr::JumpIfFalse(first_else));
    lower_inner(then, out, ctx);
    out.push(Instr::Jump(end_label.clone()));

    for (i, (elif_cond, elif_body)) in elifs.iter().enumerate() {
        out.push(Instr::Label(format!("elif_{id}_{i}")));
        lower_inner(elif_cond, out, ctx);
        let next = if i + 1 < elifs.len() {
            format!("elif_{id}_{}", i + 1)
        } else {
            format!("else_{id}")
        };
        out.push(Instr::JumpIfFalse(next));
        lower_inner(elif_body, out, ctx);
        out.push(Instr::Jump(end_label.clone()));
    }

    out.push(Instr::Label(format!("else_{id}")));
    lower_inner(else_, out, ctx);
    out.push(Instr::Label(end_label));
}

fn lower_while(cond: &Expr, body: &Expr, out: &mut Vec<Instr>, ctx: &mut Ctx) {
    let n = ctx.next();
    let start = format!("loop_start_{n}");
    let end = format!("loop_end_{n}");

    out.push(Instr::PushNil);
    out.push(Instr::Label(start.clone()));
    lower_inner(cond, out, ctx);
    out.push(Instr::JumpIfFalse(end.clone()));
    out.push(Instr::Pop);
    lower_inner(body, out, ctx);
    out.push(Instr::Jump(start));
    out.push(Instr::Label(end));
}

fn lower_call(name: &str, args: &[Expr], out: &mut Vec<Instr>, ctx: &mut Ctx) {
    match name {
        "print" => {
            assert_eq!(
                args.len(),
                1,
                "print expects exactly 1 argument, got {}",
                args.len()
            );
            lower_inner(&args[0], out, ctx);
            out.push(Instr::Print);
        }
        "sqrt" => {
            assert_eq!(args.len(), 1, "sqrt expects 1 argument");
            lower_inner(&args[0], out, ctx);
            out.push(Instr::Sqrt);
        }
        "sin" => {
            assert_eq!(args.len(), 1, "sin expects 1 argument");
            lower_inner(&args[0], out, ctx);
            out.push(Instr::Sin);
        }
        "cos" => {
            assert_eq!(args.len(), 1, "cos expects 1 argument");
            lower_inner(&args[0], out, ctx);
            out.push(Instr::Cos);
        }
        "exp" => {
            assert_eq!(args.len(), 1, "exp expects 1 argument");
            lower_inner(&args[0], out, ctx);
            out.push(Instr::Exp);
        }
        "log" => {
            assert_eq!(args.len(), 2, "log expects 2 arguments (base, value)");
            lower_inner(&args[0], out, ctx); // base
            lower_inner(&args[1], out, ctx); // value
            out.push(Instr::Log);
        }
        "rand" => {
            assert_eq!(args.len(), 0, "rand expects 0 arguments");
            out.push(Instr::Rand);
        }
        other => {
            for arg in args {
                lower_inner(arg, out, ctx);
            }
            out.push(Instr::Call(other.to_string(), args.len()));
        }
    }
}

fn lower_func(decl: &FunctionDecl) -> IrFunc {
    let params = decl.params.iter().map(|p| p.name.clone()).collect();
    let mut body = Vec::new();
    let mut ctx = Ctx::new();
    lower_inner(&decl.body, &mut body, &mut ctx);
    body.push(Instr::Ret);
    IrFunc { params, body }
}

// ── OOP lowering helpers ───────────────────────────────────────────────────────

/// Returns the ancestry chain for `type_name` from the root user-defined type
/// down to (and including) `type_name`. The `all_types` slice is the full list
/// of `TypeDecl`s from the program.
///
/// "Object" is not a TypeDecl; the chain stops at the first type whose parent
/// is absent (inherits Object implicitly) or whose parent is not in `all_types`.
fn ancestry_chain<'a>(type_name: &str, all_types: &'a [TypeDecl]) -> Vec<&'a TypeDecl> {
    // Build a map for O(1) lookup.
    let map: HashMap<&str, &TypeDecl> =
        all_types.iter().map(|t| (t.name.as_str(), t)).collect();

    // Collect the chain from this type upward.
    let mut chain: Vec<&TypeDecl> = Vec::new();
    let mut cur = type_name;
    loop {
        if let Some(decl) = map.get(cur) {
            chain.push(decl);
            match &decl.parent {
                Some(p) if map.contains_key(p.name.as_str()) => cur = p.name.as_str(),
                _ => break, // parent is Object or absent
            }
        } else {
            break;
        }
    }
    chain.reverse(); // root → leaf
    chain
}

/// Lower a type constructor. The generated IrFunc has the ctor params of
/// `decl` as its parameter list, creates an object with `NewObject`, then
/// initializes all inherited and own fields in ancestry order.
///
/// The object is bound to the reserved local variable `__self` so that each
/// field initializer can emit `LoadVar("__self"); SetField(name)` without
/// needing a Dup-before-push-value trick (which would leave the value on top
/// rather than the object).
fn lower_constructor(decl: &TypeDecl, all_types: &[TypeDecl]) -> IrFunc {
    let params: Vec<String> = decl.type_params.iter().map(|p| p.name.clone()).collect();
    let mut body = Vec::new();
    let mut ctx = Ctx::new();

    // Create the object and bind it to __self (outer scope, outlives ancestor scopes).
    body.push(Instr::NewObject(decl.name.clone()));
    body.push(Instr::BeginScope);
    body.push(Instr::BindVar("__self".to_string()));

    let chain = ancestry_chain(&decl.name, all_types);

    // For each ancestor (root → this type), initialize its attributes.
    // Ancestor ctor params are re-bound in an inner scope using parent_args
    // supplied by the next type in the chain.
    for (i, ancestor) in chain.iter().enumerate() {
        let is_last = i == chain.len() - 1; // last entry is the type itself

        if is_last {
            // Own attrs: ctor params are already in scope (IrFunc params).
            lower_attrs_into_object(&decl.attributes, &mut body, &mut ctx);
        } else {
            // Ancestor attrs: bind ancestor ctor params to the args provided by
            // the child via `inherits Parent(args)`.
            let child = chain[i + 1];
            let ancestor_param_names: Vec<&str> =
                ancestor.type_params.iter().map(|p| p.name.as_str()).collect();

            if ancestor_param_names.is_empty() && ancestor.attributes.is_empty() {
                continue;
            }

            body.push(Instr::BeginScope);

            match &child.parent {
                Some(parent_spec) => match &parent_spec.args {
                    // Explicit args: evaluate them in the current scope.
                    Some(explicit_args) => {
                        for (param_name, arg_expr) in
                            ancestor_param_names.iter().zip(explicit_args.iter())
                        {
                            lower_inner(arg_expr, &mut body, &mut ctx);
                            body.push(Instr::BindVar(param_name.to_string()));
                        }
                    }
                    // No explicit args: forward the child's ctor params.
                    None => {
                        for param_name in &ancestor_param_names {
                            body.push(Instr::LoadVar(param_name.to_string()));
                            body.push(Instr::BindVar(param_name.to_string()));
                        }
                    }
                },
                None => {}
            }

            lower_attrs_into_object(&ancestor.attributes, &mut body, &mut ctx);
            body.push(Instr::EndScope);
        }
    }

    // Return the constructed object.
    body.push(Instr::LoadVar("__self".to_string()));
    body.push(Instr::EndScope);
    body.push(Instr::Ret);
    IrFunc { params, body }
}

/// For each attribute, emit:
///   `lower(attr.init); LoadVar("__self"); SetField(name); Pop`
///
/// This relies on `__self` being bound in an enclosing scope (see
/// `lower_constructor`). `SetField` pops the object (top) then the value,
/// then pushes the value back; we discard that return value with `Pop` so
/// the stack stays clean for subsequent attributes.
fn lower_attrs_into_object(attrs: &[AttrDecl], body: &mut Vec<Instr>, ctx: &mut Ctx) {
    for attr in attrs {
        lower_inner(&attr.init, body, ctx);          // push val
        body.push(Instr::LoadVar("__self".to_string())); // push obj on top
        body.push(Instr::SetField(attr.name.clone())); // pop obj, pop val, set, push val
        body.push(Instr::Pop);                        // discard val
    }
}

/// Lower a single method declaration into an IrFunc.
///
/// `type_name` is the type that declares this method.
/// `parent_name` is its immediate parent (needed to resolve `base`).
fn lower_method(type_name: &str, parent_name: &str, decl: &hulk_ast::MethodDecl) -> IrFunc {
    let mut params = vec!["self".to_string()];
    params.extend(decl.params.iter().map(|p| p.name.clone()));

    let mut body = Vec::new();
    let mut ctx = Ctx::new();
    ctx.current_type = Some(type_name.to_string());
    ctx.current_method = Some(decl.name.clone());
    ctx.parent_type = Some(parent_name.to_string());

    lower_inner(&decl.body, &mut body, &mut ctx);
    body.push(Instr::Ret);
    IrFunc { params, body }
}

/// Lower a complete [`Program`] to an [`IrProgram`].
pub fn lower_program(program: &Program) -> IrProgram {
    let mut funcs: HashMap<String, IrFunc> = program
        .functions
        .iter()
        .map(|f| (f.name.clone(), lower_func(f)))
        .collect();

    let mut types: HashMap<String, IrTypeInfo> = HashMap::new();

    for type_decl in &program.types {
        // Constructor.
        let ctor_name = format!("__ctor_{}", type_decl.name);
        funcs.insert(ctor_name, lower_constructor(type_decl, &program.types));

        // Methods.
        let parent_name = type_decl
            .parent
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Object".to_string());

        let mut method_map: HashMap<String, String> = HashMap::new();
        for method in &type_decl.methods {
            let func_name = format!("__method_{}_{}", type_decl.name, method.name);
            let ir_func = lower_method(&type_decl.name, &parent_name, method);
            funcs.insert(func_name.clone(), ir_func);
            method_map.insert(method.name.clone(), func_name);
        }

        types.insert(
            type_decl.name.clone(),
            IrTypeInfo {
                parent: parent_name,
                methods: method_map,
            },
        );
    }

    let mut entry = Vec::new();
    let mut ctx = Ctx::new();
    lower_inner(&program.entry, &mut entry, &mut ctx);
    entry.push(Instr::Ret);

    IrProgram { funcs, types, entry }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hulk_ast::{Expr, ExprKind, Span};

    fn num(n: f64) -> Expr {
        Expr {
            span: Span::default(),
            kind: ExprKind::Number(n),
        }
    }

    fn binop(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr {
            span: Span::default(),
            kind: ExprKind::BinOp(op, Box::new(lhs), Box::new(rhs)),
        }
    }

    fn call_print(arg: Expr) -> Expr {
        Expr {
            span: Span::default(),
            kind: ExprKind::Call("print".to_string(), vec![arg]),
        }
    }

    fn mk_program(entry: Expr) -> Program {
        Program {
            types: vec![],
            functions: vec![],
            entry,
        }
    }

    fn ident(name: &str) -> Expr {
        Expr {
            span: Span::default(),
            kind: ExprKind::Ident(name.to_string()),
        }
    }

    // ── pre-existing tests ───────────────────────────────────────────────────

    #[test]
    fn lower_number_emits_push_num() {
        let mut out = Vec::new();
        lower_expr(&num(42.0), &mut out);
        assert_eq!(out, vec![Instr::PushNum(42.0)]);
    }

    #[test]
    fn lower_addition_emits_operands_then_add() {
        let mut out = Vec::new();
        lower_expr(&binop(BinOp::Add, num(1.0), num(2.0)), &mut out);
        assert_eq!(
            out,
            vec![Instr::PushNum(1.0), Instr::PushNum(2.0), Instr::Add]
        );
    }

    #[test]
    fn lower_subtraction_preserves_operand_order() {
        let mut out = Vec::new();
        lower_expr(&binop(BinOp::Sub, num(10.0), num(3.0)), &mut out);
        assert_eq!(
            out,
            vec![Instr::PushNum(10.0), Instr::PushNum(3.0), Instr::Sub]
        );
    }

    #[test]
    fn lower_negation_emits_inner_then_neg() {
        let neg = Expr {
            span: Span::default(),
            kind: ExprKind::UnOp(UnOp::Neg, Box::new(num(5.0))),
        };
        let mut out = Vec::new();
        lower_expr(&neg, &mut out);
        assert_eq!(out, vec![Instr::PushNum(5.0), Instr::Neg]);
    }

    #[test]
    fn lower_print_emits_inner_then_print() {
        let mut out = Vec::new();
        lower_expr(&call_print(num(7.0)), &mut out);
        assert_eq!(out, vec![Instr::PushNum(7.0), Instr::Print]);
    }

    #[test]
    fn lower_program_entry_ends_with_ret() {
        let ir = lower_program(&mk_program(num(1.0)));
        assert_eq!(ir.entry.last(), Some(&Instr::Ret));
    }

    #[test]
    fn lower_nested_print_1_plus_2_pow_3() {
        let expr = call_print(binop(
            BinOp::Pow,
            binop(BinOp::Add, num(1.0), num(2.0)),
            num(3.0),
        ));
        let ir = lower_program(&mk_program(expr));
        assert_eq!(
            ir.entry,
            vec![
                Instr::PushNum(1.0),
                Instr::PushNum(2.0),
                Instr::Add,
                Instr::PushNum(3.0),
                Instr::Pow,
                Instr::Print,
                Instr::Ret,
            ]
        );
    }

    #[test]
    fn lower_let_emits_scope_bind_end() {
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::Let(
                "x".to_string(),
                None,
                Box::new(num(5.0)),
                Box::new(ident("x")),
            ),
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::BeginScope,
                Instr::PushNum(5.0),
                Instr::BindVar("x".to_string()),
                Instr::LoadVar("x".to_string()),
                Instr::EndScope,
            ]
        );
    }

    #[test]
    fn lower_assign_emits_dup_then_store() {
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::Assign("a".to_string(), Box::new(num(42.0))),
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::PushNum(42.0),
                Instr::Dup,
                Instr::StoreVar("a".to_string())
            ]
        );
    }

    #[test]
    fn lower_block_pops_all_but_last() {
        let block = Expr {
            span: Span::default(),
            kind: ExprKind::Block(vec![num(1.0), num(2.0), num(3.0)]),
        };
        let mut out = Vec::new();
        lower_expr(&block, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::PushNum(1.0),
                Instr::Pop,
                Instr::PushNum(2.0),
                Instr::Pop,
                Instr::PushNum(3.0),
            ]
        );
    }

    #[test]
    fn lower_builtin_constants_inline() {
        let pi = Expr {
            span: Span::default(),
            kind: ExprKind::Ident("PI".to_string()),
        };
        let mut out = Vec::new();
        lower_expr(&pi, &mut out);
        assert_eq!(out, vec![Instr::PushNum(std::f64::consts::PI)]);
    }

    // ── OOP-specific tests ───────────────────────────────────────────────────

    #[test]
    fn new_object_emits_ctor_call() {
        // new Dog("Rex")
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::New(
                "Dog".to_string(),
                vec![Expr {
                    span: Span::default(),
                    kind: ExprKind::String("Rex".to_string()),
                }],
            ),
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::PushStr("Rex".to_string()),
                Instr::Call("__ctor_Dog".to_string(), 1),
            ]
        );
    }

    #[test]
    fn self_expr_emits_load_var_self() {
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::SelfExpr,
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(out, vec![Instr::LoadVar("self".to_string())]);
    }

    #[test]
    fn get_field_emits_load_obj_then_get_field() {
        // self.name
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::GetField(
                Box::new(Expr {
                    span: Span::default(),
                    kind: ExprKind::SelfExpr,
                }),
                "name".to_string(),
            ),
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::LoadVar("self".to_string()),
                Instr::GetField("name".to_string()),
            ]
        );
    }

    #[test]
    fn method_call_emits_args_then_self_then_call_method() {
        // d.speak()
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::MethodCall(
                Box::new(ident("d")),
                "speak".to_string(),
                vec![],
            ),
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::LoadVar("d".to_string()),
                Instr::CallMethod("speak".to_string(), 0),
            ]
        );
    }

    #[test]
    fn is_type_emits_lower_then_is_type() {
        // d is Animal
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::Is(Box::new(ident("d")), "Animal".to_string()),
        };
        let mut out = Vec::new();
        lower_expr(&expr, &mut out);
        assert_eq!(
            out,
            vec![
                Instr::LoadVar("d".to_string()),
                Instr::IsType("Animal".to_string()),
            ]
        );
    }

    #[test]
    fn lower_program_generates_constructor_and_method_funcs() {
        use hulk_ast::{AttrDecl, MethodDecl, Param, TypeDecl};

        // type Point(x: Number) { x = x; zero() => 0; }
        let type_decl = TypeDecl {
            name: "Point".to_string(),
            type_params: vec![Param {
                name: "x".to_string(),
                ty: None,
                span: Span::default(),
            }],
            parent: None,
            attributes: vec![AttrDecl {
                name: "x".to_string(),
                ty: None,
                init: ident("x"),
                span: Span::default(),
            }],
            methods: vec![MethodDecl {
                name: "zero".to_string(),
                params: vec![],
                return_ty: None,
                body: num(0.0),
                span: Span::default(),
            }],
            span: Span::default(),
        };

        let program = Program {
            types: vec![type_decl],
            functions: vec![],
            entry: num(0.0),
        };

        let ir = lower_program(&program);

        assert!(ir.funcs.contains_key("__ctor_Point"), "missing __ctor_Point");
        assert!(
            ir.funcs.contains_key("__method_Point_zero"),
            "missing __method_Point_zero"
        );
        assert!(ir.types.contains_key("Point"), "missing type info for Point");

        let type_info = &ir.types["Point"];
        assert_eq!(type_info.parent, "Object");
        assert!(
            type_info.methods.contains_key("zero"),
            "method map missing 'zero'"
        );
        assert_eq!(type_info.methods["zero"], "__method_Point_zero");

        // Constructor body: NewObject + Dup + LoadVar("x") + SetField("x") + Ret
        let ctor = &ir.funcs["__ctor_Point"];
        assert_eq!(ctor.params, vec!["x"]);
        assert!(ctor.body.contains(&Instr::NewObject("Point".to_string())));
        assert!(ctor.body.contains(&Instr::SetField("x".to_string())));
    }
}
