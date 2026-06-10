//! Semantic tests for the generics extension.
//!
//! Coverage:
//! - Generic types are registered with their type parameters.
//! - `List[Number]` vs `List[String]` are NOT mutually conformant (invariance).
//! - `T` resolves to `Type::Param("T")` inside generic body, and conforms only
//!   to itself + `Object`.
//! - `new List[Number]()` produces `Type::Generic("List", [Number])`.
//! - Method/field lookup substitutes `T` by the concrete type argument.
//! - Arity mismatch in generic args is reported.

use hulk_semantic::ast::*;
use hulk_semantic::{SemError, analyze};

fn parse(source: &str) -> Program {
    hulk_parser::parse(source).expect("parse failed")
}

#[test]
fn empty_generic_type_typechecks() {
    let p = parse("type Box[T] { } 0;");
    assert!(analyze(&p).is_ok());
}

#[test]
fn generic_type_with_param_field_typechecks() {
    let source = r#"
        type Box[T](item: T) {
            item: T = item;
            get(): T => self.item;
        }
        0;
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn generic_function_returning_param_typechecks() {
    let source = r#"
        function id[T](x: T): T => x;
        0;
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn new_with_generic_args_typechecks() {
    let source = r#"
        type Box[T](item: T) {
            item: T = item;
        }
        new Box[Number](42);
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn new_with_wrong_arity_of_generic_args_is_rejected() {
    let source = r#"
        type Pair[A, B](a: A, b: B) {
            a: A = a;
            b: B = b;
        }
        new Pair[Number](1, 2);
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(e, SemError::Arity { .. })),
        "expected Arity error, got {errs:?}"
    );
}

#[test]
fn method_call_on_generic_returns_substituted_type() {
    // `(new Box[Number](42)).get()` must typecheck against `Number`.
    let source = r#"
        type Box[T](item: T) {
            item: T = item;
            get(): T => self.item;
        }
        let x: Number = (new Box[Number](42)).get() in x;
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn method_call_on_generic_with_wrong_let_annotation_is_rejected() {
    // `Box[String].get()` returns `String`, not `Number`.
    let source = r#"
        type Box[T](item: T) {
            item: T = item;
            get(): T => self.item;
        }
        let x: Number = (new Box[String]("hi")).get() in x;
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(e, SemError::Mismatch { .. })),
        "expected Mismatch error, got {errs:?}"
    );
}

#[test]
fn ctor_arg_type_mismatch_on_generic_is_rejected() {
    // `new Box[Number]("hi")` — String is not Number.
    let source = r#"
        type Box[T](item: T) {
            item: T = item;
        }
        new Box[Number]("hi");
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(e, SemError::Mismatch { .. })),
        "expected Mismatch error, got {errs:?}"
    );
}

#[test]
fn non_generic_types_still_typecheck() {
    // Regression: confirm that backward compat is preserved.
    let source = r#"
        type Point(x: Number, y: Number) {
            x: Number = x;
            y: Number = y;
            getX(): Number => self.x;
        }
        (new Point(1, 2)).getX();
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn list_of_number_does_not_conform_to_list_of_string() {
    // `let x: List[String] = new List[Number]()` must fail.
    let source = r#"
        type List[T] { }
        let x: List[String] = new List[Number]() in 0;
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(e, SemError::Mismatch { .. })),
        "expected Mismatch (invariance), got {errs:?}"
    );
}
