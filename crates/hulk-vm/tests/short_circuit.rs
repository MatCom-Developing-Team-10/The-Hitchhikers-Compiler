//! End-to-end tests for short-circuit evaluation of `&` and `|`.
//!
//! Each program puts a would-be runtime error (division by zero) on the side
//! that must NOT be evaluated, so a successful run proves the operand was
//! skipped, and an error proves it was evaluated.

fn run(source: &str) -> Result<(), hulk_vm::VmError> {
    let ast = hulk_parser::parse(source).expect("parse");
    hulk_semantic::analyze(&ast).expect("semantic");
    let ir = hulk_ir::lower_program(&ast);
    hulk_vm::Vm::run_program(ir)
}

#[test]
fn and_skips_rhs_when_lhs_false() {
    // false & (1/0 == 0): rhs must not run → no DivisionByZero.
    assert!(run("print(false & (1 / 0 == 0));").is_ok());
}

#[test]
fn or_skips_rhs_when_lhs_true() {
    // true | (1/0 == 0): rhs must not run → no DivisionByZero.
    assert!(run("print(true | (1 / 0 == 0));").is_ok());
}

#[test]
fn and_evaluates_rhs_when_lhs_true() {
    // true & (1/0 == 0): rhs IS evaluated → DivisionByZero.
    assert_eq!(
        run("print(true & (1 / 0 == 0));"),
        Err(hulk_vm::VmError::DivisionByZero)
    );
}

#[test]
fn or_evaluates_rhs_when_lhs_false() {
    // false | (1/0 == 0): rhs IS evaluated → DivisionByZero.
    assert_eq!(
        run("print(false | (1 / 0 == 0));"),
        Err(hulk_vm::VmError::DivisionByZero)
    );
}
