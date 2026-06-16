//! Semantic-analysis diagnostics. Every variant carries the span that should
//! be highlighted in the source by the caller (typically the CLI).

use crate::ast::Span;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemError {
    #[error("reserved name `{name}`")]
    ReservedName { name: String, span: Span },

    #[error("reserved type name `{name}`")]
    ReservedTypeName { name: String, span: Span },

    #[error("undefined variable `{name}`")]
    UndefinedVariable { name: String, span: Span },

    #[error("undefined function `{name}`")]
    UndefinedFunction { name: String, span: Span },

    #[error("undefined type `{name}`")]
    UndefinedType { name: String, span: Span },

    #[error("type mismatch: expected `{expected}`, found `{found}`")]
    Mismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("cannot inherit from built-in type `{name}`")]
    InheritBuiltin { name: String, span: Span },

    #[error("cyclic inheritance involving `{name}`")]
    CyclicInheritance { name: String, span: Span },

    #[error("duplicate type `{name}`")]
    DuplicateType { name: String, span: Span },

    #[error("duplicate function `{name}`")]
    DuplicateFunction { name: String, span: Span },

    #[error("method `{name}` overrides parent with a different signature")]
    OverrideSignatureMismatch { name: String, span: Span },

    #[error("wrong number of arguments: expected {expected}, found {found}")]
    Arity {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("type `{ty}` has no attribute `{name}`")]
    NoSuchAttribute {
        ty: String,
        name: String,
        span: Span,
    },

    #[error("type `{ty}` has no method `{name}`")]
    NoSuchMethod {
        ty: String,
        name: String,
        span: Span,
    },

    #[error("`self` is not a valid assignment target")]
    SelfAssign { span: Span },

    #[error("attributes are private; can only be assigned through `self`")]
    NonSelfFieldAssign { span: Span },

    #[error("`base()` used outside an overriding method")]
    BaseOutsideOverride { span: Span },

    #[error("`base()` is only valid when the parent declares a method with the same name")]
    BaseNoParentMethod { span: Span },

    #[error("`{name}` is not an interface")]
    NotAnInterface { name: String, span: Span },

    #[error("type `{ty}` does not implement method `{method}` required by interface `{iface}`")]
    MissingInterfaceMethod {
        ty: String,
        iface: String,
        method: String,
        span: Span,
    },

    #[error(
        "method `{method}` in type `{ty}` has a different signature than the one required by interface `{iface}`"
    )]
    InterfaceSignatureMismatch {
        ty: String,
        iface: String,
        method: String,
        span: Span,
    },

    #[error("cannot instantiate interface `{name}` — use a concrete type")]
    CannotInstantiateInterface { name: String, span: Span },

    #[error(
        "type `{ty}` is not iterable (the Iterable protocol requires both `next(): Boolean` and `current()`)"
    )]
    NotIterable { ty: String, span: Span },
}

impl SemError {
    pub fn span(&self) -> Span {
        match self {
            SemError::ReservedName { span, .. }
            | SemError::ReservedTypeName { span, .. }
            | SemError::UndefinedVariable { span, .. }
            | SemError::UndefinedFunction { span, .. }
            | SemError::UndefinedType { span, .. }
            | SemError::Mismatch { span, .. }
            | SemError::InheritBuiltin { span, .. }
            | SemError::CyclicInheritance { span, .. }
            | SemError::DuplicateType { span, .. }
            | SemError::DuplicateFunction { span, .. }
            | SemError::OverrideSignatureMismatch { span, .. }
            | SemError::Arity { span, .. }
            | SemError::NoSuchAttribute { span, .. }
            | SemError::NoSuchMethod { span, .. }
            | SemError::SelfAssign { span }
            | SemError::NonSelfFieldAssign { span }
            | SemError::BaseOutsideOverride { span }
            | SemError::BaseNoParentMethod { span } => *span,
            SemError::NotAnInterface { span, .. }
            | SemError::MissingInterfaceMethod { span, .. }
            | SemError::InterfaceSignatureMismatch { span, .. }
            | SemError::CannotInstantiateInterface { span, .. }
            | SemError::NotIterable { span, .. } => *span,
        }
    }
}
