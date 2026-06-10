//! Parser tests for the interfaces extension.

use hulk_ast::TypeRef;
use hulk_parser::parse;

#[test]
fn parses_empty_interface() {
    let prog = parse("interface Empty { } 0;").unwrap();
    assert_eq!(prog.interfaces.len(), 1);
    assert_eq!(prog.interfaces[0].name, "Empty");
    assert!(prog.interfaces[0].methods.is_empty());
}

#[test]
fn parses_interface_with_methods() {
    let source = r#"
        interface Comparable {
            compare(other: Comparable): Number;
            min(): Number;
        }
        0;
    "#;
    let prog = parse(source).unwrap();
    let i = &prog.interfaces[0];
    assert_eq!(i.name, "Comparable");
    assert_eq!(i.methods.len(), 2);
    assert_eq!(i.methods[0].name, "compare");
    assert_eq!(
        i.methods[0].return_ty,
        Some(TypeRef::Simple("Number".into()))
    );
}

#[test]
fn parses_generic_interface() {
    let source = r#"
        interface Container[T] {
            get(): T;
            put(x: T): T;
        }
        0;
    "#;
    let prog = parse(source).unwrap();
    let i = &prog.interfaces[0];
    assert_eq!(i.generic_params, vec!["T".to_string()]);
}

#[test]
fn parses_interface_extends() {
    let source = r#"
        interface Ord { compare(): Number; }
        interface Comparable extends Ord {
            equals(): Boolean;
        }
        0;
    "#;
    let prog = parse(source).unwrap();
    let comparable = &prog.interfaces[1];
    assert_eq!(comparable.extends.len(), 1);
    assert_eq!(comparable.extends[0], TypeRef::Simple("Ord".into()));
}

#[test]
fn parses_type_implements() {
    let source = r#"
        interface Greeter { greet(): String; }
        type Person(name: String) implements Greeter {
            name = name;
            greet(): String => self.name;
        }
        0;
    "#;
    let prog = parse(source).unwrap();
    let p = &prog.types[0];
    assert_eq!(p.implements, vec![TypeRef::Simple("Greeter".into())]);
}

#[test]
fn parses_type_implements_multiple_interfaces() {
    let source = r#"
        interface A { fa(): Number; }
        interface B { fb(): Number; }
        type T implements A, B {
            fa(): Number => 1;
            fb(): Number => 2;
        }
        0;
    "#;
    let prog = parse(source).unwrap();
    let t = &prog.types[0];
    assert_eq!(t.implements.len(), 2);
}

#[test]
fn parses_type_with_inherits_and_implements() {
    let source = r#"
        interface Greeter { greet(): String; }
        type Animal(name: String) {
            name = name;
            greet(): String => self.name;
        }
        type Dog(name: String) inherits Animal(name) implements Greeter {
            bark(): String => "woof";
        }
        0;
    "#;
    let prog = parse(source).unwrap();
    let dog = &prog.types[1];
    assert!(dog.parent.is_some());
    assert_eq!(dog.implements.len(), 1);
}

#[test]
fn parses_non_interface_program_still_works() {
    // Backwards-compat regression: no interface = empty Vec.
    let prog = parse("type Point(x, y) { x = x; y = y; } 0;").unwrap();
    assert!(prog.interfaces.is_empty());
    assert!(prog.types[0].implements.is_empty());
}
