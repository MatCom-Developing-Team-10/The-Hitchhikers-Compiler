//! Bytecode IR and lowering for the HULK compiler.
//!
//! See `bytecode.spec.md` for the full specification.
//! Week-1 scope: numeric arithmetic and `print` only.

use hulk_ast::{BinOp, Expr, ExprKind, Program, UnOp};

/// A single stack-machine instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    /// Push a numeric literal onto the stack.
    PushNum(f64),
    /// Pop `b`, pop `a`, push `a + b`.
    Add,
    /// Pop `b`, pop `a`, push `a - b`.
    Sub,
    /// Pop `b`, pop `a`, push `a * b`.
    Mul,
    /// Pop `b`, pop `a`, push `a / b`.
    Div,
    /// Pop `b`, pop `a`, push `a ^ b`.
    Pow,
    /// Pop `b`, pop `a`, push `a % b`.
    Mod,
    /// Pop `a`, push `-a`.
    Neg,
    /// Pop `a` and print it to stdout. Leaves nothing on the stack.
    Print,
    /// End execution.
    Ret,
}

/// Lower a single expression, appending instructions to `out`.
///
/// Invariant: each call adds exactly one net value to the stack at runtime,
/// except for a `print(...)` call which adds zero.
///
/// Panics if `expr` contains any `ExprKind` outside the Week-1 scope.
pub fn lower_expr(expr: &Expr, out: &mut Vec<Instr>) {
    match &expr.kind {
        ExprKind::Number(n) => {
            out.push(Instr::PushNum(*n));
        }

        ExprKind::BinOp(op, lhs, rhs) => {
            lower_expr(lhs, out);
            lower_expr(rhs, out);
            match op {
                BinOp::Add => out.push(Instr::Add),
                BinOp::Sub => out.push(Instr::Sub),
                BinOp::Mul => out.push(Instr::Mul),
                BinOp::Div => out.push(Instr::Div),
                BinOp::Pow => out.push(Instr::Pow),
                BinOp::Mod => out.push(Instr::Mod),
                other => panic!("unsupported binary operator in week-1 lowering: {other:?}"),
            }
        }

        ExprKind::UnOp(op, inner) => {
            lower_expr(inner, out);
            match op {
                UnOp::Neg => out.push(Instr::Neg),
                other => panic!("unsupported unary operator in week-1 lowering: {other:?}"),
            }
        }

        ExprKind::Call(name, args) if name == "print" => {
            assert_eq!(
                args.len(),
                1,
                "print expects exactly 1 argument, got {}",
                args.len()
            );
            lower_expr(&args[0], out);
            out.push(Instr::Print);
        }

        ExprKind::Call(name, _) => {
            panic!("unsupported call in week-1 lowering: {name}");
        }

        other => panic!("unsupported expr kind in week-1 lowering: {other:?}"),
    }
}

/// Lower a complete program to a flat instruction sequence.
///
/// Lowers `program.entry` and appends `Ret` at the end.
pub fn lower_program(program: &Program) -> Vec<Instr> {
    let mut out = Vec::new();
    lower_expr(&program.entry, &mut out);
    out.push(Instr::Ret);
    out
}

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
        // lhs first, rhs second — VM pops rhs then lhs
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
    fn lower_program_ends_with_ret() {
        let program = Program {
            types: vec![],
            functions: vec![],
            entry: num(1.0),
        };
        let instrs = lower_program(&program);
        assert_eq!(instrs.last(), Some(&Instr::Ret));
    }

    #[test]
    fn lower_nested_expression_print_1_plus_2_pow_3() {
        // print((1 + 2) ^ 3)  →  PushNum(1), PushNum(2), Add, PushNum(3), Pow, Print, Ret
        let expr = call_print(binop(BinOp::Pow, binop(BinOp::Add, num(1.0), num(2.0)), num(3.0)));
        let program = Program {
            types: vec![],
            functions: vec![],
            entry: expr,
        };
        let instrs = lower_program(&program);
        assert_eq!(
            instrs,
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
}
