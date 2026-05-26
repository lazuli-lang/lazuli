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

/// Single error type returned by every parser entry point.
///
/// Two flavours: `Pest` carries a span-anchored authoring error (the
/// common case); `Expected` is reserved for internal-only "this branch
/// should be unreachable" failures with no useful span. The analyzer
/// upgrades these into diagnostics with codes downstream.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Span-anchored authoring error (unknown keyword, bad indent, ...).
    #[error("{message}")]
    Pest { message: String, span: Span },

    /// Internal "unreachable branch" failure with no source location.
    #[error("internal parser error: expected {expected}")]
    Expected { expected: &'static str },
}

impl ParseError {
    /// Source span best-effort. `Pest` returns the carried span;
    /// `Expected` returns a synthetic `0..1` so downstream consumers
    /// can still produce a placeholder diagnostic.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::{ParseError, Span};
    ///
    /// let e = ParseError::Pest {
    ///     message: "unexpected token".into(),
    ///     span: Span::new(7, 12),
    /// };
    /// assert_eq!(e.span(), Span::new(7, 12));
    ///
    /// let internal = ParseError::Expected { expected: "indent block" };
    /// assert_eq!(internal.span(), Span::new(0, 1));
    /// ```
    pub fn span(&self) -> Span {
        match self {
            Self::Pest { span, .. } => *span,
            Self::Expected { .. } => Span::new(0, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pest_error_carries_message_and_span() {
        let e = ParseError::Pest {
            message: "bad token".into(),
            span: Span::new(3, 5),
        };
        assert_eq!(e.span(), Span::new(3, 5));
        assert!(e.to_string().contains("bad token"));
    }

    #[test]
    fn expected_error_synthesises_span() {
        let e = ParseError::Expected {
            expected: "block child",
        };
        assert_eq!(e.span(), Span::new(0, 1));
    }
}
