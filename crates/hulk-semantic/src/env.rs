//! Lexical environment: a stack of scopes, each a `HashMap<String, Binding>`.
//!
//! Sequential `let` semantics (HULK spec A.4.1) is preserved by the **parser**
//! (role A): `let a = 1, b = a + 1 in ...` is desugared to nested `Let` nodes,
//! so each binding lives in its own freshly-pushed scope and the chain falls
//! out naturally on the semantic side — no special multi-binding logic here.

use crate::ast::Span;
use crate::types::Type;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Binding {
    pub ty: Type,
    pub span: Span,
}

#[derive(Default)]
pub struct Env {
    scopes: Vec<HashMap<String, Binding>>,
}

impl Env {
    pub fn new() -> Self {
        Self { scopes: vec![HashMap::new()] }
    }

    pub fn enter(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn leave(&mut self) {
        self.scopes.pop().expect("popping root scope");
    }

    pub fn define(&mut self, name: &str, b: Binding) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), b);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }
}
