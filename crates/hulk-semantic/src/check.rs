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
use std::collections::HashMap;

#[derive(Clone)]
struct MethodScope {
    owner: String,
    name: String,
}

pub struct Checker {
    pub ctx: TypeCtx,
    pub errors: Vec<SemError>,
    current_method: Option<MethodScope>,
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
                    ctor_params: Vec::new(),
                    parent_args: td.parent.as_ref().and_then(|p| p.args.clone()),
                    attrs: HashMap::new(),
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
            if matches!(f.name.as_str(), "self" | "base") {
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
                if matches!(p.name.as_str(), "self" | "base") {
                    self.errors.push(SemError::ReservedName {
                        name: p.name.clone(),
                        span: p.span,
                    });
                }
            }
            let params = f
                .params
                .iter()
                .map(|p| self.resolve_or_default(p.ty.as_deref(), p.span))
                .collect();
            let returns = match &f.return_ty {
                Some(n) => self.resolve_or_default(Some(n), f.span),
                None => Type::Object,
            };
            self.ctx
                .funcs
                .insert(f.name.clone(), FunctionSig { params, returns });
        }
        for td in &prog.types {
            for p in &td.type_params {
                if matches!(p.name.as_str(), "self" | "base") {
                    self.errors.push(SemError::ReservedName {
                        name: p.name.clone(),
                        span: p.span,
                    });
                }
            }
            let ctor_params: Vec<(String, Type)> = td
                .type_params
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        self.resolve_or_default(p.ty.as_deref(), p.span),
                    )
                })
                .collect();
            let mut attrs = HashMap::new();
            for a in &td.attributes {
                attrs.insert(
                    a.name.clone(),
                    self.resolve_or_default(a.ty.as_deref(), a.span),
                );
            }
            let mut methods = HashMap::new();
            for m in &td.methods {
                for p in &m.params {
                    if matches!(p.name.as_str(), "self" | "base") {
                        self.errors.push(SemError::ReservedName {
                            name: p.name.clone(),
                            span: p.span,
                        });
                    }
                }
                let params = m
                    .params
                    .iter()
                    .map(|p| self.resolve_or_default(p.ty.as_deref(), p.span))
                    .collect();
                let returns = match &m.return_ty {
                    Some(n) => self.resolve_or_default(Some(n), m.span),
                    None => Type::Object,
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
            if let Some(info) = self.ctx.types.get_mut(&td.name) {
                info.ctor_params = ctor_params;
                info.attrs = attrs;
                info.methods = methods;
            }
        }
    }

    fn resolve_or_default(&mut self, name: Option<&str>, span: Span) -> Type {
        match name {
            Some(n) => match self.ctx.resolve_name(n) {
                Some(t) => t,
                None => {
                    self.errors.push(SemError::UndefinedType {
                        name: n.into(),
                        span,
                    });
                    Type::Object
                }
            },
            None => Type::Object,
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

    // ---------------------------------------------- pass 3: bodies & entry

    pub fn check_bodies(&mut self, prog: &Program) {
        for f in &prog.functions {
            let mut env = Env::new();
            let sig = self.ctx.funcs.get(&f.name).cloned();
            if let Some(sig) = sig {
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
            }
        }

        for td in &prog.types {
            let info = self.ctx.types.get(&td.name).cloned();
            if let Some(info) = info {
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
                    if let Some(declared) = a.ty.as_ref().and_then(|n| self.ctx.resolve_name(n)) {
                        self.require(&init_ty, &declared, a.init.span);
                    }
                }
                for m in &td.methods {
                    let mut env = Env::new();
                    env.define(
                        "self",
                        Binding {
                            ty: Type::User(td.name.clone()),
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
                let sig = match &rt {
                    Type::User(tn) => self.lookup_inherited_method(tn, name),
                    _ => None,
                };
                match sig {
                    Some(sig) => {
                        self.check_args(&sig.params, &arg_tys, args, e.span);
                        sig.returns
                    }
                    None => {
                        if !matches!(rt, Type::Error) {
                            self.errors.push(SemError::NoSuchMethod {
                                ty: rt.name().into(),
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
                if !is_self {
                    // A.7: attributes are private — accessible only via `self`.
                    self.errors.push(SemError::NoSuchAttribute {
                        ty: rt.name().into(),
                        name: name.clone(),
                        span: e.span,
                    });
                    return Type::Error;
                }
                match &rt {
                    Type::User(tn) => match self.lookup_inherited_attr(tn, name) {
                        Some(t) => t,
                        None => {
                            self.errors.push(SemError::NoSuchAttribute {
                                ty: tn.clone(),
                                name: name.clone(),
                                span: e.span,
                            });
                            Type::Error
                        }
                    },
                    _ => Type::Error,
                }
            }

            ExprKind::Let(name, annot, value, body) => {
                if matches!(name.as_str(), "self" | "base") {
                    self.errors.push(SemError::ReservedName {
                        name: name.clone(),
                        span: e.span,
                    });
                }
                let vt = self.check_expr(env, value);
                let bound = match annot {
                    Some(t) => match self.ctx.resolve_name(t) {
                        Some(at) => {
                            self.require(&vt, &at, value.span);
                            at
                        }
                        None => {
                            self.errors.push(SemError::UndefinedType {
                                name: t.clone(),
                                span: e.span,
                            });
                            vt
                        }
                    },
                    None => vt,
                };
                env.enter();
                if !matches!(name.as_str(), "self" | "base") {
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
                let Type::User(tn) = &rt else {
                    return Type::Error;
                };
                match self.lookup_inherited_attr(tn, name) {
                    Some(t) => {
                        self.require(&vt, &t, value.span);
                        t
                    }
                    None => {
                        self.errors.push(SemError::NoSuchAttribute {
                            ty: tn.clone(),
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
                // The iterable protocol is not yet modelled. We bind `var` to
                // `Number` (matches `range(...)`); generic iterables will
                // require extending `TypeCtx` with a `Protocol` notion.
                let _ = self.check_expr(env, iter);
                env.enter();
                env.define(
                    var,
                    Binding {
                        ty: Type::Number,
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

            ExprKind::New(tname, args) => {
                let arg_tys: Vec<Type> = args.iter().map(|a| self.check_expr(env, a)).collect();
                if !self.ctx.types.contains_key(tname) {
                    self.errors.push(SemError::UndefinedType {
                        name: tname.clone(),
                        span: e.span,
                    });
                    return Type::Error;
                }
                let expected = self.effective_ctor_params(tname);
                self.check_args(&expected, &arg_tys, args, e.span);
                Type::User(tname.clone())
            }

            ExprKind::Is(expr, _ty) => {
                let _ = self.check_expr(env, expr);
                if self.ctx.resolve_name(_ty).is_none() {
                    self.errors.push(SemError::UndefinedType {
                        name: _ty.clone(),
                        span: e.span,
                    });
                }
                Type::Boolean
            }

            ExprKind::As(expr, tname) => {
                let _ = self.check_expr(env, expr);
                match self.ctx.resolve_name(tname) {
                    Some(t) => t,
                    None => {
                        self.errors.push(SemError::UndefinedType {
                            name: tname.clone(),
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
            found: found.name().into(),
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
                expected: expected.name().into(),
                found: found.name().into(),
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
    c.check_overrides(prog);
    c.check_bodies(prog);
    if c.errors.is_empty() {
        Ok(c.ctx)
    } else {
        Err(c.errors)
    }
}
