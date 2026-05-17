#![allow(rustdoc::invalid_rust_codeblocks)]

//! Semantic analysis for HULK — role B's deliverable.
//!
//! ## Scope
//!
//! This crate implements **only** semantic analysis: name resolution, type
//! checking, conformance, override validation. The lexer/parser (role A) and
//! the IR/VM (role C) live in their own crates and are not touched here.
//!
//! ## AST dependency
//!
//! The semantic analyzer operates on the shared [`hulk_ast`] types
//! (`Program`, `Expr`, `ExprKind`, etc.), re-exported here as [`ast`] for
//! internal convenience.
//!
//! ## Pipeline
//!
//! Three passes plus an override-identity check, all sharing a single mutable
//! [`Checker`]:
//!
//! 1. `collect` — register every type name; reject duplicate types,
//!    inheritance from builtins, and cycles.
//! 2. `sign` — fill attribute types, method signatures,
//!    constructor parameters, function signatures. After
//!    this pass the global signature environment is
//!    complete (HULK spec A.3 — "any function can call
//!    any other regardless of declaration order").
//! 3. `check_overrides` — `T.m` and `Parent.m` must have identical signatures
//!    (HULK spec A.7.4 — "the exact same signature").
//! 4. `check_bodies` — walk every function/method/attribute body and the
//!    program's entry expression, propagating types and
//!    accumulating diagnostics.
//!
//! See [`analyze`] for the canonical entry point.

pub use hulk_ast as ast;
pub mod check;
pub mod env;
pub mod error;
pub mod types;

pub use check::{Checker, analyze};
pub use env::{Binding, Env};
pub use error::SemError;
pub use types::{FunctionSig, MethodSig, Type, TypeCtx, TypeInfo};
