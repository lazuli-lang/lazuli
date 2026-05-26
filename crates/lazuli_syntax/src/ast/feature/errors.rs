//! IR Error-Vocab (Cell PARSE-1) — `errors` block surface AST.
//!
//! The `errors` block lives at indent 2 under the feature header. Closed
//! children at indent 4:
//!
//!   * `default hide` / `default expose` — at most one.
//!   * `expose client 4xx <comma-list>` — at most one.
//!   * `expose client 5xx <comma-list>` — at most one.
//!   * `<code> message @translation.<key>` — zero or more, one per
//!     closed-catalog error code (`policy_denied`, `validation_failed`,
//!     `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`,
//!     `method_not_allowed`, `integration_error`). Closed-catalog
//!     enforcement lives in the analyzer / doctor; the parser keeps the
//!     code as a verbatim identifier so unknown codes surface as
//!     `ERR-VOCAB-CODE-UNKNOWN` rather than a hard parse error.
//!
//! See `docs/proposals/ir-error-messages-vocab.md` §2.C / §3.4.

use serde::{Deserialize, Serialize};

use super::super::Span;
use super::policy::TranslationKeyRefAst;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorsDecl {
    /// `default hide` | `default expose`. `None` defers to the runtime
    /// default (currently `Hide`). At most one entry per block.
    pub default: Option<ErrorExposureDefaultAst>,
    /// 4xx envelope-field exposure: `expose client 4xx <comma-list>`.
    /// Closed-catalog enforcement (allowed fields: `message`, `code`,
    /// `data`, `message_key`) lives on the analyzer / doctor side;
    /// parser keeps verbatim tokens. At most one line per block.
    pub exposure_4xx: Vec<String>,
    /// 5xx envelope-field exposure: `expose client 5xx <comma-list>`.
    /// Closed catalog (`code`, `data`) — `message` is intentionally
    /// excluded so 5xx stays framework-internal. At most one line.
    pub exposure_5xx: Vec<String>,
    /// `expose to @audience <name> <comma-list>` rows.
    pub audience_exposure: Vec<FeatureErrorExposeRuleDecl>,
    /// `error_redact <pattern>` rows. Pattern text is preserved
    /// verbatim except for surrounding quotes.
    pub redact_patterns: Vec<String>,
    /// `<code> message @translation.<key>` rows in source order.
    pub messages: Vec<FeatureErrorMessageDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorExposeRuleDecl {
    pub audience: Option<String>,
    pub fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorExposureDefaultAst {
    Hide,
    Expose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorMessageDecl {
    /// Verbatim error code identifier (e.g. `policy_denied`). Closed-
    /// catalog validation runs analyzer-side.
    pub code: String,
    /// The `@translation.<key>` reference.
    pub message: TranslationKeyRefAst,
    pub span: Span,
}
