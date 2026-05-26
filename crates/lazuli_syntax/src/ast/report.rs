//! `report <name>` kind AST — tabular export contract.
//!
//! Lazuli's declarative answer to the `api + opaque handler` pattern
//! for static-column exports. Authoring a CSV/XLSX export now means
//! naming columns, choosing a source query, picking formats — no Go
//! file required for the canonical case. See
//! `docs/proposals/report-vocab.md` v0.2.
//!
//! Surface (Surface B — v0.2 ships only this one):
//!
//! ```text
//! report <name>
//!   source <qualified_query_ref>
//!   columns
//!     <col> from row.<field> | @fn.<name>(args) [label "..."] [format "..."]
//!   formats csv, xlsx
//!   storage <ref>          (optional)
//!   visibility signed|public|private
//!   signed_ttl <duration>
//!   filename "..."
//!   policy @policy.<name>
//!   rate_limit "..."
//!   audit ...
//! ```
//!
//! `source`, `columns`, `formats`, `policy` are required; the rest are
//! conditional (e.g. `signed_ttl` only when `visibility=signed`).
//! `policy_expr` is the RB.S6 structured form that supersedes the
//! string-based `policy` field at lowering time.

use serde::{Deserialize, Serialize};

use super::{CommandAudit, PolicyExprAst, RateLimitSpecAst, Span};

/// `report <name>` block — declarative tabular export.
///
/// Lazuli's answer to the `api + opaque handler` pattern for
/// static-column exports: authors name columns, pick a source query and
/// formats, and the codegen emits the export pipeline. No Go file
/// required for the canonical case. See module-level docs for the
/// surface and `docs/proposals/report-vocab.md` for the rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDecl {
    pub name: String,
    /// `source <qualified_query_ref>` — required. Captured verbatim
    /// (`customer.query.list`, `query.list`). Analyzer resolves.
    pub source: String,
    /// `columns` block. Must contain at least one entry; doctor enforces
    /// via `REPORT-COLUMNS-EMPTY-001`.
    pub columns: Vec<ReportColumnAst>,
    /// `formats csv, xlsx` — closed catalog enforced at lowering /
    /// doctor via `REPORT-FORMAT-UNKNOWN-001`.
    pub formats: Vec<String>,
    /// `storage <capability_ref>` — optional. When omitted and the
    /// package declares exactly one `object_storage` capability, the
    /// analyzer binds implicitly. Otherwise `REPORT-STORAGE-AMBIGUOUS-001`.
    pub storage: Option<String>,
    /// `visibility signed|public|private` — defaults to `signed` at
    /// lowering. Closed catalog.
    pub visibility: Option<String>,
    /// `signed_ttl 1h` — duration literal preserved as text. Required
    /// when `visibility=signed`; rejected otherwise.
    pub signed_ttl: Option<String>,
    /// `filename "..."` — template string. Tokens validated by the
    /// analyzer via `REPORT-FILENAME-TOKEN-UNKNOWN-001`.
    pub filename: Option<String>,
    /// `policy @policy.<name>` — required.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `rate_limit "..."` — required when policy includes `@scope.public`.
    /// Env-aware per `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    /// `audit <subjects>` canonical block (see `CommandAudit`).
    pub audit: Option<CommandAudit>,
    pub span: Span,
}

/// One column in a `report.columns` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportColumnAst {
    pub name: String,
    /// `from row.<field>` or `from @fn.<name>(args)`.
    pub source: ReportColumnSourceAst,
    /// `label "..."` — optional human label.
    pub label: Option<String>,
    /// `format "..."` — optional value format hint (`yyyy-mm-dd`,
    /// `currency:BRL`, etc.). v0 catalog is documented; the parser
    /// captures verbatim.
    pub format: Option<String>,
    pub span: Span,
}

/// Column-source grammar — proposal v0.2 closes this to two variants.
/// `Constant(String)` was rejected (no pilot evidence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ReportColumnSourceAst {
    /// `row.<field>` — project a field from the source query record.
    RowField(String),
    /// `@fn.<name>(arg, arg, ...)` — call a user-defined or capability
    /// function. Args are captured verbatim (comma-split, trimmed).
    FnCall { name: String, args: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_field_serde_token_is_kind_tagged() {
        let v = serde_json::to_value(ReportColumnSourceAst::RowField("name".into())).unwrap();
        assert_eq!(v["kind"], "RowField");
        assert_eq!(v["value"], "name");
    }

    #[test]
    fn fn_call_serde_carries_args() {
        let s = ReportColumnSourceAst::FnCall {
            name: "currency".into(),
            args: vec!["row.total".into(), "BRL".into()],
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["kind"], "FnCall");
        assert_eq!(v["value"]["args"][1], "BRL");
    }
}
