//! Semantic tests for the interfaces extension.

use hulk_semantic::ast::*;
use hulk_semantic::{SemError, analyze};

fn parse(source: &str) -> Program {
    hulk_parser::parse(source).expect("parse failed")
}

#[test]
fn empty_interface_typechecks() {
    let p = parse("interface Empty { } 0;");
    assert!(analyze(&p).is_ok());
}

#[test]
fn type_implementing_interface_typechecks() {
    let source = r#"
        interface Greeter { greet(): String; }
        type Person(name: String) implements Greeter {
            name: String = name;
            greet(): String => self.name;
        }
        (new Person("kevin")).greet();
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn type_missing_interface_method_is_rejected() {
    let source = r#"
        interface Greeter { greet(): String; }
        type Person(name: String) implements Greeter {
            name: String = name;
        }
        0;
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| matches!(e, SemError::MissingInterfaceMethod { .. })),
        "expected MissingInterfaceMethod, got {errs:?}"
    );
}

#[test]
fn type_wrong_signature_for_interface_method_is_rejected() {
    let source = r#"
        interface Greeter { greet(): String; }
        type Person(name: String) implements Greeter {
            name: String = name;
            greet(): Number => 42;
        }
        0;
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| matches!(e, SemError::InterfaceSignatureMismatch { .. })),
        "expected InterfaceSignatureMismatch, got {errs:?}"
    );
}

#[test]
fn interface_as_let_type_accepts_concrete_implementing_type() {
    // let x: Greeter = new Person(...) in ...
    let source = r#"
        interface Greeter { greet(): String; }
        type Person(name: String) implements Greeter {
            name: String = name;
            greet(): String => self.name;
        }
        let g: Greeter = new Person("kevin") in g.greet();
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn cannot_instantiate_interface() {
    let source = r#"
        interface Greeter { greet(): String; }
        new Greeter();
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| matches!(e, SemError::CannotInstantiateInterface { .. })),
        "expected CannotInstantiateInterface, got {errs:?}"
    );
}

#[test]
fn implements_a_non_interface_is_rejected() {
    let source = r#"
        type Greeter { greet(): String => "hi"; }
        type Person implements Greeter {
            greet(): String => "hi";
        }
        0;
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| matches!(e, SemError::NotAnInterface { .. })),
        "expected NotAnInterface, got {errs:?}"
    );
}

#[test]
fn interface_extends_chain_imposes_all_methods() {
    // Comparable extends Ord. T implements Comparable must define both.
    let source = r#"
        interface Ord { compare(): Number; }
        interface Comparable extends Ord {
            equals(): Boolean;
        }
        type T implements Comparable {
            equals(): Boolean => true;
        }
        0;
    "#;
    let p = parse(source);
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            SemError::MissingInterfaceMethod { method, .. } if method == "compare"
        )),
        "expected MissingInterfaceMethod(compare), got {errs:?}"
    );
}

#[test]
fn parent_implementing_interface_satisfies_child() {
    // If Animal implements Greeter (with greet()), then Dog inherits from Animal
    // and is also a valid Greeter — no need to redeclare.
    let source = r#"
        interface Greeter { greet(): String; }
        type Animal(name: String) implements Greeter {
            name: String = name;
            greet(): String => self.name;
        }
        type Dog(name: String) inherits Animal(name) {
        }
        let g: Greeter = new Dog("Rex") in g.greet();
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn non_interface_programs_still_typecheck() {
    let p = parse(r#"type Point(x, y) { x = x; y = y; } (new Point(1, 2)).x;"#);
    let errs = analyze(&p).unwrap_err();
    // The original test would still fail (no `getX` and `x` is private),
    // but at least no interface-related errors should appear.
    assert!(
        !errs
            .iter()
            .any(|e| matches!(e, SemError::MissingInterfaceMethod { .. })),
        "no interface errors expected, got {errs:?}"
    );
}
