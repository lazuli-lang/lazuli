//! `ParseError` — the single error type returned by every parser entry point.
//!
//! There are two flavours: `Pest` carries a span-anchored authoring error
//! (the common case — bad indent, unknown keyword, missing token); `Expected`
//! is reserved for internal-only "this branch should be unreachable" failures
//! where we have no useful span to attach. Callers should treat both as
//! user-facing for now — the analyzer crate is what upgrades these into
//! diagnostics with codes.
//!
//! Codified parser-error constants ship as `pub const E_*` strings here so
//! the analyzer and downstream tooling can recognise them by prefix on the
//! `Pest.message` field — see `E_WORKFLOW_RETIRED` below. The convention is
//! `[<CODE>] <message>` so a plain text search for `E-FOO-BAR` finds the
//! coded diagnostic across the parser and the upgraded analyzer diagnostic.

use thiserror::Error;

use crate::ast::Span;

/// `E-WORKFLOW-RETIRED` — emitted when a `.lzi` author still uses the
/// retired `workflow <name>` feature-child keyword. Replacement: a
/// `lifecycle <field>` block nested inside the targeted `resource`
/// (proposal: `docs/proposals/lifecycle-vocab.md` §2.1, §2.1.4 + §5
/// last row). The parser carries this constant as the leading
/// `[E-WORKFLOW-RETIRED]` tag on the `ParseError::Pest.message`; the
/// analyzer's diagnostic upgrade reads the prefix to populate the
/// stable diagnostic `code` field.
pub const E_WORKFLOW_RETIRED: &str = "E-WORKFLOW-RETIRED";

/// `E-CONTEXT-RETIRED` — emitted when a `.lzi` author writes a
/// feature-header-level `context "<path>"` line. That form was never a
/// recognised parser branch, so it used to be silently dropped (zero
/// `context_path` in the IR), violating inviolable rule #7 (no silent
/// runtime behaviour). Feature context is now resolved by CONVENTION: a
/// co-located `<feature>.ctx.md` sidecar next to the `.lzi` file (no
/// keyword, no path argument). Mirrors `E-WORKFLOW-RETIRED`: the parser
/// carries this constant as the leading `[E-CONTEXT-RETIRED]` tag on the
/// `ParseError::Pest.message` so downstream tooling recognises it by
/// code. NOTE: this is scoped to the feature-header `context "<string>"`
/// form only — the live agent-body `context <expr>` keyword and
/// `context:` map keys are unaffected.
pub const E_CONTEXT_RETIRED: &str = "E-CONTEXT-RETIRED";

/// `E-ATTACH-CTX-RETIRED` — emitted when a `.lzi` author writes a
/// feature-header-level `attach_ctx "<path>"` line. The `attach_ctx`
/// keyword was retired in favour of a co-located `<feature>.ctx.md`
/// CONVENTION: the analyzer probes `<dir-of-the-.lzi>/<feature>.ctx.md`
/// and auto-attaches it when present (no keyword, no path argument, a
/// single resolution base). Mirrors `E-CONTEXT-RETIRED` /
/// `E-WORKFLOW-RETIRED`: the parser carries this constant as the leading
/// `[E-ATTACH-CTX-RETIRED]` tag on the `ParseError::Pest.message` so
/// downstream tooling recognises it by code.
pub const E_ATTACH_CTX_RETIRED: &str = "E-ATTACH-CTX-RETIRED";

/// Single error type returned by every parser entry point.
///
/// Two flavours: `Pest` carries a span-anchored authoring error (the
/// common case); `Expected` is reserved for internal-only "this branch
/// should be unreachable" failures with no useful span. The analyzer
/// upgrades these into diagnostics with codes downstream.
#[derive(Debug, Error)]
#[non_exhaustive]
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
