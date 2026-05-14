//! Type representation and the conformance / LCA algorithms (HULK spec A.8.4).

use crate::ast::Expr;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Number,
    String,
    Boolean,
    Object,
    User(String),
    /// Poison value — propagated after a reported error to silence cascades.
    Error,
}

impl Type {
    pub fn name(&self) -> &str {
        match self {
            Type::Number    => "Number",
            Type::String    => "String",
            Type::Boolean   => "Boolean",
            Type::Object    => "Object",
            Type::User(n)   => n,
            Type::Error     => "<error>",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MethodSig {
    pub params: Vec<Type>,
    pub returns: Type,
    /// Name of the type that *declares* (not just inherits) this method.
    pub owner: String,
}

#[derive(Clone, Debug)]
pub struct FunctionSig {
    pub params: Vec<Type>,
    pub returns: Type,
}

#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub parent: String,
    pub ctor_params: Vec<(String, Type)>,
    /// `None` means "forward our own ctor params to the parent" (A.7.3 default).
    pub parent_args: Option<Vec<Expr>>,
    pub attrs: HashMap<String, Type>,
    pub methods: HashMap<String, MethodSig>,
}

#[derive(Default)]
pub struct TypeCtx {
    pub types: HashMap<String, TypeInfo>,
    pub funcs: HashMap<String, FunctionSig>,
    pub builtin_consts: HashMap<String, Type>,
}

impl TypeCtx {
    pub fn new() -> Self {
        let mut funcs = HashMap::new();
        // A.2.3 builtin math functions.
        funcs.insert("print".into(),
            FunctionSig { params: vec![Type::Object], returns: Type::Object });
        funcs.insert("sqrt".into(),
            FunctionSig { params: vec![Type::Number], returns: Type::Number });
        funcs.insert("sin".into(),
            FunctionSig { params: vec![Type::Number], returns: Type::Number });
        funcs.insert("cos".into(),
            FunctionSig { params: vec![Type::Number], returns: Type::Number });
        funcs.insert("exp".into(),
            FunctionSig { params: vec![Type::Number], returns: Type::Number });
        funcs.insert("log".into(),
            FunctionSig { params: vec![Type::Number, Type::Number], returns: Type::Number });
        funcs.insert("rand".into(),
            FunctionSig { params: vec![], returns: Type::Number });
        // `range(a, b)` is leniently typed: it produces "something iterable of Number".
        // We do not model Iterable as a real protocol yet, so we return Object and let
        // `for (x in range(...))` bind `x : Number` by construction (see Checker).
        funcs.insert("range".into(),
            FunctionSig { params: vec![Type::Number, Type::Number], returns: Type::Object });

        let mut builtin_consts = HashMap::new();
        builtin_consts.insert("PI".into(), Type::Number);
        builtin_consts.insert("E".into(),  Type::Number);

        Self { types: HashMap::new(), funcs, builtin_consts }
    }

    /// Resolve a syntactic type name (`"Number"`, `"Point"`, etc.) to a `Type`.
    pub fn resolve_name(&self, name: &str) -> Option<Type> {
        match name {
            "Number"  => Some(Type::Number),
            "String"  => Some(Type::String),
            "Boolean" => Some(Type::Boolean),
            "Object"  => Some(Type::Object),
            other if self.types.contains_key(other) => Some(Type::User(other.into())),
            _ => None,
        }
    }

    /// Conformance (A.8.4): is `a <= b`?
    ///
    /// Rules from the spec, in order of application:
    /// 1. `Type::Error` is transparent (silences cascading errors).
    /// 2. Reflexive: every type conforms to itself.
    /// 3. `Object` is the top of the hierarchy.
    /// 4. `Number`, `String`, `Boolean` conform only to themselves and to `Object`.
    /// 5. Otherwise: walk the parent chain of `a`; if it reaches `b`, conform.
    pub fn conforms(&self, a: &Type, b: &Type) -> bool {
        if matches!(a, Type::Error) || matches!(b, Type::Error) {
            return true;
        }
        if a == b { return true; }
        if matches!(b, Type::Object) { return true; }
        if matches!(a, Type::Number | Type::String | Type::Boolean | Type::Object) {
            return false;
        }
        let mut cur = match a {
            Type::User(n) => n.clone(),
            _ => return false,
        };
        let target = match b {
            Type::User(n) => n.clone(),
            _ => return false,
        };
        while let Some(info) = self.types.get(&cur) {
            if info.parent == target { return true; }
            if info.parent == "Object" { return false; }
            cur = info.parent.clone();
        }
        false
    }

    /// Lowest common ancestor in the type hierarchy. Used to type
    /// `if`/`elif`/`else` chains (A.5.2) where every branch may produce a
    /// different concrete type.
    pub fn lca(&self, a: &Type, b: &Type) -> Type {
        if a == b { return a.clone(); }
        if matches!(a, Type::Error) { return b.clone(); }
        if matches!(b, Type::Error) { return a.clone(); }

        let mut ancestors_a = Vec::new();
        let mut cur = a.clone();
        loop {
            ancestors_a.push(cur.clone());
            match self.parent_of(&cur) {
                Some(p) => cur = p,
                None => break,
            }
        }
        let mut cur = b.clone();
        loop {
            if ancestors_a.contains(&cur) {
                return cur;
            }
            match self.parent_of(&cur) {
                Some(p) => cur = p,
                None => return Type::Object,
            }
        }
    }

    fn parent_of(&self, t: &Type) -> Option<Type> {
        match t {
            Type::Number | Type::String | Type::Boolean => Some(Type::Object),
            Type::Object | Type::Error => None,
            Type::User(name) => {
                let info = self.types.get(name)?;
                Some(match info.parent.as_str() {
                    "Object" => Type::Object,
                    other => Type::User(other.to_string()),
                })
            }
        }
    }
}
