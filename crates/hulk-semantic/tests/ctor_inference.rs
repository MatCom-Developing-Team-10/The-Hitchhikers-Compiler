//! Tests for constructor-parameter inference (HULK spec A.9.3).
//!
//! An un-annotated type/constructor parameter is inferred from the argument
//! types passed at its `new T(...)` call sites (the LCA across all sites). This
//! lets the book's canonical un-annotated OOP examples (A.7) type-check, while
//! staying monotonic: a param is only narrowed from the permissive `Object`
//! default, never the other way, so previously-accepted programs are unaffected.

use hulk_semantic::analyze;

fn parse(source: &str) -> hulk_semantic::ast::Program {
    hulk_parser::parse(source).expect("parse failed")
}

#[test]
fn unannotated_point_inferred_from_new_site() {
    // `type Point(x, y)` with no annotations; `new Point(3, 4)` forces x,y:Number,
    // so `getX()`/`getY()` are Number and the `@`/`+` below type-check.
    let source = r#"
        type Point(x, y) {
            x = x;
            y = y;
            getX() => self.x;
            getY() => self.y;
        }
        let p = new Point(3, 4) in {
            print("x: " @ p.getX());
            print(p.getX() + p.getY());
        }
    "#;
    let result = analyze(&parse(source));
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn unannotated_param_used_arithmetically_in_method() {
    // The param flows into an attribute used in arithmetic inside a method.
    let source = r#"
        type P(x) { x = x; sq() => self.x * self.x; }
        print(new P(5).sq());
    "#;
    let result = analyze(&parse(source));
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn unannotated_ctor_param_inferred_through_inheritance_forwarding() {
    // `Knight inherits Person` forwards Person's ctor params; the String args at
    // `new Knight("Phil","Collins")` must infer Person's params as String so the
    // `@@` concatenations type-check (A.7.4 polymorphism, verbatim).
    let source = r#"
        type Person(firstname, lastname) {
            firstname = firstname;
            lastname = lastname;
            name() => self.firstname @@ self.lastname;
        }
        type Knight inherits Person {
            name() => "Sir" @@ base();
        }
        print(new Knight("Phil", "Collins").name());
    "#;
    let result = analyze(&parse(source));
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn conflicting_new_sites_leave_param_as_object() {
    // Two call sites disagree (Number vs String), so the LCA is `Object` and the
    // param is NOT narrowed; arithmetic on it is then a type error (A.9.3 permits
    // the inferer to fail). This pins that the inference does not over-narrow.
    let source = r#"
        type Box(v) { v = v; doubled() => self.v + self.v; }
        {
            print(new Box(5).doubled());
            print(new Box("hi"));
        }
    "#;
    let result = analyze(&parse(source));
    assert!(
        result.is_err(),
        "conflicting sites must leave param as Object and reject the arithmetic"
    );
}

#[test]
fn annotated_ctor_params_are_never_overridden() {
    // An explicit annotation always wins, even if a call site would suggest a
    // different (incompatible) type — the call-site mismatch is the error.
    let source = r#"
        type N(x: Number) { x: Number = x; }
        new N("not a number");
    "#;
    let result = analyze(&parse(source));
    assert!(
        result.is_err(),
        "passing a String to an annotated Number ctor param must be rejected"
    );
}
