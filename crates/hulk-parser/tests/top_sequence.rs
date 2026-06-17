//! Parser tests for the top-level entry point: HULK is expression-based, so the
//! program body is an implicit block — a `;`-separated statement sequence whose
//! value is the last expression (A.2.4). A lone expression must NOT be wrapped in
//! a `Block`, so single-expression programs keep producing identical ASTs.

use hulk_ast::ExprKind;
use hulk_parser::parse;

#[test]
fn single_expression_entry_is_not_wrapped_in_a_block() {
    // `expr` and `expr;` both yield the bare expression as the entry (no Block).
    for src in ["print(42)", "print(42);"] {
        let prog = parse(src).unwrap();
        assert!(
            !matches!(prog.entry.kind, ExprKind::Block(_)),
            "single-expression entry must not be wrapped in a Block (src = {src:?})"
        );
        assert!(matches!(prog.entry.kind, ExprKind::Call(ref name, _) if name == "print"));
    }
}

#[test]
fn top_level_statement_sequence_desugars_to_a_block() {
    // A bare `;`-separated sequence at the top level (no braces) becomes a Block
    // whose statements are the sequence, in order.
    let prog = parse("print(1); print(2); print(3);").unwrap();
    match &prog.entry.kind {
        ExprKind::Block(stmts) => assert_eq!(stmts.len(), 3),
        other => panic!("expected a Block entry, got {other:?}"),
    }
}

#[test]
fn top_level_sequence_without_trailing_semicolon() {
    let prog = parse("print(1); print(2)").unwrap();
    assert!(matches!(prog.entry.kind, ExprKind::Block(ref s) if s.len() == 2));
}

#[test]
fn declarations_then_statement_sequence() {
    // Declarations may precede a flat statement sequence.
    let prog = parse("function f() => 1; print(f()); print(2);").unwrap();
    assert_eq!(prog.functions.len(), 1);
    assert!(matches!(prog.entry.kind, ExprKind::Block(ref s) if s.len() == 2));
}
