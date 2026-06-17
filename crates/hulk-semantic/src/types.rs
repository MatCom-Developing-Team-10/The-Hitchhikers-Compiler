//! Type representation and the conformance / LCA algorithms (HULK spec A.8.4).
//!
//! Generics extension (type erasure):
//! - `Type::Generic(name, args)` is an instantiated generic type like `List[Number]`.
//! - `Type::Param(name)` is an unresolved type parameter `T` inside a generic
//!   declaration's body. It conforms to itself and to `Object` only.
//! - Generic conformance is **invariant** in its arguments — `List[Animal]` does
//!   NOT conform to `List[Object]`. This avoids variance surprises in a
//!   teaching language and matches Java's behavior for raw generics.
//! - At runtime, generics are erased: `List[Number]` and `List[String]` both
//!   reduce to a `List` object with the same vtable.

use crate::ast::{Expr, TypeRef};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Number,
    String,
    Boolean,
    Object,
    /// A non-generic user-defined type (e.g., `Point`).
    User(String),
    /// An instantiated generic type (e.g., `List[Number]`, `Map[String, Point]`).
    Generic(String, Vec<Type>),
    /// An unresolved type parameter inside a generic declaration's scope.
    Param(String),
    /// A typed iterable `T*` (A.11.2): any value implementing the iterable
    /// protocol whose `current()` yields the boxed element type.
    Iterable(Box<Type>),
    /// A vector `T[]` (A.12): a concrete iterable of `T` that also supports
    /// indexing and `size()`.
    Vector(Box<Type>),
    /// Poison value — propagated after a reported error to silence cascades.
    Error,
}

impl Type {
    pub fn name(&self) -> String {
        match self {
            Type::Number => "Number".into(),
            Type::String => "String".into(),
            Type::Boolean => "Boolean".into(),
            Type::Object => "Object".into(),
            Type::User(n) | Type::Param(n) => n.clone(),
            Type::Generic(n, args) => {
                let mut s = format!("{n}[");
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&a.name());
                }
                s.push(']');
                s
            }
            Type::Iterable(elem) => format!("{}*", elem.name()),
            Type::Vector(elem) => format!("{}[]", elem.name()),
            Type::Error => "<error>".into(),
        }
    }

    /// Return the base (erased) name for runtime lookup: `List[Number]` → `List`.
    pub fn base_name(&self) -> Option<&str> {
        match self {
            Type::User(n) | Type::Generic(n, _) | Type::Param(n) => Some(n),
            _ => None,
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
    /// Generic type parameters of this function (empty for non-generic functions).
    pub generic_params: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub parent: String,
    /// Generic type parameters declared by this type (empty for non-generic types).
    pub generic_params: Vec<String>,
    pub ctor_params: Vec<(String, Type)>,
    /// `None` means "forward our own ctor params to the parent" (A.7.3 default).
    pub parent_args: Option<Vec<Expr>>,
    /// Interfaces this type implements (extension). Resolved to Type values
    /// (typically `Type::User(name)` or `Type::Generic(...)`).
    pub implements: Vec<Type>,
    pub attrs: HashMap<String, Type>,
    pub methods: HashMap<String, MethodSig>,
}

/// Information about a declared interface (extension).
#[derive(Clone, Debug)]
pub struct InterfaceInfo {
    /// Generic type parameters declared by this interface.
    pub generic_params: Vec<String>,
    /// Parent interfaces (multiple inheritance allowed via `extends`).
    pub extends: Vec<Type>,
    /// Method signatures (no body).
    pub methods: HashMap<String, MethodSig>,
}

#[derive(Default, Debug)]
pub struct TypeCtx {
    pub types: HashMap<String, TypeInfo>,
    /// Interface registry (extension). Disjoint from `types`: a name cannot
    /// be both a type and an interface.
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub funcs: HashMap<String, FunctionSig>,
    pub builtin_consts: HashMap<String, Type>,
}

impl TypeCtx {
    pub fn new() -> Self {
        let mut funcs = HashMap::new();
        // A.2.3 builtin math functions.
        funcs.insert(
            "print".into(),
            FunctionSig {
                params: vec![Type::Object],
                returns: Type::Object,
                generic_params: vec![],
            },
        );
        funcs.insert(
            "sqrt".into(),
            FunctionSig {
                params: vec![Type::Number],
                returns: Type::Number,
                generic_params: vec![],
            },
        );
        funcs.insert(
            "sin".into(),
            FunctionSig {
                params: vec![Type::Number],
                returns: Type::Number,
                generic_params: vec![],
            },
        );
        funcs.insert(
            "cos".into(),
            FunctionSig {
                params: vec![Type::Number],
                returns: Type::Number,
                generic_params: vec![],
            },
        );
        funcs.insert(
            "exp".into(),
            FunctionSig {
                params: vec![Type::Number],
                returns: Type::Number,
                generic_params: vec![],
            },
        );
        funcs.insert(
            "log".into(),
            FunctionSig {
                params: vec![Type::Number, Type::Number],
                returns: Type::Number,
                generic_params: vec![],
            },
        );
        funcs.insert(
            "rand".into(),
            FunctionSig {
                params: vec![],
                returns: Type::Number,
                generic_params: vec![],
            },
        );
        // `range(a, b)` is leniently typed: it produces "something iterable of Number".
        // We do not model Iterable as a real protocol yet, so we return Object and let
        // `for (x in range(...))` bind `x : Number` by construction (see Checker).
        funcs.insert(
            "range".into(),
            FunctionSig {
                params: vec![Type::Number, Type::Number],
                returns: Type::Object,
                generic_params: vec![],
            },
        );

        let mut builtin_consts = HashMap::new();
        builtin_consts.insert("PI".into(), Type::Number);
        builtin_consts.insert("E".into(), Type::Number);

        Self {
            types: HashMap::new(),
            interfaces: HashMap::new(),
            funcs,
            builtin_consts,
        }
    }

    /// Resolve a syntactic type name (`"Number"`, `"Point"`, `"Comparable"`, etc.)
    /// to a `Type`. Treats unknown names as user types only when they are
    /// registered as either a type or an interface.
    pub fn resolve_name(&self, name: &str) -> Option<Type> {
        match name {
            "Number" => Some(Type::Number),
            "String" => Some(Type::String),
            "Boolean" => Some(Type::Boolean),
            "Object" => Some(Type::Object),
            other if self.types.contains_key(other) => Some(Type::User(other.into())),
            other if self.interfaces.contains_key(other) => Some(Type::User(other.into())),
            _ => None,
        }
    }

    /// Returns true if `name` is a declared interface.
    pub fn is_interface(&self, name: &str) -> bool {
        self.interfaces.contains_key(name)
    }

    /// Resolve a `TypeRef` (which may carry generic arguments) to a `Type`.
    ///
    /// - `Simple("Number")` → `Type::Number`
    /// - `Simple("List")` (where `List` has 0 generic params) → `Type::User("List")`
    /// - `Generic("List", [Number])` → `Type::Generic("List", [Number])`
    ///
    /// Returns `None` if the base name is unknown or the arity does not match
    /// (caller reports a more specific diagnostic).
    pub fn resolve_type_ref(&self, tref: &TypeRef) -> Option<Type> {
        match tref {
            TypeRef::Simple(name) => self.resolve_name(name),
            TypeRef::Iterable(inner) => {
                Some(Type::Iterable(Box::new(self.resolve_type_ref(inner)?)))
            }
            TypeRef::Vector(inner) => Some(Type::Vector(Box::new(self.resolve_type_ref(inner)?))),
            TypeRef::Generic(name, args) => {
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_ref(a).unwrap_or(Type::Error))
                    .collect();
                if let Some(info) = self.types.get(name) {
                    if info.generic_params.len() == args.len() {
                        return Some(Type::Generic(name.clone(), resolved_args));
                    }
                }
                None
            }
        }
    }

    /// Resolve a `TypeRef` in a scope that includes generic type parameters
    /// (e.g., inside `function map[T, U](...)`, `T` resolves to `Type::Param("T")`).
    pub fn resolve_type_ref_in_scope(&self, tref: &TypeRef, scope: &[String]) -> Option<Type> {
        match tref {
            TypeRef::Simple(name) => {
                if scope.iter().any(|p| p == name) {
                    Some(Type::Param(name.clone()))
                } else {
                    self.resolve_name(name)
                }
            }
            TypeRef::Iterable(inner) => Some(Type::Iterable(Box::new(
                self.resolve_type_ref_in_scope(inner, scope)?,
            ))),
            TypeRef::Vector(inner) => Some(Type::Vector(Box::new(
                self.resolve_type_ref_in_scope(inner, scope)?,
            ))),
            TypeRef::Generic(name, args) => {
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| {
                        self.resolve_type_ref_in_scope(a, scope)
                            .unwrap_or(Type::Error)
                    })
                    .collect();
                if let Some(info) = self.types.get(name) {
                    if info.generic_params.len() == args.len() {
                        return Some(Type::Generic(name.clone(), resolved_args));
                    }
                }
                None
            }
        }
    }

    /// Conformance (A.8.4): is `a <= b`?
    ///
    /// Rules from the spec, in order of application:
    /// 1. `Type::Error` is transparent (silences cascading errors).
    /// 2. Reflexive: every type conforms to itself.
    /// 3. `Object` is the top of the hierarchy.
    /// 4. `Number`, `String`, `Boolean` conform only to themselves and to `Object`.
    /// 5. Generic types conform invariantly: `T[A]` ≤ `T[B]` iff `A == B`.
    /// 6. Type parameters (`Param`) conform only to themselves and `Object`.
    /// 7. **Interfaces (extension):** `a` conforms to interface `b` if either
    ///    (a) `a` or one of its ancestors declares `implements b` (or implements
    ///    an interface that transitively `extends b`) — nominal conformance; or
    ///    (b) `a` structurally provides every method `b` requires (incl. via
    ///    `extends`) with a matching signature — protocol-style conformance
    ///    (A.10.2), no `implements` clause needed.
    /// 8. Otherwise: walk the parent chain of `a`; if it reaches `b`, conform.
    pub fn conforms(&self, a: &Type, b: &Type) -> bool {
        if matches!(a, Type::Error) || matches!(b, Type::Error) {
            return true;
        }
        if a == b {
            return true;
        }
        if matches!(b, Type::Object) {
            return true;
        }
        // Typed iterable `T*` (A.11.2). `a` conforms to `Iterable(elem)` if it
        // is itself an iterable of a conforming element type, or a concrete type
        // that implements the iterable protocol with a conforming `current()`.
        if let Type::Iterable(elem) = b {
            return match a {
                Type::Iterable(ea) | Type::Vector(ea) => self.conforms(ea, elem),
                Type::User(_) | Type::Generic(_, _) => self
                    .iterable_element_of(a)
                    .is_some_and(|e| self.conforms(&e, elem)),
                _ => false,
            };
        }
        // A vector `T[]` conforms to another vector only invariantly (handled by
        // the `a == b` check above); to anything else it conforms only via
        // `Object` (handled above) or `T*` (handled above).
        if matches!(a, Type::Vector(_)) {
            return false;
        }
        // An iterable on the left conforms to nothing else (beyond `Object`,
        // handled above, and another iterable, handled above).
        if matches!(a, Type::Iterable(_)) {
            return false;
        }
        if matches!(
            a,
            Type::Number | Type::String | Type::Boolean | Type::Object | Type::Param(_)
        ) {
            return false;
        }
        let cur_base = match a {
            Type::User(n) | Type::Generic(n, _) => n.clone(),
            _ => return false,
        };
        let target_base = match b {
            Type::User(n) | Type::Generic(n, _) => n.clone(),
            _ => return false,
        };
        // Interface subtyping (extension): if `b` names an interface, accept
        // either an explicit `implements` (nominal) or structural conformance —
        // the type provides every required method with a matching signature,
        // even without an `implements` clause (protocol-style, A.10.2).
        if self.is_interface(&target_base) {
            if self.implements_interface(&cur_base, &target_base) {
                return true;
            }
            return self.structurally_implements(&cur_base, b);
        }
        // Generic invariance: only equal heads + equal args conform (handled by `a == b` above).
        let mut cur = cur_base;
        while let Some(info) = self.types.get(&cur) {
            if info.parent == target_base {
                if let Type::Generic(_, _) = b {
                    return false;
                }
                return true;
            }
            if info.parent == "Object" {
                return false;
            }
            cur = info.parent.clone();
        }
        false
    }

    /// Returns true if type `tname` (or any of its ancestors) declares
    /// `implements iface`, transitively through interface `extends`.
    fn implements_interface(&self, tname: &str, iface: &str) -> bool {
        let mut cur = tname.to_string();
        while let Some(info) = self.types.get(&cur) {
            for t in &info.implements {
                if let Some(base) = t.base_name() {
                    if base == iface || self.interface_extends(base, iface) {
                        return true;
                    }
                }
            }
            if info.parent == "Object" {
                return false;
            }
            cur = info.parent.clone();
        }
        false
    }

    /// Structural (protocol-style) conformance: a type conforms to an interface
    /// if it provides every method the interface requires (including those
    /// inherited through `extends`), with matching signatures — even when the
    /// type carries no explicit `implements` clause (HULK protocols, A.10.2).
    fn structurally_implements(&self, tname: &str, iface_ty: &Type) -> bool {
        let (iface_name, iface_args) = match iface_ty {
            Type::User(n) => (n.clone(), Vec::new()),
            Type::Generic(n, args) => (n.clone(), args.clone()),
            _ => return false,
        };
        let Some(iface_info) = self.interfaces.get(&iface_name) else {
            return false;
        };
        // Substitute the interface's generic parameters with the args at the use site.
        let subst: HashMap<String, Type> = iface_info
            .generic_params
            .iter()
            .cloned()
            .zip(iface_args.iter().cloned())
            .collect();

        let mut required: HashMap<String, MethodSig> = HashMap::new();
        self.collect_interface_methods(&iface_name, &mut required);

        for (mname, req_sig) in &required {
            let req_params: Vec<Type> = req_sig
                .params
                .iter()
                .map(|p| self.substitute(p, &subst))
                .collect();
            let req_ret = self.substitute(&req_sig.returns, &subst);
            match self.lookup_type_method(tname, mname) {
                Some(own) if own.params == req_params && own.returns == req_ret => {}
                _ => return false,
            }
        }
        true
    }

    /// Collect every method an interface requires, walking the `extends` chain.
    fn collect_interface_methods(&self, iface: &str, out: &mut HashMap<String, MethodSig>) {
        let Some(info) = self.interfaces.get(iface) else {
            return;
        };
        for ext in &info.extends {
            if let Some(base) = ext.base_name() {
                self.collect_interface_methods(base, out);
            }
        }
        for (name, sig) in &info.methods {
            out.insert(name.clone(), sig.clone());
        }
    }

    /// Look up a method on a type, walking the inheritance chain to `Object`.
    fn lookup_type_method(&self, tname: &str, name: &str) -> Option<MethodSig> {
        let mut cur = tname.to_string();
        while let Some(info) = self.types.get(&cur) {
            if let Some(sig) = info.methods.get(name) {
                return Some(sig.clone());
            }
            if info.parent == "Object" {
                return None;
            }
            cur = info.parent.clone();
        }
        None
    }

    /// Element type produced by iterating a concrete type that implements the
    /// iterable protocol (A.11): it must provide both `next()` and `current()`,
    /// and the element type is `current()`'s (substituted) return type. Returns
    /// `None` when the type does not implement the full protocol.
    pub fn iterable_element_of(&self, ty: &Type) -> Option<Type> {
        let (name, subst) = match ty {
            Type::User(n) => (n.clone(), HashMap::new()),
            Type::Generic(n, targs) => {
                let gp = self.types.get(n).map(|i| &i.generic_params);
                let subst: HashMap<String, Type> = gp
                    .map(|gp| gp.iter().cloned().zip(targs.iter().cloned()).collect())
                    .unwrap_or_default();
                (n.clone(), subst)
            }
            _ => return None,
        };
        self.lookup_type_method(&name, "next")?;
        let current = self.lookup_type_method(&name, "current")?;
        Some(self.substitute(&current.returns, &subst))
    }

    /// Returns true if interface `from` extends `to` transitively.
    fn interface_extends(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        let Some(info) = self.interfaces.get(from) else {
            return false;
        };
        for ext in &info.extends {
            if let Some(base) = ext.base_name() {
                if self.interface_extends(base, to) {
                    return true;
                }
            }
        }
        false
    }

    /// Lowest common ancestor in the type hierarchy. Used to type
    /// `if`/`elif`/`else` chains (A.5.2) where every branch may produce a
    /// different concrete type.
    pub fn lca(&self, a: &Type, b: &Type) -> Type {
        if a == b {
            return a.clone();
        }
        if matches!(a, Type::Error) {
            return b.clone();
        }
        if matches!(b, Type::Error) {
            return a.clone();
        }

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
            Type::Object | Type::Error | Type::Param(_) => None,
            // A typed iterable / vector is a structural protocol rooted at `Object`.
            Type::Iterable(_) | Type::Vector(_) => Some(Type::Object),
            Type::User(name) | Type::Generic(name, _) => {
                let info = self.types.get(name)?;
                Some(match info.parent.as_str() {
                    "Object" => Type::Object,
                    other => Type::User(other.to_string()),
                })
            }
        }
    }

    /// Substitute type parameters in `ty` according to `subst` (param name → type).
    ///
    /// Used at generic method/field lookup time: when `self : List[Number]`
    /// and `head` is declared `T`, the lookup substitutes `T → Number`.
    pub fn substitute(&self, ty: &Type, subst: &HashMap<String, Type>) -> Type {
        match ty {
            Type::Param(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
            Type::Generic(name, args) => Type::Generic(
                name.clone(),
                args.iter().map(|a| self.substitute(a, subst)).collect(),
            ),
            Type::Iterable(elem) => Type::Iterable(Box::new(self.substitute(elem, subst))),
            Type::Vector(elem) => Type::Vector(Box::new(self.substitute(elem, subst))),
            _ => ty.clone(),
        }
    }
}
