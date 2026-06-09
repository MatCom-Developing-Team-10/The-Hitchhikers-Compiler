//! Bytecode IR and lowering for the HULK compiler.
//!
//! See `bytecode.spec.md` (v0.1) and `ir-v2.spec.md` (v0.2) for the full
//! specification of the instruction set and lowering rules.

use std::collections::HashMap;

use hulk_ast::{BinOp, Expr, ExprKind, FunctionDecl, Program, UnOp};

// ── Value ────────────────────────────────────────────────────────────────────

/// A runtime value on the VM stack.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// IEEE-754 double.
    Num(f64),
    /// Boolean.
    Bool(bool),
    /// Owned string.
    Str(String),
    /// Nil — result of a void expression (e.g. a while that never executes).
    Nil,
}

impl Value {
    /// Short human-readable type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "Number",
            Value::Bool(_) => "Boolean",
            Value::Str(_) => "String",
            Value::Nil => "Nil",
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

/// A complete lowered program.
#[derive(Debug)]
pub struct IrProgram {
    /// User-defined functions, keyed by name.
    pub funcs: HashMap<String, IrFunc>,
    /// Entry-point instructions; always ends with `Ret`.
    pub entry: Vec<Instr>,
}

// ── Label counter ─────────────────────────────────────────────────────────────

struct Ctx {
    counter: usize,
}

impl Ctx {
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
///
/// Panics on any `ExprKind` outside the supported scope (OOP, `for`).
pub fn lower_expr(expr: &Expr, out: &mut Vec<Instr>) {
    let mut ctx = Ctx { counter: 0 };
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

        ExprKind::New(_, _)
        | ExprKind::MethodCall(_, _, _)
        | ExprKind::GetField(_, _)
        | ExprKind::AssignField(_, _, _)
        | ExprKind::Base(_)
        | ExprKind::SelfExpr
        | ExprKind::Is(_, _)
        | ExprKind::As(_, _) => {
            panic!("OOP features are not supported in the current backend scope");
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

    // First condition.
    lower_inner(cond, out, ctx);
    let first_else = if elifs.is_empty() {
        format!("else_{id}")
    } else {
        format!("elif_{id}_0")
    };
    out.push(Instr::JumpIfFalse(first_else));
    lower_inner(then, out, ctx);
    out.push(Instr::Jump(end_label.clone()));

    // elif branches.
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

    // else branch.
    out.push(Instr::Label(format!("else_{id}")));
    lower_inner(else_, out, ctx);
    out.push(Instr::Label(end_label));
}

fn lower_while(cond: &Expr, body: &Expr, out: &mut Vec<Instr>, ctx: &mut Ctx) {
    let n = ctx.next();
    let start = format!("loop_start_{n}");
    let end = format!("loop_end_{n}");

    out.push(Instr::PushNil); // default value if loop never executes
    out.push(Instr::Label(start.clone()));
    lower_inner(cond, out, ctx);
    out.push(Instr::JumpIfFalse(end.clone()));
    out.push(Instr::Pop); // discard previous iteration's value
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
    let mut ctx = Ctx { counter: 0 };
    lower_inner(&decl.body, &mut body, &mut ctx);
    body.push(Instr::Ret);
    IrFunc { params, body }
}

/// Lower a complete [`Program`] to an [`IrProgram`].
pub fn lower_program(program: &Program) -> IrProgram {
    let funcs = program
        .functions
        .iter()
        .map(|f| (f.name.clone(), lower_func(f)))
        .collect();

    let mut entry = Vec::new();
    let mut ctx = Ctx { counter: 0 };
    lower_inner(&program.entry, &mut entry, &mut ctx);
    entry.push(Instr::Ret);

    IrProgram { funcs, entry }
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
        // let x = 5 in x
        let expr = Expr {
            span: Span::default(),
            kind: ExprKind::Let(
                "x".to_string(),
                None,
                Box::new(num(5.0)),
                Box::new(Expr {
                    span: Span::default(),
                    kind: ExprKind::Ident("x".to_string()),
                }),
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
        // a := 42
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
        // { 1; 2; 3 }
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
        // PI
        let pi = Expr {
            span: Span::default(),
            kind: ExprKind::Ident("PI".to_string()),
        };
        let mut out = Vec::new();
        lower_expr(&pi, &mut out);
        assert_eq!(out, vec![Instr::PushNum(std::f64::consts::PI)]);
    }
}
