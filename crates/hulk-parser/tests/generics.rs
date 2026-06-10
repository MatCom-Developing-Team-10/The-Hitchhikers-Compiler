//! Parser tests for the generics extension (`type T[A]`, `function f[A]`,
//! `new T[Args](args)`, `is T[Args]`, `as T[Args]`, `let x: T[Args]`).

use hulk_ast::{ExprKind, TypeRef};
use hulk_parser::parse;

#[test]
fn parses_generic_type_declaration() {
    let prog = parse("type Box[T](item: T) { item = item; } 0;").unwrap();
    assert_eq!(prog.types.len(), 1);
    let t = &prog.types[0];
    assert_eq!(t.name, "Box");
    assert_eq!(t.generic_params, vec!["T".to_string()]);
    assert_eq!(t.type_params.len(), 1);
}

#[test]
fn parses_generic_type_with_multiple_params() {
    let prog = parse("type Pair[A, B](a: A, b: B) { a = a; b = b; } 0;").unwrap();
    let t = &prog.types[0];
    assert_eq!(t.generic_params, vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn parses_generic_type_without_ctor_params() {
    let prog = parse("type Empty[T] { } 0;").unwrap();
    let t = &prog.types[0];
    assert_eq!(t.generic_params, vec!["T".to_string()]);
    assert!(t.type_params.is_empty());
}

#[test]
fn parses_non_generic_type_still_works() {
    let prog = parse("type Point(x, y) { x = x; y = y; } 0;").unwrap();
    let t = &prog.types[0];
    assert!(t.generic_params.is_empty());
    assert_eq!(t.type_params.len(), 2);
}

#[test]
fn parses_generic_function_declaration() {
    let prog = parse("function id[T](x: T): T => x; 0;").unwrap();
    let f = &prog.functions[0];
    assert_eq!(f.name, "id");
    assert_eq!(f.generic_params, vec!["T".to_string()]);
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].ty, Some(TypeRef::Simple("T".into())));
}

#[test]
fn parses_generic_new_expression() {
    let prog = parse("new List[Number]();").unwrap();
    if let ExprKind::New(name, generics, args) = &prog.entry.kind {
        assert_eq!(name, "List");
        assert_eq!(generics, &vec![TypeRef::Simple("Number".into())]);
        assert!(args.is_empty());
    } else {
        panic!("expected New");
    }
}

#[test]
fn parses_nested_generic_in_let_annotation() {
    let prog = parse("let xs: List[Map[String, Number]] = 0 in xs;").unwrap();
    if let ExprKind::Let(_, Some(ty), _, _) = &prog.entry.kind {
        match ty {
            TypeRef::Generic(name, args) => {
                assert_eq!(name, "List");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    TypeRef::Generic(inner, inner_args) => {
                        assert_eq!(inner, "Map");
                        assert_eq!(inner_args.len(), 2);
                    }
                    _ => panic!("expected nested Generic"),
                }
            }
            _ => panic!("expected Generic at top"),
        }
    } else {
        panic!("expected Let with annotation");
    }
}

#[test]
fn parses_is_with_generic_type() {
    let prog = parse("x is List[Number];").unwrap();
    if let ExprKind::Is(_, ty) = &prog.entry.kind {
        assert_eq!(
            ty,
            &TypeRef::Generic("List".into(), vec![TypeRef::Simple("Number".into())])
        );
    } else {
        panic!("expected Is");
    }
}

#[test]
fn parses_as_with_generic_type() {
    let prog = parse("x as Map[String, Point];").unwrap();
    if let ExprKind::As(_, ty) = &prog.entry.kind {
        assert!(matches!(ty, TypeRef::Generic(name, _) if name == "Map"));
    } else {
        panic!("expected As");
    }
}

#[test]
fn parses_method_with_generic_param_type() {
    let source = r#"type Box[T](item: T) {
        item = item;
        get(): T => self.item;
    }
    0;"#;
    let prog = parse(source).unwrap();
    let t = &prog.types[0];
    assert_eq!(t.methods.len(), 1);
    assert_eq!(t.methods[0].return_ty, Some(TypeRef::Simple("T".into())));
}
