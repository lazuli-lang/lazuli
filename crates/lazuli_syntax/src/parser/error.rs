//! `ParseError` — the single error type returned by every parser entry point.
//!
//! There are two flavours: `Pest` carries a span-anchored authoring error
//! (the common case — bad indent, unknown keyword, missing token); `Expected`
//! is reserved for internal-only "this branch should be unreachable" failures
//! where we have no useful span to attach. Callers should treat both as
//! user-facing for now — the analyzer crate is what upgrades these into
//! diagnostics with codes.

use thiserror::Error;

use crate::ast::Span;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("{message}")]
    Pest { message: String, span: Span },

    #[error("internal parser error: expected {expected}")]
    Expected { expected: &'static str },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            Self::Pest { span, .. } => *span,
            Self::Expected { .. } => Span::new(0, 1),
        }
    }
}
