//! End-to-end tests for the semantic analyzer.
//!
//! We construct `Program` values directly via the AST contract so the tests
//! don't depend on the lexer/parser (those are role A's responsibility). Each
//! test exercises one rule from the HULK book and asserts either success or
//! a specific `SemError` variant.

use hulk_semantic::ast::*;
use hulk_semantic::{SemError, analyze};

// ---------- small helpers ----------

fn e(kind: ExprKind) -> Expr {
    Expr {
        span: Span::default(),
        kind,
    }
}

fn n(v: f64) -> Expr {
    e(ExprKind::Number(v))
}
fn s(v: &str) -> Expr {
    e(ExprKind::String(v.into()))
}
fn id(name: &str) -> Expr {
    e(ExprKind::Ident(name.into()))
}
fn call(name: &str, args: Vec<Expr>) -> Expr {
    e(ExprKind::Call(name.into(), args))
}
fn binop(op: BinOp, l: Expr, r: Expr) -> Expr {
    e(ExprKind::BinOp(op, Box::new(l), Box::new(r)))
}

fn b(v: bool) -> Expr {
    e(ExprKind::Bool(v))
}

fn prog(entry: Expr) -> Program {
    Program {
        interfaces: vec![],
        types: vec![],
        functions: vec![],
        entry,
    }
}

// ---------- positive: arithmetic, strings, let, if, while ----------

#[test]
fn arithmetic_typechecks() {
    // print((1 + 2) * 3)
    let p = prog(call(
        "print",
        vec![binop(BinOp::Mul, binop(BinOp::Add, n(1.0), n(2.0)), n(3.0))],
    ));
    assert!(analyze(&p).is_ok());
}

#[test]
fn string_concat_typechecks() {
    // print("life is " @ 42)
    let p = prog(call(
        "print",
        vec![binop(BinOp::Concat, s("life is "), n(42.0))],
    ));
    assert!(analyze(&p).is_ok());
}

#[test]
fn nested_let_shadowing() {
    // let a = 1 in let a = a + 1 in print(a)
    let p = prog(e(ExprKind::Let(
        "a".into(),
        None,
        Box::new(n(1.0)),
        Box::new(e(ExprKind::Let(
            "a".into(),
            None,
            Box::new(binop(BinOp::Add, id("a"), n(1.0))),
            Box::new(call("print", vec![id("a")])),
        ))),
    )));
    assert!(analyze(&p).is_ok());
}

#[test]
fn if_else_lca_is_string() {
    // if (true) "a" else "b"
    let p = prog(e(ExprKind::If(
        Box::new(e(ExprKind::Bool(true))),
        Box::new(s("a")),
        vec![],
        Box::new(s("b")),
    )));
    assert!(analyze(&p).is_ok());
}

#[test]
fn while_with_destructive_assign() {
    // let a = 3 in while (a > 0) a := a - 1
    let p = prog(e(ExprKind::Let(
        "a".into(),
        None,
        Box::new(n(3.0)),
        Box::new(e(ExprKind::While(
            Box::new(binop(BinOp::Gt, id("a"), n(0.0))),
            Box::new(e(ExprKind::Assign(
                "a".into(),
                Box::new(binop(BinOp::Sub, id("a"), n(1.0))),
            ))),
        ))),
    )));
    assert!(analyze(&p).is_ok());
}

#[test]
fn builtin_constants_pi_and_e() {
    // print(PI + E)
    let p = prog(call("print", vec![binop(BinOp::Add, id("PI"), id("E"))]));
    assert!(analyze(&p).is_ok());
}

// ---------- positive: OOP (types, inheritance, base) ----------

fn method(name: &str, return_ty: Option<&str>, body: Expr) -> MethodDecl {
    MethodDecl {
        name: name.into(),
        params: vec![],
        return_ty: return_ty.map(TypeRef::from),
        body,
        span: Span::default(),
    }
}

#[test]
fn type_attributes_and_method() {
    // type Point(x: Number, y: Number) {
    //     x: Number = x;
    //     y: Number = y;
    //     getX(): Number => self.x;
    // }
    // (new Point(3, 4)).getX()
    let point = TypeDecl {
        name: "Point".into(),
        generic_params: vec![],
        type_params: vec![
            Param {
                name: "x".into(),
                ty: Some("Number".into()),
                span: Span::default(),
            },
            Param {
                name: "y".into(),
                ty: Some("Number".into()),
                span: Span::default(),
            },
        ],
        parent: None,
        implements: vec![],
        attributes: vec![
            AttrDecl {
                name: "x".into(),
                ty: Some("Number".into()),
                init: id("x"),
                span: Span::default(),
            },
            AttrDecl {
                name: "y".into(),
                ty: Some("Number".into()),
                init: id("y"),
                span: Span::default(),
            },
        ],
        methods: vec![method(
            "getX",
            Some("Number"),
            e(ExprKind::GetField(
                Box::new(e(ExprKind::SelfExpr)),
                "x".into(),
            )),
        )],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![point],
        functions: vec![],
        entry: e(ExprKind::MethodCall(
            Box::new(e(ExprKind::New(
                "Point".into(),
                vec![],
                vec![n(3.0), n(4.0)],
            ))),
            "getX".into(),
            vec![],
        )),
    };
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

#[test]
fn inheritance_with_base_call() {
    // type Person { greet(): String => "hi"; }
    // type Knight inherits Person { greet(): String => "sir " @ base(); }
    // (new Knight()).greet()
    let person = TypeDecl {
        name: "Person".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: None,
        implements: vec![],
        attributes: vec![],
        methods: vec![method("greet", Some("String"), s("hi"))],
        span: Span::default(),
    };
    let knight = TypeDecl {
        name: "Knight".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: Some(ParentSpec {
            name: "Person".into(),
            args: None,
            span: Span::default(),
        }),
        implements: vec![],
        attributes: vec![],
        methods: vec![method(
            "greet",
            Some("String"),
            binop(BinOp::Concat, s("sir "), e(ExprKind::Base(vec![]))),
        )],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![person, knight],
        functions: vec![],
        entry: e(ExprKind::MethodCall(
            Box::new(e(ExprKind::New("Knight".into(), vec![], vec![]))),
            "greet".into(),
            vec![],
        )),
    };
    let result = analyze(&p);
    assert!(result.is_ok(), "expected ok, got {:?}", result.err());
}

// ---------- negative: every SemError variant we can reach by construction --

fn first_err(p: &Program) -> SemError {
    analyze(p)
        .expect_err("expected at least one error")
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn undefined_variable_reported() {
    let p = prog(id("nope"));
    assert!(matches!(first_err(&p), SemError::UndefinedVariable { .. }));
}

#[test]
fn undefined_function_reported() {
    let p = prog(call("nope", vec![]));
    assert!(matches!(first_err(&p), SemError::UndefinedFunction { .. }));
}

#[test]
fn type_mismatch_in_let_annotation() {
    // let x: Number = "hi" in x
    let p = prog(e(ExprKind::Let(
        "x".into(),
        Some("Number".into()),
        Box::new(s("hi")),
        Box::new(id("x")),
    )));
    assert!(matches!(first_err(&p), SemError::Mismatch { .. }));
}

#[test]
fn cannot_inherit_from_builtin() {
    let bad = TypeDecl {
        name: "Bad".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: Some(ParentSpec {
            name: "Number".into(),
            args: None,
            span: Span::default(),
        }),
        implements: vec![],
        attributes: vec![],
        methods: vec![],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![bad],
        functions: vec![],
        entry: n(0.0),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|x| matches!(x, SemError::InheritBuiltin { .. }))
    );
}

#[test]
fn override_with_different_signature() {
    // type P { f(): Number => 1; }
    // type Q inherits P { f(): String => "x"; }     <-- mismatch
    let p_type = TypeDecl {
        name: "P".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: None,
        implements: vec![],
        attributes: vec![],
        methods: vec![method("f", Some("Number"), n(1.0))],
        span: Span::default(),
    };
    let q_type = TypeDecl {
        name: "Q".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: Some(ParentSpec {
            name: "P".into(),
            args: None,
            span: Span::default(),
        }),
        implements: vec![],
        attributes: vec![],
        methods: vec![method("f", Some("String"), s("x"))],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![p_type, q_type],
        functions: vec![],
        entry: n(0.0),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|x| matches!(x, SemError::OverrideSignatureMismatch { .. }))
    );
}

#[test]
fn self_assign_is_rejected() {
    // type T { f(): Object { self := new T(); } }    -- artificial, just for the rule
    // We exercise the rule directly through an Assign target named "self".
    let t = TypeDecl {
        name: "T".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: None,
        implements: vec![],
        attributes: vec![],
        methods: vec![method(
            "f",
            None,
            e(ExprKind::Assign(
                "self".into(),
                Box::new(e(ExprKind::New("T".into(), vec![], vec![]))),
            )),
        )],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![t],
        functions: vec![],
        entry: e(ExprKind::MethodCall(
            Box::new(e(ExprKind::New("T".into(), vec![], vec![]))),
            "f".into(),
            vec![],
        )),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|x| matches!(x, SemError::SelfAssign { .. }))
    );
}

#[test]
fn duplicate_type_reported() {
    let dup = |n: &str| TypeDecl {
        name: n.into(),
        generic_params: vec![],
        type_params: vec![],
        parent: None,
        implements: vec![],
        attributes: vec![],
        methods: vec![],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![dup("X"), dup("X")],
        functions: vec![],
        entry: n(0.0),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|x| matches!(x, SemError::DuplicateType { .. }))
    );
}

#[test]
fn arity_mismatch_on_builtin_call() {
    // sqrt(1, 2)   -- sqrt expects 1 arg
    let p = prog(call("sqrt", vec![n(1.0), n(2.0)]));
    assert!(matches!(first_err(&p), SemError::Arity { .. }));
}

#[test]
fn base_outside_override_is_rejected() {
    let t = TypeDecl {
        name: "Solo".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: None,
        implements: vec![],
        attributes: vec![],
        // `solo()` has no parent method to call — `base()` should be flagged.
        methods: vec![method("solo", None, e(ExprKind::Base(vec![])))],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![t],
        functions: vec![],
        entry: e(ExprKind::MethodCall(
            Box::new(e(ExprKind::New("Solo".into(), vec![], vec![]))),
            "solo".into(),
            vec![],
        )),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(errs.iter().any(|x| matches!(
        x,
        SemError::BaseNoParentMethod { .. } | SemError::BaseOutsideOverride { .. }
    )));
}

#[test]
fn errors_do_not_cascade() {
    // print(undefined_var + 1)
    // We expect ONE error (UndefinedVariable), not a cascade like "+ expects Number".
    let p = prog(call(
        "print",
        vec![binop(BinOp::Add, id("undefined_var"), n(1.0))],
    ));
    let errs = analyze(&p).unwrap_err();
    assert_eq!(errs.len(), 1, "expected single error, got: {errs:?}");
    assert!(matches!(errs[0], SemError::UndefinedVariable { .. }));
}

#[test]
fn is_requires_defined_type() {
    let p = prog(e(ExprKind::Is(Box::new(n(1.0)), "Nope".into())));
    assert!(matches!(first_err(&p), SemError::UndefinedType { .. }));
}

#[test]
fn concat_rejects_non_string_number_operands() {
    let p = prog(call("print", vec![binop(BinOp::Concat, b(true), s("x"))]));
    assert!(matches!(first_err(&p), SemError::Mismatch { .. }));
}

#[test]
fn explicit_inherits_args_are_typechecked() {
    // type P(x: Number) { }
    // type C inherits P("hi") { }    -- mismatch
    let p_type = TypeDecl {
        name: "P".into(),
        generic_params: vec![],
        type_params: vec![Param {
            name: "x".into(),
            ty: Some("Number".into()),
            span: Span::default(),
        }],
        parent: None,
        implements: vec![],
        attributes: vec![],
        methods: vec![],
        span: Span::default(),
    };
    let c_type = TypeDecl {
        name: "C".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: Some(ParentSpec {
            name: "P".into(),
            args: Some(vec![s("hi")]),
            span: Span::default(),
        }),
        implements: vec![],
        attributes: vec![],
        methods: vec![],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![p_type, c_type],
        functions: vec![],
        entry: n(0.0),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(errs.iter().any(|x| matches!(x, SemError::Mismatch { .. })));
}

#[test]
fn reserved_type_names_are_rejected() {
    let t = TypeDecl {
        name: "Number".into(),
        generic_params: vec![],
        type_params: vec![],
        parent: None,
        implements: vec![],
        attributes: vec![],
        methods: vec![],
        span: Span::default(),
    };
    let p = Program {
        interfaces: vec![],
        types: vec![t],
        functions: vec![],
        entry: n(0.0),
    };
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|x| matches!(x, SemError::ReservedTypeName { .. }))
    );
}

#[test]
fn reserved_let_binder_names_are_rejected() {
    // let self = 1 in 2
    let p = prog(e(ExprKind::Let(
        "self".into(),
        None,
        Box::new(n(1.0)),
        Box::new(n(2.0)),
    )));
    let errs = analyze(&p).unwrap_err();
    assert!(
        errs.iter()
            .any(|x| matches!(x, SemError::ReservedName { .. }))
    );
}
