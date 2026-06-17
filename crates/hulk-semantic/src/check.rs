//! The HULK type checker. One brazo per `ExprKind` variant, dispatched by a
//! single recursive `check_expr`. State on [`Checker`]:
//!
//! - `ctx`: the global type and function registry (built by `collect`/`sign`).
//! - `errors`: accumulated diagnostics; the analysis never aborts on first
//!   error. Failed expressions get `Type::Error` so cascading messages don't
//!   bury the original cause.
//! - `current_method`: tracks the type/method currently being checked so that
//!   `base(...)` can find the parent implementation.

use crate::ast::*;
use crate::env::{Binding, Env};
use crate::error::SemError;
use crate::types::{FunctionSig, MethodSig, Type, TypeCtx, TypeInfo};
use std::collections::{HashMap, HashSet};

/// Accumulated inference state for a single un-annotated parameter while
/// walking a function/method body (A.9.3). Constraints come from usages that
/// require a concrete type (arithmetic ⇒ `Number`, a condition ⇒ `Boolean`,
/// a method call ⇒ the unique declaring type, etc.). Usages that accept any
/// type (e.g. `==`, `print`, `@`) contribute no constraint.
#[derive(Clone)]
enum ParamConstraint {
    /// No constraining usage seen yet.
    Unknown,
    /// A single most-specific type consistent with every usage so far.
    Known(Type),
    /// Two usages demanded types in different branches of the hierarchy; the
    /// inferer is allowed to fail here (A.9.3), so we leave the parameter as
    /// `Object` and let the type checker report the resulting mismatch.
    Conflict,
}

impl ParamConstraint {
    /// Fold a new required type into the constraint, keeping the most specific
    /// type when one conforms to the other and flagging a `Conflict` otherwise.
    /// Non-informative requirements (`Object`/`Error`) are ignored.
    fn add(&mut self, ctx: &TypeCtx, ty: Type) {
        if matches!(ty, Type::Object | Type::Error) {
            return;
        }
        match self {
            ParamConstraint::Unknown => *self = ParamConstraint::Known(ty),
            ParamConstraint::Known(existing) => {
                if *existing == ty {
                    // already agree
                } else if ctx.conforms(&ty, existing) {
                    *self = ParamConstraint::Known(ty); // new one is more specific
                } else if !ctx.conforms(existing, &ty) {
                    *self = ParamConstraint::Conflict; // incompatible branches
                }
            }
            ParamConstraint::Conflict => {}
        }
    }
}

#[derive(Clone)]
struct MethodScope {
    owner: String,
    name: String,
}

pub struct Checker {
    pub ctx: TypeCtx,
    pub errors: Vec<SemError>,
    current_method: Option<MethodScope>,
    /// Generic type parameters in scope for the current declaration body
    /// (e.g., `T` inside `function map[T,U](...)` or inside methods of `List[T]`).
    current_generic_scope: Vec<String>,
    /// Side-channel for constructor-parameter inference (A.9.3). When `Some`,
    /// every `new T(args)` evaluated by `check_expr` folds its argument types
    /// (by position) into the running LCA for the type that *declares* `T`'s
    /// effective ctor params, so an un-annotated ctor param can be inferred
    /// from its call sites. Keyed by declaring-type name; each `Vec<Type>` has
    /// one running LCA slot per ctor param (`Type::Error` = neutral element).
    /// `None` during normal checking, so the hook is inert outside
    /// `infer_ctor_params`.
    ctor_arg_obs: Option<HashMap<String, Vec<Type>>>,
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker {
    pub fn new() -> Self {
        Self {
            ctx: TypeCtx::new(),
            errors: Vec::new(),
            current_method: None,
            current_generic_scope: Vec::new(),
            ctor_arg_obs: None,
        }
    }

    // -------------------------------------------------- pass 1: type/func names

    pub fn collect(&mut self, prog: &Program) {
        for td in &prog.types {
            if matches!(td.name.as_str(), "Number" | "String" | "Boolean" | "Object") {
                self.errors.push(SemError::ReservedTypeName {
                    name: td.name.clone(),
                    span: td.span,
                });
                continue;
            }
            if self.ctx.types.contains_key(&td.name) {
                self.errors.push(SemError::DuplicateType {
                    name: td.name.clone(),
                    span: td.span,
                });
                continue;
            }
            let parent = match &td.parent {
                Some(p) => {
                    if matches!(p.name.as_str(), "Number" | "String" | "Boolean") {
                        self.errors.push(SemError::InheritBuiltin {
                            name: p.name.clone(),
                            span: p.span,
                        });
                        "Object".to_string()
                    } else {
                        p.name.clone()
                    }
                }
                None => "Object".to_string(),
            };
            self.ctx.types.insert(
                td.name.clone(),
                TypeInfo {
                    parent,
                    generic_params: td.generic_params.clone(),
                    ctor_params: Vec::new(),
                    parent_args: td.parent.as_ref().and_then(|p| p.args.clone()),
                    implements: Vec::new(),
                    attrs: HashMap::new(),
                    methods: HashMap::new(),
                },
            );
        }

        // Register interface names (extension). Must not collide with types
        // or builtins. Methods are filled in during the `sign` pass.
        for iface in &prog.interfaces {
            if matches!(
                iface.name.as_str(),
                "Number" | "String" | "Boolean" | "Object"
            ) {
                self.errors.push(SemError::ReservedTypeName {
                    name: iface.name.clone(),
                    span: iface.span,
                });
                continue;
            }
            if self.ctx.types.contains_key(&iface.name)
                || self.ctx.interfaces.contains_key(&iface.name)
            {
                self.errors.push(SemError::DuplicateType {
                    name: iface.name.clone(),
                    span: iface.span,
                });
                continue;
            }
            self.ctx.interfaces.insert(
                iface.name.clone(),
                crate::types::InterfaceInfo {
                    generic_params: iface.generic_params.clone(),
                    extends: Vec::new(),
                    methods: HashMap::new(),
                },
            );
        }

        for td in &prog.types {
            if let Some(p) = &td.parent {
                if matches!(p.name.as_str(), "Object" | "Number" | "String" | "Boolean") {
                    continue;
                }
                if !self.ctx.types.contains_key(&p.name) {
                    self.errors.push(SemError::UndefinedType {
                        name: p.name.clone(),
                        span: p.span,
                    });
                    if let Some(info) = self.ctx.types.get_mut(&td.name) {
                        info.parent = "Object".into();
                    }
                }
            }
        }

        let type_names: Vec<String> = self.ctx.types.keys().cloned().collect();
        for tname in &type_names {
            if self.has_cycle(tname) {
                let span = prog
                    .types
                    .iter()
                    .find(|t| &t.name == tname)
                    .map(|t| t.span)
                    .unwrap_or_default();
                self.errors.push(SemError::CyclicInheritance {
                    name: tname.clone(),
                    span,
                });
                if let Some(info) = self.ctx.types.get_mut(tname) {
                    info.parent = "Object".into();
                }
            }
        }

        for f in &prog.functions {
            if matches!(f.name.as_str(), "self") {
                self.errors.push(SemError::ReservedName {
                    name: f.name.clone(),
                    span: f.span,
                });
                continue;
            }
            if self.ctx.funcs.contains_key(&f.name) {
                self.errors.push(SemError::DuplicateFunction {
                    name: f.name.clone(),
                    span: f.span,
                });
                continue;
            }
            self.ctx.funcs.insert(
                f.name.clone(),
                FunctionSig {
                    params: vec![Type::Object; f.params.len()],
                    returns: Type::Object,
                    generic_params: f.generic_params.clone(),
                },
            );
        }
    }

    fn has_cycle(&self, start: &str) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut cur = start.to_string();
        let limit = self.ctx.types.len() + 2;
        for _ in 0..limit {
            if !seen.insert(cur.clone()) {
                return true;
            }
            match self.ctx.types.get(&cur) {
                Some(info) if info.parent != "Object" => cur = info.parent.clone(),
                _ => return false,
            }
        }
        true
    }

    // ----------------------------------------------- pass 2: fill signatures

    pub fn sign(&mut self, prog: &Program) {
        for f in &prog.functions {
            for p in &f.params {
                if matches!(p.name.as_str(), "self") {
                    self.errors.push(SemError::ReservedName {
                        name: p.name.clone(),
                        span: p.span,
                    });
                }
            }
            let scope = f.generic_params.clone();
            let params = f
                .params
                .iter()
                .map(|p| self.resolve_or_default(p.ty.as_ref(), &scope, p.span))
                .collect();
            // No annotation: leave a sentinel to be filled by `infer_returns`.
            // `Type::Error` acts as a wildcard in `lca`/`conforms`, so a
            // self-recursive body converges to its non-recursive branch's type.
            let returns = match &f.return_ty {
                Some(tref) => self.resolve_or_default(Some(tref), &scope, f.span),
                None => Type::Error,
            };
            self.ctx.funcs.insert(
                f.name.clone(),
                FunctionSig {
                    params,
                    returns,
                    generic_params: f.generic_params.clone(),
                },
            );
        }
        for td in &prog.types {
            for p in &td.type_params {
                if matches!(p.name.as_str(), "self") {
                    self.errors.push(SemError::ReservedName {
                        name: p.name.clone(),
                        span: p.span,
                    });
                }
            }
            let scope = td.generic_params.clone();
            let ctor_params: Vec<(String, Type)> = td
                .type_params
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        self.resolve_or_default(p.ty.as_ref(), &scope, p.span),
                    )
                })
                .collect();
            let mut attrs = HashMap::new();
            for a in &td.attributes {
                let attr_type = if a.ty.is_some() {
                    self.resolve_or_default(a.ty.as_ref(), &scope, a.span)
                } else {
                    // No explicit annotation: infer from the initializer expression
                    // using ctor params as scope. Errors are suppressed here because
                    // check_bodies will report them with full context.
                    let saved_errors = self.errors.len();
                    let saved_scope =
                        std::mem::replace(&mut self.current_generic_scope, scope.clone());
                    let mut env = Env::new();
                    for (pname, pty) in &ctor_params {
                        env.define(
                            pname,
                            Binding {
                                ty: pty.clone(),
                                span: a.span,
                            },
                        );
                    }
                    let inferred = self.check_expr(&mut env, &a.init);
                    self.errors.truncate(saved_errors);
                    self.current_generic_scope = saved_scope;
                    if inferred == Type::Error {
                        Type::Object
                    } else {
                        inferred
                    }
                };
                attrs.insert(a.name.clone(), attr_type);
            }
            let mut methods = HashMap::new();
            for m in &td.methods {
                for p in &m.params {
                    if matches!(p.name.as_str(), "self") {
                        self.errors.push(SemError::ReservedName {
                            name: p.name.clone(),
                            span: p.span,
                        });
                    }
                }
                let params = m
                    .params
                    .iter()
                    .map(|p| self.resolve_or_default(p.ty.as_ref(), &scope, p.span))
                    .collect();
                // No annotation: sentinel filled later by `infer_returns`.
                let returns = match &m.return_ty {
                    Some(tref) => self.resolve_or_default(Some(tref), &scope, m.span),
                    None => Type::Error,
                };
                methods.insert(
                    m.name.clone(),
                    MethodSig {
                        params,
                        returns,
                        owner: td.name.clone(),
                    },
                );
            }
            // Resolve the `implements` clause to concrete Type values, reporting
            // unknown names and non-interface targets.
            let implements: Vec<Type> = td
                .implements
                .iter()
                .filter_map(
                    |tref| match self.ctx.resolve_type_ref_in_scope(tref, &scope) {
                        Some(t) => {
                            if let Some(base) = t.base_name() {
                                if !self.ctx.is_interface(base) {
                                    self.errors.push(SemError::NotAnInterface {
                                        name: base.to_string(),
                                        span: td.span,
                                    });
                                    return None;
                                }
                            }
                            Some(t)
                        }
                        None => {
                            self.errors.push(SemError::UndefinedType {
                                name: tref.to_string(),
                                span: td.span,
                            });
                            None
                        }
                    },
                )
                .collect();
            if let Some(info) = self.ctx.types.get_mut(&td.name) {
                info.ctor_params = ctor_params;
                info.attrs = attrs;
                info.methods = methods;
                info.implements = implements;
            }
        }

        // Fill in interface methods and `extends`.
        for iface in &prog.interfaces {
            let scope = iface.generic_params.clone();
            let extends: Vec<Type> = iface
                .extends
                .iter()
                .filter_map(
                    |tref| match self.ctx.resolve_type_ref_in_scope(tref, &scope) {
                        Some(t) => {
                            if let Some(base) = t.base_name() {
                                if !self.ctx.is_interface(base) {
                                    self.errors.push(SemError::NotAnInterface {
                                        name: base.to_string(),
                                        span: iface.span,
                                    });
                                    return None;
                                }
                            }
                            Some(t)
                        }
                        None => {
                            self.errors.push(SemError::UndefinedType {
                                name: tref.to_string(),
                                span: iface.span,
                            });
                            None
                        }
                    },
                )
                .collect();
            let mut methods = HashMap::new();
            for m in &iface.methods {
                let params = m
                    .params
                    .iter()
                    .map(|p| self.resolve_or_default(p.ty.as_ref(), &scope, p.span))
                    .collect();
                let returns = match &m.return_ty {
                    Some(tref) => self.resolve_or_default(Some(tref), &scope, m.span),
                    None => Type::Object,
                };
                methods.insert(
                    m.name.clone(),
                    MethodSig {
                        params,
                        returns,
                        owner: iface.name.clone(),
                    },
                );
            }
            if let Some(info) = self.ctx.interfaces.get_mut(&iface.name) {
                info.extends = extends;
                info.methods = methods;
            }
        }
    }

    fn resolve_or_default(&mut self, tref: Option<&TypeRef>, scope: &[String], span: Span) -> Type {
        match tref {
            Some(t) => match self.ctx.resolve_type_ref_in_scope(t, scope) {
                Some(ty) => ty,
                None => {
                    self.errors.push(SemError::UndefinedType {
                        name: t.to_string(),
                        span,
                    });
                    Type::Object
                }
            },
            None => Type::Object,
        }
    }

    // ------------------------------------------ pass 2.2: return inference
    //
    // Fill in the return type of every function and type method that lacks an
    // explicit annotation by walking its body. Runs to a fixpoint so that
    // mutually-dependent inferences settle: each un-annotated return starts as
    // the `Type::Error` sentinel (set in `sign`), which behaves as a wildcard in
    // `lca`/`conforms`. Errors raised while inferring are discarded — they are
    // re-reported with full context by `check_bodies`.
    pub fn infer_returns(&mut self, prog: &Program) {
        // Each iteration resolves at least one further dependency level, so an
        // acyclic dependency graph reaches its fixpoint within this bound.
        let inferable = prog
            .functions
            .iter()
            .filter(|f| f.return_ty.is_none())
            .count()
            + prog
                .types
                .iter()
                .flat_map(|t| &t.methods)
                .filter(|m| m.return_ty.is_none())
                .count();
        let max_iters = inferable + 1;

        for _ in 0..max_iters {
            let mut changed = false;

            for f in &prog.functions {
                if f.return_ty.is_some() {
                    continue;
                }
                let Some(sig) = self.ctx.funcs.get(&f.name).cloned() else {
                    continue;
                };
                let saved_errors = self.errors.len();
                let saved_scope =
                    std::mem::replace(&mut self.current_generic_scope, f.generic_params.clone());
                let mut env = Env::new();
                for (p, pt) in f.params.iter().zip(sig.params.iter()) {
                    env.define(
                        &p.name,
                        Binding {
                            ty: pt.clone(),
                            span: p.span,
                        },
                    );
                }
                let inferred = self.check_expr(&mut env, &f.body);
                self.errors.truncate(saved_errors);
                self.current_generic_scope = saved_scope;
                if let Some(sig) = self.ctx.funcs.get_mut(&f.name) {
                    if sig.returns != inferred {
                        sig.returns = inferred;
                        changed = true;
                    }
                }
            }

            for td in &prog.types {
                let self_ty = if td.generic_params.is_empty() {
                    Type::User(td.name.clone())
                } else {
                    Type::Generic(
                        td.name.clone(),
                        td.generic_params
                            .iter()
                            .map(|p| Type::Param(p.clone()))
                            .collect(),
                    )
                };
                for m in &td.methods {
                    if m.return_ty.is_some() {
                        continue;
                    }
                    let Some(sig) = self
                        .ctx
                        .types
                        .get(&td.name)
                        .and_then(|i| i.methods.get(&m.name))
                        .cloned()
                    else {
                        continue;
                    };
                    let saved_errors = self.errors.len();
                    let saved_scope = std::mem::replace(
                        &mut self.current_generic_scope,
                        td.generic_params.clone(),
                    );
                    let saved_method = self.current_method.take();
                    let mut env = Env::new();
                    env.define(
                        "self",
                        Binding {
                            ty: self_ty.clone(),
                            span: m.span,
                        },
                    );
                    for (p, pt) in m.params.iter().zip(sig.params.iter()) {
                        env.define(
                            &p.name,
                            Binding {
                                ty: pt.clone(),
                                span: p.span,
                            },
                        );
                    }
                    self.current_method = Some(MethodScope {
                        owner: td.name.clone(),
                        name: m.name.clone(),
                    });
                    let inferred = self.check_expr(&mut env, &m.body);
                    self.current_method = saved_method;
                    self.errors.truncate(saved_errors);
                    self.current_generic_scope = saved_scope;
                    if let Some(msig) = self
                        .ctx
                        .types
                        .get_mut(&td.name)
                        .and_then(|i| i.methods.get_mut(&m.name))
                    {
                        if msig.returns != inferred {
                            msig.returns = inferred;
                            changed = true;
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    // ----------------------------------- pass 2.4: interface implementation
    //
    // Verify that every type with an `implements` clause provides each method
    // required by the interface (including inherited interface methods via
    // `extends`), with matching parameter and return types after substituting
    // the interface's generic parameters.
    pub fn check_interfaces(&mut self, prog: &Program) {
        for td in &prog.types {
            let Some(info) = self.ctx.types.get(&td.name).cloned() else {
                continue;
            };
            for iface_ty in &info.implements {
                let (iface_name, iface_args) = match iface_ty {
                    Type::User(n) => (n.clone(), Vec::new()),
                    Type::Generic(n, args) => (n.clone(), args.clone()),
                    _ => continue,
                };
                let required = self.collect_required_methods(&iface_name);
                for (method_name, required_sig) in required {
                    // Substitute interface's generic params with the args used in `implements`.
                    let iface_info = match self.ctx.interfaces.get(&iface_name) {
                        Some(i) => i,
                        None => continue,
                    };
                    let subst: HashMap<String, Type> = iface_info
                        .generic_params
                        .iter()
                        .cloned()
                        .zip(iface_args.iter().cloned())
                        .collect();
                    let required_params: Vec<Type> = required_sig
                        .params
                        .iter()
                        .map(|p| self.ctx.substitute(p, &subst))
                        .collect();
                    let required_returns = self.ctx.substitute(&required_sig.returns, &subst);

                    let own = self.lookup_inherited_method(&td.name, &method_name);
                    match own {
                        None => {
                            self.errors.push(SemError::MissingInterfaceMethod {
                                ty: td.name.clone(),
                                iface: iface_name.clone(),
                                method: method_name.clone(),
                                span: td.span,
                            });
                        }
                        Some(own_sig) => {
                            if own_sig.params != required_params
                                || own_sig.returns != required_returns
                            {
                                self.errors.push(SemError::InterfaceSignatureMismatch {
                                    ty: td.name.clone(),
                                    iface: iface_name.clone(),
                                    method: method_name,
                                    span: td.span,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Collect every method that a type implementing `iface` must provide,
    /// including those inherited from interfaces in the `extends` chain.
    fn collect_required_methods(&self, iface: &str) -> HashMap<String, MethodSig> {
        let mut out = HashMap::new();
        self.collect_required_methods_into(iface, &mut out);
        out
    }

    fn collect_required_methods_into(&self, iface: &str, out: &mut HashMap<String, MethodSig>) {
        let Some(info) = self.ctx.interfaces.get(iface) else {
            return;
        };
        for ext in &info.extends {
            if let Some(base) = ext.base_name() {
                self.collect_required_methods_into(base, out);
            }
        }
        for (name, sig) in &info.methods {
            out.insert(name.clone(), sig.clone());
        }
    }

    // ------------------------------------------ pass 2.5: override signatures

    pub fn check_overrides(&mut self, prog: &Program) {
        for td in &prog.types {
            let parent = self
                .ctx
                .types
                .get(&td.name)
                .map(|i| i.parent.clone())
                .unwrap_or_else(|| "Object".into());
            for m in &td.methods {
                if let Some(parent_sig) = self.lookup_inherited_method(&parent, &m.name) {
                    let own_sig = self
                        .ctx
                        .types
                        .get(&td.name)
                        .and_then(|i| i.methods.get(&m.name))
                        .cloned();
                    if let Some(own_sig) = own_sig {
                        if own_sig.params != parent_sig.params
                            || own_sig.returns != parent_sig.returns
                        {
                            self.errors.push(SemError::OverrideSignatureMismatch {
                                name: m.name.clone(),
                                span: m.span,
                            });
                        }
                    }
                }
            }
        }
    }

    fn lookup_inherited_method(&self, ty: &str, name: &str) -> Option<MethodSig> {
        if ty == "Object" {
            return None;
        }
        let info = self.ctx.types.get(ty)?;
        if let Some(sig) = info.methods.get(name) {
            return Some(sig.clone());
        }
        self.lookup_inherited_method(&info.parent, name)
    }

    /// Look up a method declared by an interface (including its `extends` chain).
    fn lookup_interface_method(&self, iface: &str, name: &str) -> Option<MethodSig> {
        let info = self.ctx.interfaces.get(iface)?;
        if let Some(sig) = info.methods.get(name) {
            return Some(sig.clone());
        }
        for ext in &info.extends {
            if let Some(base) = ext.base_name() {
                if let Some(sig) = self.lookup_interface_method(base, name) {
                    return Some(sig);
                }
            }
        }
        None
    }

    fn lookup_inherited_attr(&self, ty: &str, name: &str) -> Option<Type> {
        if ty == "Object" {
            return None;
        }
        let info = self.ctx.types.get(ty)?;
        if let Some(t) = info.attrs.get(name) {
            return Some(t.clone());
        }
        self.lookup_inherited_attr(&info.parent, name)
    }

    /// Element type produced by iterating `iter_ty`: the (substituted) return
    /// type of its `current()` method. Returns `None` when the type is not a
    /// user type/interface or does not implement the full Iterable protocol
    /// (A.11) — i.e. it lacks `current()` *or* `next()`. Requiring both here
    /// means the type checker reports a non-iterable type instead of letting it
    /// type-check and then crash in the VM's `for` lowering (which calls both
    /// `next()` and `current()`). `range`'s runtime-only `Range` is not a user
    /// type, so it returns `None` and the caller falls back to `Number`.
    fn iterator_element_type(&self, iter_ty: &Type) -> Option<Type> {
        let (name, subst) = match iter_ty {
            Type::Iterable(elem) | Type::Vector(elem) => return Some((**elem).clone()),
            Type::User(n) => (n.clone(), HashMap::new()),
            Type::Generic(n, targs) => {
                let info_params = if self.ctx.is_interface(n) {
                    self.ctx.interfaces.get(n).map(|i| &i.generic_params)
                } else {
                    self.ctx.types.get(n).map(|i| &i.generic_params)
                };
                let subst: HashMap<String, Type> = info_params
                    .map(|gp| gp.iter().cloned().zip(targs.iter().cloned()).collect())
                    .unwrap_or_default();
                (n.clone(), subst)
            }
            _ => return None,
        };
        let lookup = |method: &str| {
            if self.ctx.is_interface(&name) {
                self.lookup_interface_method(&name, method)
            } else {
                self.lookup_inherited_method(&name, method)
            }
        };
        // The protocol requires `next()` to advance/terminate the iteration.
        lookup("next")?;
        let sig = lookup("current")?;
        Some(self.ctx.substitute(&sig.returns, &subst))
    }

    /// Resolve the effective constructor parameter types for `T`, following
    /// the forwarding rule from A.7.3 (no own `type_params` and no explicit
    /// `inherits Parent(...)` arg list ⇒ use the parent's effective ctor).
    fn effective_ctor_params(&self, tname: &str) -> Vec<Type> {
        let Some(info) = self.ctx.types.get(tname) else {
            return Vec::new();
        };
        if !info.ctor_params.is_empty() {
            return info.ctor_params.iter().map(|(_, t)| t.clone()).collect();
        }
        if info.parent_args.is_none() && info.parent != "Object" {
            return self.effective_ctor_params(&info.parent);
        }
        Vec::new()
    }

    /// Name of the type that actually *declares* the constructor parameters used
    /// when instantiating `tname` — i.e. `tname` itself, or, for a child that
    /// implicitly forwards (no own params and no explicit `inherits P(args)`),
    /// the nearest ancestor that declares them. Mirrors `effective_ctor_params`'
    /// forwarding rule so a `new Child(...)` can constrain the parent's params.
    fn effective_ctor_owner(&self, tname: &str) -> Option<String> {
        let info = self.ctx.types.get(tname)?;
        if !info.ctor_params.is_empty() {
            return Some(tname.to_string());
        }
        if info.parent_args.is_none() && info.parent != "Object" {
            return self.effective_ctor_owner(&info.parent);
        }
        None
    }

    // ---------------------------------------- pass 2.0: ctor-parameter inference
    //
    // Implements A.9.3 for *type* (constructor) parameters: an un-annotated ctor
    // param is inferred from the argument types passed at its `new T(...)` call
    // sites. The inferred type is the LCA (lowest common ancestor) of every
    // observed argument — the most specific type that can hold all of them. With
    // it, the book's canonical un-annotated `type Point(x, y) { x = x; ... }`
    // (instantiated as `new Point(3, 4)`) infers `x, y : Number`, so the derived
    // attributes and methods (`getX() => self.x`) are `Number` instead of
    // `Object`, and later arithmetic/`@` on them type-checks.
    //
    // Safe and monotonic, like `infer_params`: a ctor param is only narrowed when
    // it is currently the permissive `Object` default (un-annotated) and every
    // call site agrees on a more specific common type. Annotated params and any
    // type whose call sites disagree (LCA stays `Object`) are left untouched, so
    // a program that type-checked before still does.
    pub fn infer_ctor_params(&mut self, prog: &Program) {
        let total_unannotated: usize = prog
            .types
            .iter()
            .flat_map(|t| &t.type_params)
            .filter(|p| p.ty.is_none())
            .count();
        if total_unannotated == 0 {
            return;
        }
        let max_iters = total_unannotated + 2;

        for _ in 0..max_iters {
            // Phase 1: observe the argument types at every `new` site program-wide.
            self.ctor_arg_obs = Some(HashMap::new());
            self.observe_new_sites(prog);
            let obs = self.ctor_arg_obs.take().unwrap_or_default();

            // Phase 2: narrow un-annotated ctor params toward the observed LCA, and
            // re-derive the attributes of any type whose params changed.
            let mut changed = false;
            for td in &prog.types {
                let Some(joined) = obs.get(&td.name) else {
                    continue;
                };
                let old = self
                    .ctx
                    .types
                    .get(&td.name)
                    .map(|i| i.ctor_params.clone())
                    .unwrap_or_default();
                let mut new_params = old.clone();
                for (i, p) in td.type_params.iter().enumerate() {
                    if p.ty.is_some() || i >= new_params.len() {
                        continue; // annotated params are never inferred over
                    }
                    if !matches!(new_params[i].1, Type::Object) {
                        continue; // only ever narrow from the `Object` default
                    }
                    if let Some(j) = joined.get(i) {
                        if !matches!(j, Type::Error | Type::Object) {
                            new_params[i].1 = j.clone();
                        }
                    }
                }
                if new_params != old {
                    if let Some(info) = self.ctx.types.get_mut(&td.name) {
                        info.ctor_params = new_params;
                    }
                    let attrs = self.infer_attr_types(td);
                    if let Some(info) = self.ctx.types.get_mut(&td.name) {
                        info.attrs = attrs;
                    }
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }
        self.ctor_arg_obs = None;
    }

    /// Walk every function/method/attribute body and the program entry with the
    /// right environments so that `check_expr`'s `New` arm fires the ctor-arg
    /// observation hook. Errors raised during this dry run are discarded; the
    /// real diagnostics come from `check_bodies`.
    fn observe_new_sites(&mut self, prog: &Program) {
        let saved_errors = self.errors.len();

        for f in &prog.functions {
            let Some(sig) = self.ctx.funcs.get(&f.name).cloned() else {
                continue;
            };
            let saved_scope =
                std::mem::replace(&mut self.current_generic_scope, f.generic_params.clone());
            let mut env = Env::new();
            for (p, pt) in f.params.iter().zip(sig.params.iter()) {
                env.define(
                    &p.name,
                    Binding {
                        ty: pt.clone(),
                        span: p.span,
                    },
                );
            }
            let _ = self.check_expr(&mut env, &f.body);
            self.current_generic_scope = saved_scope;
        }

        for td in &prog.types {
            let self_ty = if td.generic_params.is_empty() {
                Type::User(td.name.clone())
            } else {
                Type::Generic(
                    td.name.clone(),
                    td.generic_params
                        .iter()
                        .map(|p| Type::Param(p.clone()))
                        .collect(),
                )
            };
            let scope = td.generic_params.clone();
            let ctor_params: Vec<(String, Type)> = self
                .ctx
                .types
                .get(&td.name)
                .map(|i| i.ctor_params.clone())
                .unwrap_or_default();

            // Attribute initializers: ctor params are in scope, `self` is not.
            for a in &td.attributes {
                let saved_scope = std::mem::replace(&mut self.current_generic_scope, scope.clone());
                let mut env = Env::new();
                for (pname, pty) in &ctor_params {
                    env.define(
                        pname,
                        Binding {
                            ty: pty.clone(),
                            span: a.span,
                        },
                    );
                }
                let _ = self.check_expr(&mut env, &a.init);
                self.current_generic_scope = saved_scope;
            }

            // Method bodies: `self` plus the method's params are in scope.
            for m in &td.methods {
                let Some(sig) = self
                    .ctx
                    .types
                    .get(&td.name)
                    .and_then(|i| i.methods.get(&m.name))
                    .cloned()
                else {
                    continue;
                };
                let saved_scope = std::mem::replace(&mut self.current_generic_scope, scope.clone());
                let saved_method = self.current_method.take();
                let mut env = Env::new();
                env.define(
                    "self",
                    Binding {
                        ty: self_ty.clone(),
                        span: m.span,
                    },
                );
                for (p, pt) in m.params.iter().zip(sig.params.iter()) {
                    env.define(
                        &p.name,
                        Binding {
                            ty: pt.clone(),
                            span: p.span,
                        },
                    );
                }
                self.current_method = Some(MethodScope {
                    owner: td.name.clone(),
                    name: m.name.clone(),
                });
                let _ = self.check_expr(&mut env, &m.body);
                self.current_method = saved_method;
                self.current_generic_scope = saved_scope;
            }
        }

        // Program entry expression.
        let mut env = Env::new();
        let _ = self.check_expr(&mut env, &prog.entry);

        self.errors.truncate(saved_errors);
    }

    /// (Re)infer the type of every attribute of `td` given the type's *current*
    /// ctor-param types in `self.ctx`. Annotated attributes resolve from their
    /// annotation; un-annotated ones are inferred from their initializer in a
    /// scope where the ctor params are bound. Errors are suppressed here and
    /// re-reported by `check_bodies`. Mirrors the attribute logic in `sign` so
    /// the two stay consistent.
    fn infer_attr_types(&mut self, td: &TypeDecl) -> HashMap<String, Type> {
        let scope = td.generic_params.clone();
        let ctor_params: Vec<(String, Type)> = self
            .ctx
            .types
            .get(&td.name)
            .map(|i| i.ctor_params.clone())
            .unwrap_or_default();
        let mut attrs = HashMap::new();
        for a in &td.attributes {
            let attr_type = if a.ty.is_some() {
                self.resolve_or_default(a.ty.as_ref(), &scope, a.span)
            } else {
                let saved_errors = self.errors.len();
                let saved_scope = std::mem::replace(&mut self.current_generic_scope, scope.clone());
                let mut env = Env::new();
                for (pname, pty) in &ctor_params {
                    env.define(
                        pname,
                        Binding {
                            ty: pty.clone(),
                            span: a.span,
                        },
                    );
                }
                let inferred = self.check_expr(&mut env, &a.init);
                self.errors.truncate(saved_errors);
                self.current_generic_scope = saved_scope;
                if inferred == Type::Error {
                    Type::Object
                } else {
                    inferred
                }
            };
            attrs.insert(a.name.clone(), attr_type);
        }
        attrs
    }

    // ------------------------------------------ pass 2.1: parameter inference
    //
    // Implements A.9.3: assign a type to every un-annotated function/method
    // parameter from the way it is used in the body. The inferred type is the
    // most specific type consistent with all constraining usages; if usages
    // disagree across hierarchy branches the parameter is left as `Object` and
    // the type checker reports the mismatch (A.9.3 allows the inferer to
    // fail). Runs to a fixpoint so that a parameter constrained through a call
    // to another function settles once that function's signature is known.
    //
    // This pass is monotonic and safe: it only narrows a parameter from the
    // permissive `Object` default toward a concrete type when a usage *requires*
    // that type. Usages that accept `Object` produce no constraint, so a program
    // that type-checked before still does.
    pub fn infer_params(&mut self, prog: &Program) {
        let total_unannotated: usize = prog
            .functions
            .iter()
            .flat_map(|f| &f.params)
            .filter(|p| p.ty.is_none())
            .count()
            + prog
                .types
                .iter()
                .flat_map(|t| &t.methods)
                .flat_map(|m| &m.params)
                .filter(|p| p.ty.is_none())
                .count();
        let max_iters = total_unannotated + 2;

        for _ in 0..max_iters {
            let mut changed = false;

            for f in &prog.functions {
                let unannotated: Vec<(usize, String)> = f
                    .params
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.ty.is_none())
                    .map(|(i, p)| (i, p.name.clone()))
                    .collect();
                if unannotated.is_empty() {
                    continue;
                }
                let active: HashSet<String> = unannotated.iter().map(|(_, n)| n.clone()).collect();
                let saved_scope =
                    std::mem::replace(&mut self.current_generic_scope, f.generic_params.clone());
                let mut acc: HashMap<String, ParamConstraint> = HashMap::new();
                self.collect_param_constraints(&f.body, &active, &mut acc);
                self.current_generic_scope = saved_scope;
                if let Some(sig) = self.ctx.funcs.get_mut(&f.name) {
                    for (i, name) in &unannotated {
                        if let Some(ParamConstraint::Known(t)) = acc.get(name) {
                            if sig.params[*i] != *t {
                                sig.params[*i] = t.clone();
                                changed = true;
                            }
                        }
                    }
                }
            }

            for td in &prog.types {
                for m in &td.methods {
                    let unannotated: Vec<(usize, String)> = m
                        .params
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.ty.is_none())
                        .map(|(i, p)| (i, p.name.clone()))
                        .collect();
                    if unannotated.is_empty() {
                        continue;
                    }
                    let active: HashSet<String> =
                        unannotated.iter().map(|(_, n)| n.clone()).collect();
                    let saved_scope = std::mem::replace(
                        &mut self.current_generic_scope,
                        td.generic_params.clone(),
                    );
                    let mut acc: HashMap<String, ParamConstraint> = HashMap::new();
                    self.collect_param_constraints(&m.body, &active, &mut acc);
                    self.current_generic_scope = saved_scope;
                    if let Some(msig) = self
                        .ctx
                        .types
                        .get_mut(&td.name)
                        .and_then(|i| i.methods.get_mut(&m.name))
                    {
                        for (i, name) in &unannotated {
                            if let Some(ParamConstraint::Known(t)) = acc.get(name) {
                                if msig.params[*i] != *t {
                                    msig.params[*i] = t.clone();
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    /// Walk an expression and record, for each active (un-annotated) parameter
    /// name, the types its usages require. `active` shrinks when a `let`/`for`
    /// binding shadows a parameter name.
    fn collect_param_constraints(
        &self,
        e: &Expr,
        active: &HashSet<String>,
        acc: &mut HashMap<String, ParamConstraint>,
    ) {
        match &e.kind {
            ExprKind::BinOp(op, l, r) => {
                let required = match op {
                    BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Pow
                    | BinOp::Mod
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge => Some(Type::Number),
                    BinOp::And | BinOp::Or => Some(Type::Boolean),
                    // `==`/`!=` accept any operands; `@`/`@@` accept String or
                    // Number — neither pins a single type, so no constraint.
                    BinOp::Eq | BinOp::Ne | BinOp::Concat | BinOp::ConcatWs => None,
                };
                if let Some(t) = required {
                    self.constrain_if_param(l, active, acc, &t);
                    self.constrain_if_param(r, active, acc, &t);
                }
                self.collect_param_constraints(l, active, acc);
                self.collect_param_constraints(r, active, acc);
            }
            ExprKind::UnOp(op, x) => {
                let t = match op {
                    UnOp::Neg => Type::Number,
                    UnOp::Not => Type::Boolean,
                };
                self.constrain_if_param(x, active, acc, &t);
                self.collect_param_constraints(x, active, acc);
            }
            ExprKind::If(cond, then_b, elifs, else_b) => {
                self.constrain_if_param(cond, active, acc, &Type::Boolean);
                self.collect_param_constraints(cond, active, acc);
                self.collect_param_constraints(then_b, active, acc);
                for (c, b) in elifs {
                    self.constrain_if_param(c, active, acc, &Type::Boolean);
                    self.collect_param_constraints(c, active, acc);
                    self.collect_param_constraints(b, active, acc);
                }
                self.collect_param_constraints(else_b, active, acc);
            }
            ExprKind::While(cond, body) => {
                self.constrain_if_param(cond, active, acc, &Type::Boolean);
                self.collect_param_constraints(cond, active, acc);
                self.collect_param_constraints(body, active, acc);
            }
            ExprKind::For(var, iter, body) => {
                self.collect_param_constraints(iter, active, acc);
                let mut inner = active.clone();
                inner.remove(var);
                self.collect_param_constraints(body, &inner, acc);
            }
            ExprKind::Let(name, _annot, value, body) => {
                self.collect_param_constraints(value, active, acc);
                let mut inner = active.clone();
                inner.remove(name);
                self.collect_param_constraints(body, &inner, acc);
            }
            ExprKind::Call(name, args) => {
                if let Some(sig) = self.ctx.funcs.get(name) {
                    for (a, pty) in args.iter().zip(sig.params.iter()) {
                        self.constrain_if_param(a, active, acc, pty);
                    }
                }
                for a in args {
                    self.collect_param_constraints(a, active, acc);
                }
            }
            ExprKind::MethodCall(recv, method, args) => {
                if let Some(t) = self.unique_method_owner(method) {
                    self.constrain_if_param(recv, active, acc, &t);
                }
                self.collect_param_constraints(recv, active, acc);
                for a in args {
                    self.collect_param_constraints(a, active, acc);
                }
            }
            ExprKind::New(tname, _ta, args) => {
                let expected = self.effective_ctor_params(tname);
                for (a, pty) in args.iter().zip(expected.iter()) {
                    self.constrain_if_param(a, active, acc, pty);
                }
                for a in args {
                    self.collect_param_constraints(a, active, acc);
                }
            }
            ExprKind::Block(xs) => {
                for x in xs {
                    self.collect_param_constraints(x, active, acc);
                }
            }
            ExprKind::Assign(_, v) => self.collect_param_constraints(v, active, acc),
            ExprKind::AssignField(recv, _, v) => {
                self.collect_param_constraints(recv, active, acc);
                self.collect_param_constraints(v, active, acc);
            }
            ExprKind::GetField(recv, _) => self.collect_param_constraints(recv, active, acc),
            ExprKind::Is(x, _) | ExprKind::As(x, _) => {
                self.collect_param_constraints(x, active, acc)
            }
            ExprKind::Base(args) => {
                for a in args {
                    self.collect_param_constraints(a, active, acc);
                }
            }
            ExprKind::Vector(elems) => {
                for x in elems {
                    self.collect_param_constraints(x, active, acc);
                }
            }
            ExprKind::VectorComp(elem, var, iter) => {
                self.collect_param_constraints(iter, active, acc);
                let mut inner = active.clone();
                inner.remove(var);
                self.collect_param_constraints(elem, &inner, acc);
            }
            ExprKind::Index(obj, idx) => {
                self.collect_param_constraints(obj, active, acc);
                self.collect_param_constraints(idx, active, acc);
            }
            ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Bool(_)
            | ExprKind::Ident(_)
            | ExprKind::SelfExpr => {}
        }
    }

    /// If `e` is a direct reference to an active parameter, fold `ty` into its
    /// constraint.
    fn constrain_if_param(
        &self,
        e: &Expr,
        active: &HashSet<String>,
        acc: &mut HashMap<String, ParamConstraint>,
        ty: &Type,
    ) {
        if let ExprKind::Ident(name) = &e.kind {
            if active.contains(name) {
                acc.entry(name.clone())
                    .or_insert(ParamConstraint::Unknown)
                    .add(&self.ctx, ty.clone());
            }
        }
    }

    /// Return the unique non-generic type or interface that *declares* `method`,
    /// or `None` when zero or more than one do. Used to infer a parameter's type
    /// from a method call on it (`function px(p) => p.getX()` ⇒ `p : Point`).
    /// Conservative on purpose: when the method name is shared (e.g. an override
    /// chain) it declines to constrain rather than guess.
    fn unique_method_owner(&self, method: &str) -> Option<Type> {
        let mut found: Option<String> = None;
        for (name, info) in &self.ctx.types {
            if info.generic_params.is_empty() && info.methods.contains_key(method) {
                if found.is_some() {
                    return None;
                }
                found = Some(name.clone());
            }
        }
        for (name, info) in &self.ctx.interfaces {
            if info.generic_params.is_empty() && info.methods.contains_key(method) {
                if found.is_some() {
                    return None;
                }
                found = Some(name.clone());
            }
        }
        found.map(Type::User)
    }

    // ---------------------------------------------- pass 3: bodies & entry

    pub fn check_bodies(&mut self, prog: &Program) {
        for f in &prog.functions {
            let mut env = Env::new();
            let sig = self.ctx.funcs.get(&f.name).cloned();
            if let Some(sig) = sig {
                self.current_generic_scope = f.generic_params.clone();
                for (p, pt) in f.params.iter().zip(sig.params.iter()) {
                    env.define(
                        &p.name,
                        Binding {
                            ty: pt.clone(),
                            span: p.span,
                        },
                    );
                }
                let body_ty = self.check_expr(&mut env, &f.body);
                if f.return_ty.is_some() {
                    self.require(&body_ty, &sig.returns, f.body.span);
                }
                self.current_generic_scope.clear();
            }
        }

        for td in &prog.types {
            let info = self.ctx.types.get(&td.name).cloned();
            if let Some(info) = info {
                self.current_generic_scope = td.generic_params.clone();
                // Validate explicit parent constructor arguments in `inherits Parent(a, b, ...)`.
                // These expressions can reference this type's constructor parameters, but not `self`.
                if let Some(parent_spec) = td.parent.as_ref() {
                    if let Some(args) = parent_spec.args.as_deref() {
                        // If `collect` already repaired an invalid parent to Object, skip to avoid cascades.
                        if info.parent != "Object" {
                            let expected = self.effective_ctor_params(&info.parent);
                            let mut env = Env::new();
                            for (pname, pty) in &info.ctor_params {
                                env.define(
                                    pname,
                                    Binding {
                                        ty: pty.clone(),
                                        span: td.span,
                                    },
                                );
                            }
                            let arg_tys: Vec<Type> =
                                args.iter().map(|a| self.check_expr(&mut env, a)).collect();
                            self.check_args(&expected, &arg_tys, args, parent_spec.span);
                        }
                    }
                }

                // Attribute initializers see only type parameters (A.7.2: "no self").
                for a in &td.attributes {
                    let mut env = Env::new();
                    for (pname, pty) in &info.ctor_params {
                        env.define(
                            pname,
                            Binding {
                                ty: pty.clone(),
                                span: td.span,
                            },
                        );
                    }
                    let init_ty = self.check_expr(&mut env, &a.init);
                    if let Some(declared) =
                        a.ty.as_ref()
                            .and_then(|t| self.ctx.resolve_type_ref_in_scope(t, &td.generic_params))
                    {
                        self.require(&init_ty, &declared, a.init.span);
                    }
                }
                let self_ty = if td.generic_params.is_empty() {
                    Type::User(td.name.clone())
                } else {
                    Type::Generic(
                        td.name.clone(),
                        td.generic_params
                            .iter()
                            .map(|p| Type::Param(p.clone()))
                            .collect(),
                    )
                };
                for m in &td.methods {
                    let mut env = Env::new();
                    env.define(
                        "self",
                        Binding {
                            ty: self_ty.clone(),
                            span: m.span,
                        },
                    );
                    let sig = info.methods.get(&m.name).cloned();
                    if let Some(sig) = sig {
                        for (p, pt) in m.params.iter().zip(sig.params.iter()) {
                            env.define(
                                &p.name,
                                Binding {
                                    ty: pt.clone(),
                                    span: p.span,
                                },
                            );
                        }
                        self.current_method = Some(MethodScope {
                            owner: td.name.clone(),
                            name: m.name.clone(),
                        });
                        let body_ty = self.check_expr(&mut env, &m.body);
                        self.current_method = None;
                        if m.return_ty.is_some() {
                            self.require(&body_ty, &sig.returns, m.body.span);
                        }
                    }
                }
                self.current_generic_scope.clear();
            }
        }

        let mut env = Env::new();
        let _ = self.check_expr(&mut env, &prog.entry);
    }

    // ---------------------------------------------- core: expression checker

    pub fn check_expr(&mut self, env: &mut Env, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Number(_) => Type::Number,
            ExprKind::String(_) => Type::String,
            ExprKind::Bool(_) => Type::Boolean,

            ExprKind::Ident(name) => match env.lookup(name) {
                Some(b) => b.ty.clone(),
                None => match self.ctx.builtin_consts.get(name) {
                    Some(t) => t.clone(),
                    None => {
                        self.errors.push(SemError::UndefinedVariable {
                            name: name.clone(),
                            span: e.span,
                        });
                        Type::Error
                    }
                },
            },

            ExprKind::SelfExpr => match env.lookup("self") {
                Some(b) => b.ty.clone(),
                None => {
                    self.errors.push(SemError::UndefinedVariable {
                        name: "self".into(),
                        span: e.span,
                    });
                    Type::Error
                }
            },

            ExprKind::BinOp(op, l, r) => {
                let lt = self.check_expr(env, l);
                let rt = self.check_expr(env, r);
                self.check_binop(*op, &lt, &rt, l.span, r.span)
            }

            ExprKind::UnOp(op, x) => {
                let xt = self.check_expr(env, x);
                match op {
                    UnOp::Neg => {
                        self.require(&xt, &Type::Number, x.span);
                        Type::Number
                    }
                    UnOp::Not => {
                        self.require(&xt, &Type::Boolean, x.span);
                        Type::Boolean
                    }
                }
            }

            ExprKind::Call(name, args) => {
                let arg_tys: Vec<Type> = args.iter().map(|a| self.check_expr(env, a)).collect();
                let sig = self.ctx.funcs.get(name).cloned();
                match sig {
                    Some(sig) => {
                        self.check_args(&sig.params, &arg_tys, args, e.span);
                        sig.returns
                    }
                    None => {
                        self.errors.push(SemError::UndefinedFunction {
                            name: name.clone(),
                            span: e.span,
                        });
                        Type::Error
                    }
                }
            }

            ExprKind::MethodCall(recv, name, args) => {
                let rt = self.check_expr(env, recv);
                let arg_tys: Vec<Type> = args.iter().map(|a| self.check_expr(env, a)).collect();
                // Builtin vector methods (A.12): a vector `T[]` provides
                // `size(): Number` plus the iterable protocol (`next(): Boolean`,
                // `current(): T`).
                if let Type::Vector(elem) = &rt {
                    let builtin = match name.as_str() {
                        "size" => Some((vec![], Type::Number)),
                        "next" => Some((vec![], Type::Boolean)),
                        "current" => Some((vec![], (**elem).clone())),
                        _ => None,
                    };
                    if let Some((params, returns)) = builtin {
                        self.check_args(&params, &arg_tys, args, e.span);
                        return returns;
                    }
                    self.errors.push(SemError::NoSuchMethod {
                        ty: rt.name(),
                        name: name.clone(),
                        span: e.span,
                    });
                    return Type::Error;
                }
                let (sig, subst) = match &rt {
                    Type::User(tn) => {
                        if self.ctx.is_interface(tn) {
                            (self.lookup_interface_method(tn, name), HashMap::new())
                        } else {
                            (self.lookup_inherited_method(tn, name), HashMap::new())
                        }
                    }
                    Type::Generic(tn, targs) => {
                        let subst: HashMap<String, Type> = if self.ctx.is_interface(tn) {
                            self.ctx
                                .interfaces
                                .get(tn)
                                .map(|info| {
                                    info.generic_params
                                        .iter()
                                        .cloned()
                                        .zip(targs.iter().cloned())
                                        .collect()
                                })
                                .unwrap_or_default()
                        } else {
                            self.ctx
                                .types
                                .get(tn)
                                .map(|info| {
                                    info.generic_params
                                        .iter()
                                        .cloned()
                                        .zip(targs.iter().cloned())
                                        .collect()
                                })
                                .unwrap_or_default()
                        };
                        let sig = if self.ctx.is_interface(tn) {
                            self.lookup_interface_method(tn, name)
                        } else {
                            self.lookup_inherited_method(tn, name)
                        };
                        (sig, subst)
                    }
                    _ => (None, HashMap::new()),
                };
                match sig {
                    Some(sig) => {
                        let params: Vec<Type> = sig
                            .params
                            .iter()
                            .map(|p| self.ctx.substitute(p, &subst))
                            .collect();
                        let returns = self.ctx.substitute(&sig.returns, &subst);
                        self.check_args(&params, &arg_tys, args, e.span);
                        returns
                    }
                    None => {
                        if !matches!(rt, Type::Error) {
                            self.errors.push(SemError::NoSuchMethod {
                                ty: rt.name(),
                                name: name.clone(),
                                span: e.span,
                            });
                        }
                        Type::Error
                    }
                }
            }

            ExprKind::GetField(recv, name) => {
                let is_self = matches!(recv.kind, ExprKind::SelfExpr);
                let rt = self.check_expr(env, recv);
                // A.7: attributes are private. They are reachable through `self`,
                // or — as in mainstream OOP languages — between instances of the
                // same type hierarchy from within that type's own methods (e.g.
                // `other.x` inside a method of the receiver's type). Access from
                // a context with no enclosing `self` (top-level) stays private.
                let accessible = is_self
                    || env.lookup("self").is_some_and(|enclosing| {
                        self.ctx.conforms(&rt, &enclosing.ty)
                            || self.ctx.conforms(&enclosing.ty, &rt)
                    });
                if !accessible {
                    self.errors.push(SemError::NoSuchAttribute {
                        ty: rt.name(),
                        name: name.clone(),
                        span: e.span,
                    });
                    return Type::Error;
                }
                let (tn_opt, subst) = match &rt {
                    Type::User(tn) => (Some(tn.clone()), HashMap::new()),
                    Type::Generic(tn, targs) => {
                        let subst = self
                            .ctx
                            .types
                            .get(tn)
                            .map(|info| {
                                info.generic_params
                                    .iter()
                                    .cloned()
                                    .zip(targs.iter().cloned())
                                    .collect()
                            })
                            .unwrap_or_default();
                        (Some(tn.clone()), subst)
                    }
                    _ => (None, HashMap::new()),
                };
                match tn_opt {
                    Some(tn) => match self.lookup_inherited_attr(&tn, name) {
                        Some(t) => self.ctx.substitute(&t, &subst),
                        None => {
                            self.errors.push(SemError::NoSuchAttribute {
                                ty: tn,
                                name: name.clone(),
                                span: e.span,
                            });
                            Type::Error
                        }
                    },
                    None => Type::Error,
                }
            }

            ExprKind::Let(name, annot, value, body) => {
                if matches!(name.as_str(), "self") {
                    self.errors.push(SemError::ReservedName {
                        name: name.clone(),
                        span: e.span,
                    });
                }
                let vt = self.check_expr(env, value);
                let bound = match annot {
                    Some(t) => match self
                        .ctx
                        .resolve_type_ref_in_scope(t, &self.current_generic_scope)
                    {
                        Some(at) => {
                            self.require(&vt, &at, value.span);
                            at
                        }
                        None => {
                            self.errors.push(SemError::UndefinedType {
                                name: t.to_string(),
                                span: e.span,
                            });
                            vt
                        }
                    },
                    None => vt,
                };
                env.enter();
                if !matches!(name.as_str(), "self") {
                    env.define(
                        name,
                        Binding {
                            ty: bound,
                            span: e.span,
                        },
                    );
                }
                let bt = self.check_expr(env, body);
                env.leave();
                bt
            }

            ExprKind::Assign(name, value) => {
                let vt = self.check_expr(env, value);
                if name == "self" {
                    self.errors.push(SemError::SelfAssign { span: e.span });
                    return Type::Error;
                }
                match env.lookup(name) {
                    Some(b) => {
                        let target_ty = b.ty.clone();
                        self.require(&vt, &target_ty, value.span);
                        target_ty
                    }
                    None => {
                        self.errors.push(SemError::UndefinedVariable {
                            name: name.clone(),
                            span: e.span,
                        });
                        Type::Error
                    }
                }
            }

            ExprKind::AssignField(recv, name, value) => {
                let is_self = matches!(recv.kind, ExprKind::SelfExpr);
                if !is_self {
                    self.errors
                        .push(SemError::NonSelfFieldAssign { span: e.span });
                    return Type::Error;
                }
                let rt = self.check_expr(env, recv);
                let vt = self.check_expr(env, value);
                let (tn, subst) = match &rt {
                    Type::User(tn) => (tn.clone(), HashMap::new()),
                    Type::Generic(tn, targs) => {
                        let subst = self
                            .ctx
                            .types
                            .get(tn)
                            .map(|info| {
                                info.generic_params
                                    .iter()
                                    .cloned()
                                    .zip(targs.iter().cloned())
                                    .collect()
                            })
                            .unwrap_or_default();
                        (tn.clone(), subst)
                    }
                    _ => return Type::Error,
                };
                match self.lookup_inherited_attr(&tn, name) {
                    Some(t) => {
                        let resolved = self.ctx.substitute(&t, &subst);
                        self.require(&vt, &resolved, value.span);
                        resolved
                    }
                    None => {
                        self.errors.push(SemError::NoSuchAttribute {
                            ty: tn,
                            name: name.clone(),
                            span: e.span,
                        });
                        Type::Error
                    }
                }
            }

            ExprKind::If(cond, then_b, elifs, else_b) => {
                let ct = self.check_expr(env, cond);
                self.require(&ct, &Type::Boolean, cond.span);
                let mut result = self.check_expr(env, then_b);
                for (c, b) in elifs {
                    let ct = self.check_expr(env, c);
                    self.require(&ct, &Type::Boolean, c.span);
                    let bt = self.check_expr(env, b);
                    result = self.ctx.lca(&result, &bt);
                }
                let et = self.check_expr(env, else_b);
                self.ctx.lca(&result, &et)
            }

            ExprKind::While(cond, body) => {
                let ct = self.check_expr(env, cond);
                self.require(&ct, &Type::Boolean, cond.span);
                self.check_expr(env, body)
            }

            ExprKind::For(var, iter, body) => {
                // The loop variable takes the element type of the iterator: the
                // return type of its `current()` method. `range(...)` is typed as
                // `Object` (its `Range` type is a runtime-only builtin), so it
                // falls back to `Number`, matching the IR lowering.
                let iter_ty = self.check_expr(env, iter);
                let elem_ty = match &iter_ty {
                    // Poison: a prior error already fired; stay silent.
                    Type::Error => Type::Error,
                    // `Object` is the escape hatch for `range(...)` and other
                    // runtime-only iterables; bind the element as `Number`.
                    Type::Object => Type::Number,
                    // A typed iterable `T*` or vector `T[]` binds the element as `T`.
                    Type::Iterable(elem) | Type::Vector(elem) => (**elem).clone(),
                    // A user type/interface is iterable only if it provides the
                    // iterator protocol (probed via `current()`).
                    Type::User(_) | Type::Generic(_, _) => {
                        match self.iterator_element_type(&iter_ty) {
                            Some(t) => t,
                            None => {
                                self.errors.push(SemError::NotIterable {
                                    ty: iter_ty.name(),
                                    span: iter.span,
                                });
                                Type::Error
                            }
                        }
                    }
                    // Primitives (`Number`/`String`/`Boolean`) and type params
                    // are not iterable.
                    _ => {
                        self.errors.push(SemError::NotIterable {
                            ty: iter_ty.name(),
                            span: iter.span,
                        });
                        Type::Error
                    }
                };
                env.enter();
                env.define(
                    var,
                    Binding {
                        ty: elem_ty,
                        span: e.span,
                    },
                );
                let bt = self.check_expr(env, body);
                env.leave();
                bt
            }

            ExprKind::Block(exprs) => {
                let mut last = Type::Object;
                for x in exprs {
                    last = self.check_expr(env, x);
                }
                last
            }

            ExprKind::New(tname, type_args, args) => {
                let arg_tys: Vec<Type> = args.iter().map(|a| self.check_expr(env, a)).collect();
                if self.ctx.is_interface(tname) {
                    self.errors.push(SemError::CannotInstantiateInterface {
                        name: tname.clone(),
                        span: e.span,
                    });
                    return Type::Error;
                }
                let Some(info) = self.ctx.types.get(tname).cloned() else {
                    self.errors.push(SemError::UndefinedType {
                        name: tname.clone(),
                        span: e.span,
                    });
                    return Type::Error;
                };
                // Validate generic arity against the type declaration. When the
                // counts differ, at least one side is non-empty, so the mismatch
                // covers both "missing args" and "args on a non-generic type".
                if info.generic_params.len() != type_args.len() {
                    self.errors.push(SemError::Arity {
                        expected: info.generic_params.len(),
                        found: type_args.len(),
                        span: e.span,
                    });
                    return Type::Error;
                }
                // Resolve generic arguments and build substitution.
                let resolved_args: Vec<Type> = type_args
                    .iter()
                    .map(|t| {
                        self.ctx
                            .resolve_type_ref_in_scope(t, &self.current_generic_scope)
                            .unwrap_or_else(|| {
                                self.errors.push(SemError::UndefinedType {
                                    name: t.to_string(),
                                    span: e.span,
                                });
                                Type::Error
                            })
                    })
                    .collect();
                let subst: HashMap<String, Type> = info
                    .generic_params
                    .iter()
                    .cloned()
                    .zip(resolved_args.iter().cloned())
                    .collect();
                let expected: Vec<Type> = self
                    .effective_ctor_params(tname)
                    .iter()
                    .map(|t| self.ctx.substitute(t, &subst))
                    .collect();
                // Ctor-param inference side-channel (A.9.3): record, per declaring
                // type, the LCA of the argument types seen at every `new` site, so
                // an un-annotated ctor param can be inferred from its callers.
                // Only active during `infer_ctor_params` (otherwise `None`).
                if self.ctor_arg_obs.is_some() {
                    if let Some(owner) = self.effective_ctor_owner(tname) {
                        let arity = self
                            .ctx
                            .types
                            .get(&owner)
                            .map(|i| i.ctor_params.len())
                            .unwrap_or(0);
                        if arity > 0 {
                            let mut slots = self
                                .ctor_arg_obs
                                .as_ref()
                                .and_then(|m| m.get(&owner).cloned())
                                .unwrap_or_else(|| vec![Type::Error; arity]);
                            for (slot, aty) in slots.iter_mut().zip(arg_tys.iter()) {
                                *slot = self.ctx.lca(slot, aty);
                            }
                            if let Some(obs) = self.ctor_arg_obs.as_mut() {
                                obs.insert(owner, slots);
                            }
                        }
                    }
                }
                self.check_args(&expected, &arg_tys, args, e.span);
                if info.generic_params.is_empty() {
                    Type::User(tname.clone())
                } else {
                    Type::Generic(tname.clone(), resolved_args)
                }
            }

            ExprKind::Is(expr, tref) => {
                let _ = self.check_expr(env, expr);
                if self
                    .ctx
                    .resolve_type_ref_in_scope(tref, &self.current_generic_scope)
                    .is_none()
                {
                    self.errors.push(SemError::UndefinedType {
                        name: tref.to_string(),
                        span: e.span,
                    });
                }
                Type::Boolean
            }

            ExprKind::As(expr, tref) => {
                let _ = self.check_expr(env, expr);
                match self
                    .ctx
                    .resolve_type_ref_in_scope(tref, &self.current_generic_scope)
                {
                    Some(t) => t,
                    None => {
                        self.errors.push(SemError::UndefinedType {
                            name: tref.to_string(),
                            span: e.span,
                        });
                        Type::Error
                    }
                }
            }

            ExprKind::Base(args) => {
                let arg_tys: Vec<Type> = args.iter().map(|a| self.check_expr(env, a)).collect();
                let Some(scope) = self.current_method.clone() else {
                    self.errors
                        .push(SemError::BaseOutsideOverride { span: e.span });
                    return Type::Error;
                };
                let parent = self
                    .ctx
                    .types
                    .get(&scope.owner)
                    .map(|i| i.parent.clone())
                    .unwrap_or_else(|| "Object".into());
                match self.lookup_inherited_method(&parent, &scope.name) {
                    Some(sig) => {
                        self.check_args(&sig.params, &arg_tys, args, e.span);
                        sig.returns
                    }
                    None => {
                        self.errors
                            .push(SemError::BaseNoParentMethod { span: e.span });
                        Type::Error
                    }
                }
            }

            // ── Vectors (A.12) ──────────────────────────────────────────────
            ExprKind::Vector(elems) => {
                // The element type is the lowest common ancestor of every
                // element's type. An empty literal `[]` is `Object[]`.
                let mut elem_ty: Option<Type> = None;
                for x in elems {
                    let xt = self.check_expr(env, x);
                    elem_ty = Some(match elem_ty {
                        None => xt,
                        Some(acc) => self.ctx.lca(&acc, &xt),
                    });
                }
                Type::Vector(Box::new(elem_ty.unwrap_or(Type::Object)))
            }

            ExprKind::VectorComp(elem, var, iter) => {
                let iter_ty = self.check_expr(env, iter);
                let bound = match &iter_ty {
                    Type::Error => Type::Error,
                    Type::Object => Type::Number,
                    Type::Iterable(t) | Type::Vector(t) => (**t).clone(),
                    Type::User(_) | Type::Generic(_, _) => {
                        match self.iterator_element_type(&iter_ty) {
                            Some(t) => t,
                            None => {
                                self.errors.push(SemError::NotIterable {
                                    ty: iter_ty.name(),
                                    span: iter.span,
                                });
                                Type::Error
                            }
                        }
                    }
                    _ => {
                        self.errors.push(SemError::NotIterable {
                            ty: iter_ty.name(),
                            span: iter.span,
                        });
                        Type::Error
                    }
                };
                env.enter();
                env.define(
                    var,
                    Binding {
                        ty: bound,
                        span: e.span,
                    },
                );
                let elem_ty = self.check_expr(env, elem);
                env.leave();
                Type::Vector(Box::new(elem_ty))
            }

            ExprKind::Index(obj, idx) => {
                let ot = self.check_expr(env, obj);
                let it = self.check_expr(env, idx);
                self.require(&it, &Type::Number, idx.span);
                match &ot {
                    Type::Vector(elem) => (**elem).clone(),
                    Type::Error => Type::Error,
                    _ => {
                        self.errors.push(SemError::NotIndexable {
                            ty: ot.name(),
                            span: obj.span,
                        });
                        Type::Error
                    }
                }
            }
        }
    }

    // ------------------------------------------------ binop / arity helpers

    fn check_binop(&mut self, op: BinOp, lt: &Type, rt: &Type, ls: Span, rs: Span) -> Type {
        use BinOp::*;
        match op {
            Add | Sub | Mul | Div | Pow | Mod => {
                self.require(lt, &Type::Number, ls);
                self.require(rt, &Type::Number, rs);
                Type::Number
            }
            Concat | ConcatWs => {
                // A.2.2: `@` concatenates strings or string-representations of numbers.
                // Enforce `String`/`Number` operands (plus `Error` poison) and produce `String`.
                self.require_concat_operand(lt, ls);
                self.require_concat_operand(rt, rs);
                Type::String
            }
            Lt | Le | Gt | Ge => {
                self.require(lt, &Type::Number, ls);
                self.require(rt, &Type::Number, rs);
                Type::Boolean
            }
            Eq | Ne => Type::Boolean,
            And | Or => {
                self.require(lt, &Type::Boolean, ls);
                self.require(rt, &Type::Boolean, rs);
                Type::Boolean
            }
        }
    }

    fn require_concat_operand(&mut self, found: &Type, span: Span) {
        if matches!(found, Type::Error) {
            return;
        }
        if matches!(found, Type::String | Type::Number) {
            return;
        }
        self.errors.push(SemError::Mismatch {
            expected: "String or Number".into(),
            found: found.name(),
            span,
        });
    }

    fn check_args(&mut self, params: &[Type], arg_tys: &[Type], args: &[Expr], call_span: Span) {
        if params.len() != args.len() {
            self.errors.push(SemError::Arity {
                expected: params.len(),
                found: args.len(),
                span: call_span,
            });
            return;
        }
        for ((at, et), arg) in arg_tys.iter().zip(params.iter()).zip(args.iter()) {
            self.require(at, et, arg.span);
        }
    }

    fn require(&mut self, found: &Type, expected: &Type, span: Span) {
        if !self.ctx.conforms(found, expected) {
            self.errors.push(SemError::Mismatch {
                expected: expected.name(),
                found: found.name(),
                span,
            });
        }
    }
}

/// Run all three semantic passes and return either the populated type context
/// or the list of diagnostics. Errors are accumulated rather than fatal so
/// that one bad expression doesn't hide every later one.
pub fn analyze(prog: &Program) -> Result<TypeCtx, Vec<SemError>> {
    let mut c = Checker::new();
    c.collect(prog);
    c.sign(prog);
    c.infer_ctor_params(prog);
    c.infer_params(prog);
    c.infer_returns(prog);
    c.check_interfaces(prog);
    c.check_overrides(prog);
    c.check_bodies(prog);
    if c.errors.is_empty() {
        Ok(c.ctx)
    } else {
        Err(c.errors)
    }
}
