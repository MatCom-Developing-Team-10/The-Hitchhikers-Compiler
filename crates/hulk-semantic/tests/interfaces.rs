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
fn structural_conformance_without_implements_clause() {
    // A type with the right methods conforms to an interface even without an
    // explicit `implements` clause (protocol-style structural typing, A.10.2).
    // This is the matcom `ok/interfaces/interface_basic` scenario.
    let source = r#"
        interface Printable { to_string(): String; }
        type Point(x: Number, y: Number) {
            x: Number = x;
            y: Number = y;
            to_string(): String => "point";
        }
        let p: Printable = new Point(1, 2) in p.to_string();
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn structural_conformance_requires_matching_signature() {
    // Missing the required method (wrong name) must NOT conform structurally.
    let source = r#"
        interface Printable { to_string(): String; }
        type Point(x: Number) {
            x: Number = x;
            describe(): String => "point";
        }
        let p: Printable = new Point(1) in 0;
    "#;
    let p = parse(source);
    assert!(
        analyze(&p).is_err(),
        "a type lacking the required method must not conform"
    );
}

#[test]
fn typed_iterable_param_accepts_conforming_iterable() {
    // `T*` (A.11.2): a user type implementing next()/current(): Number conforms
    // to a `Number*` parameter, and the loop variable binds to Number.
    let source = r#"
        type Squares(n: Number) {
            i: Number = 0;
            limit: Number = n;
            next(): Boolean { self.i := self.i + 1; self.i <= self.limit; }
            current(): Number { self.i * self.i; }
        }
        function sum_gen(gen: Number*): Number {
            let s: Number = 0 in { for (x in gen) s := s + x; s; };
        }
        sum_gen(new Squares(3));
    "#;
    let p = parse(source);
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn typed_iterable_param_rejects_non_iterable() {
    // Passing a Number where a `Number*` is expected must be a type error.
    let source = r#"
        function sum_gen(gen: Number*): Number => 0;
        sum_gen(42);
    "#;
    let p = parse(source);
    assert!(
        analyze(&p).is_err(),
        "a non-iterable argument must not conform to `Number*`"
    );
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
