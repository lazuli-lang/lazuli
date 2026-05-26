//! Iron-hand context-vocabulary — feature-scoped intent declarations.
//!
//! `purpose "<sentence>"`, `non_goals { "..." }`, `attach_ctx "<path>"` —
//! surfaced by `VOCAB-CONTEXT-*` lints. The `tdd-iron-hand` preset
//! promotes the lints from warn to error. See
//! `docs/canonical-semantics.md#feature-context-vocabulary`.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// Iron-hand `purpose "<sentence>"` line. The string is whatever the
/// author wrote between the quotes; empty / whitespace-only is allowed
/// at parse time so the lint can fire a precise diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeaturePurpose {
    pub text: String,
    pub span: Span,
}

/// Iron-hand `non_goals` block. Children are one-quoted-string-per-line
/// entries (mirrors how `uses` lists work for the wire-thin slice).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureNonGoals {
    pub entries: Vec<String>,
    pub span: Span,
}

/// Iron-hand `attach_ctx "<relative-path>"` line. Path is verbatim;
/// resolution against the project root happens in the doctor lint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureAttachCtx {
    pub path: String,
    pub span: Span,
}
