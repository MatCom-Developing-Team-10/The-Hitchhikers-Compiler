//! LALRpop-based parser for the HULK language.
//!
//! The grammar lives in `grammar.lalrpop` and is generated into `grammar.rs`
//! by the build script.

use hulk_ast::{AttrDecl, MethodDecl, Program};
use hulk_lexer::{LexError, Lexer, Token};
use lalrpop_util::lalrpop_mod;

lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    pub(crate) grammar
);

/// Parser error type.
pub type ParseError<'input> = lalrpop_util::ParseError<usize, Token, LexError>;

/// Helper enum used internally by the grammar to distinguish type members
/// during parsing.
#[derive(Debug)]
pub enum TypeMemberKind {
    /// An attribute declaration.
    Attr(AttrDecl),
    /// A method declaration.
    Method(MethodDecl),
}

/// Parse a HULK source string into a [`Program`] AST.
pub fn parse(source: &str) -> Result<Program, ParseError<'_>> {
    let lexer = Lexer::new(source);
    grammar::ProgramParser::new().parse(source, lexer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hulk_ast::*;

    #[test]
    fn parses_number_literal() {
        let prog = parse("42;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::Number(n) if (n - 42.0).abs() < f64::EPSILON));
    }

    #[test]
    fn parses_string_literal() {
        let prog = parse(r#""Hello World";"#).unwrap();
        assert!(matches!(&prog.entry.kind, ExprKind::String(s) if s == "Hello World"));
    }

    #[test]
    fn parses_boolean_literals() {
        let prog = parse("true;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::Bool(true)));

        let prog = parse("false;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::Bool(false)));
    }

    #[test]
    fn parses_arithmetic_precedence() {
        // 1 + 2 * 3 should be 1 + (2 * 3)
        let prog = parse("1 + 2 * 3;").unwrap();
        if let ExprKind::BinOp(BinOp::Add, lhs, rhs) = &prog.entry.kind {
            assert!(matches!(lhs.kind, ExprKind::Number(_)));
            assert!(matches!(rhs.kind, ExprKind::BinOp(BinOp::Mul, _, _)));
        } else {
            panic!("expected Add at top level");
        }
    }

    #[test]
    fn parses_power_right_associative() {
        // 2 ^ 3 ^ 4 should be 2 ^ (3 ^ 4)
        let prog = parse("2 ^ 3 ^ 4;").unwrap();
        if let ExprKind::BinOp(BinOp::Pow, _base, exp) = &prog.entry.kind {
            assert!(matches!(exp.kind, ExprKind::BinOp(BinOp::Pow, _, _)));
        } else {
            panic!("expected Pow at top level");
        }
    }

    #[test]
    fn parses_concat_right_associative() {
        let prog = parse(r#""a" @ "b" @ "c";"#).unwrap();
        if let ExprKind::BinOp(BinOp::Concat, _lhs, rhs) = &prog.entry.kind {
            assert!(matches!(rhs.kind, ExprKind::BinOp(BinOp::Concat, _, _)));
        } else {
            panic!("expected Concat at top level");
        }
    }

    #[test]
    fn parses_unary_negation() {
        let prog = parse("-42;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::UnOp(UnOp::Neg, _)));
    }

    #[test]
    fn parses_let_expression() {
        let prog = parse("let x = 42 in x;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::Let(..)));
    }

    #[test]
    fn parses_multi_let_desugars_to_nested() {
        let prog = parse("let a = 1, b = 2 in a;").unwrap();
        // Should be Let(a, _, 1, Let(b, _, 2, Ident(a)))
        if let ExprKind::Let(name_a, _, _, inner) = &prog.entry.kind {
            assert_eq!(name_a, "a");
            assert!(matches!(&inner.kind, ExprKind::Let(name_b, _, _, _) if name_b == "b"));
        } else {
            panic!("expected outer Let");
        }
    }

    #[test]
    fn parses_let_with_type_annotation() {
        let prog = parse("let x: Number = 42 in x;").unwrap();
        if let ExprKind::Let(_, ty, _, _) = &prog.entry.kind {
            assert_eq!(ty.as_deref(), Some("Number"));
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parses_if_else() {
        let prog = parse("if (true) 1 else 2;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::If(..)));
    }

    #[test]
    fn parses_if_elif_else() {
        let prog = parse("if (true) 1 elif (false) 2 else 3;").unwrap();
        if let ExprKind::If(_, _, elifs, _) = &prog.entry.kind {
            assert_eq!(elifs.len(), 1);
        } else {
            panic!("expected If");
        }
    }

    #[test]
    fn parses_while_loop() {
        let prog = parse("while (true) 42;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::While(..)));
    }

    #[test]
    fn parses_for_loop() {
        let prog = parse("for (x in range(0, 10)) x;").unwrap();
        if let ExprKind::For(name, _, _) = &prog.entry.kind {
            assert_eq!(name, "x");
        } else {
            panic!("expected For");
        }
    }

    #[test]
    fn parses_function_call() {
        let prog = parse("print(42);").unwrap();
        if let ExprKind::Call(name, args) = &prog.entry.kind {
            assert_eq!(name, "print");
            assert_eq!(args.len(), 1);
        } else {
            panic!("expected Call");
        }
    }

    #[test]
    fn parses_block_expression() {
        let prog = parse("{ 1; 2; 3 }").unwrap();
        if let ExprKind::Block(stmts) = &prog.entry.kind {
            assert_eq!(stmts.len(), 3);
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn parses_new_expression() {
        let prog = parse("new Point(1, 2);").unwrap();
        if let ExprKind::New(name, args) = &prog.entry.kind {
            assert_eq!(name, "Point");
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected New");
        }
    }

    #[test]
    fn parses_self_expression() {
        // self is valid as an atom even outside a type (semantic will catch it)
        let prog = parse("self;").unwrap();
        assert!(matches!(prog.entry.kind, ExprKind::SelfExpr));
    }

    #[test]
    fn parses_base_call() {
        let prog = parse("base(1, 2);").unwrap();
        if let ExprKind::Base(args) = &prog.entry.kind {
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected Base");
        }
    }

    #[test]
    fn parses_method_call() {
        let prog = parse("x.foo(1);").unwrap();
        if let ExprKind::MethodCall(_, name, args) = &prog.entry.kind {
            assert_eq!(name, "foo");
            assert_eq!(args.len(), 1);
        } else {
            panic!("expected MethodCall");
        }
    }

    #[test]
    fn parses_field_access() {
        let prog = parse("self.x;").unwrap();
        assert!(matches!(&prog.entry.kind, ExprKind::GetField(_, name) if name == "x"));
    }

    #[test]
    fn parses_destructive_assignment() {
        let prog = parse("let a = 0 in { a := 1; a }").unwrap();
        if let ExprKind::Let(_, _, _, body) = &prog.entry.kind {
            if let ExprKind::Block(stmts) = &body.kind {
                assert!(matches!(&stmts[0].kind, ExprKind::Assign(name, _) if name == "a"));
            } else {
                panic!("expected Block");
            }
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parses_is_expression() {
        let prog = parse("x is Number;").unwrap();
        assert!(matches!(&prog.entry.kind, ExprKind::Is(_, ty) if ty == "Number"));
    }

    #[test]
    fn parses_as_expression() {
        let prog = parse("x as Number;").unwrap();
        assert!(matches!(&prog.entry.kind, ExprKind::As(_, ty) if ty == "Number"));
    }

    #[test]
    fn parses_inline_function_declaration() {
        let prog = parse("function tan(x) => sin(x) / cos(x); 42;").unwrap();
        assert_eq!(prog.functions.len(), 1);
        assert_eq!(prog.functions[0].name, "tan");
        assert_eq!(prog.functions[0].params.len(), 1);
    }

    #[test]
    fn parses_block_function_declaration() {
        let source = r#"function operate(x, y) {
            print(x + y);
            print(x * y);
        }
        42;"#;
        let prog = parse(source).unwrap();
        assert_eq!(prog.functions.len(), 1);
        assert_eq!(prog.functions[0].name, "operate");
    }

    #[test]
    fn parses_type_declaration() {
        let source = r#"type Point(x, y) {
            x = x;
            y = y;
            getX() => self.x;
        }
        42;"#;
        let prog = parse(source).unwrap();
        assert_eq!(prog.types.len(), 1);
        assert_eq!(prog.types[0].name, "Point");
        assert_eq!(prog.types[0].type_params.len(), 2);
        assert_eq!(prog.types[0].attributes.len(), 2);
        assert_eq!(prog.types[0].methods.len(), 1);
    }

    #[test]
    fn parses_type_with_inheritance() {
        let source = r#"type PolarPoint inherits Point {
            rho() => 0;
        }
        42;"#;
        let prog = parse(source).unwrap();
        assert!(prog.types[0].parent.is_some());
        let parent = prog.types[0].parent.as_ref().unwrap();
        assert_eq!(parent.name, "Point");
        assert!(parent.args.is_none()); // no explicit parent args
    }

    #[test]
    fn parses_complete_program() {
        let source = r#"
            function cot(x) => 1 / tan(x);
            function tan(x) => sin(x) / cos(x);

            print(tan(PI) ^ 2 + cot(PI) ^ 2);
        "#;
        let prog = parse(source).unwrap();
        assert_eq!(prog.functions.len(), 2);
        assert!(matches!(prog.entry.kind, ExprKind::Call(..)));
    }
}
