//! Report IR — `report <name>` tabular export contract.
//!
//! A report declares **what tabular data leaves the system, in what format,
//! gated by which policy, at what filename**. The IR is a pure data shape:
//! it has no concept of byte-level CSV/XLSX writing, signed-URL signing, or
//! HTTP routing. Those concerns live downstream — `runtime/go/lazuli/report`
//! does the byte-level work, the codegen emitter wires the policy gate, and
//! the gateway resolves the signed URL.
//!
//! ## Format catalog
//!
//! [`ReportFormat`] is a **closed catalog** of two writer formats: CSV and
//! XLSX. JSON, Parquet, and PDF are explicitly out of scope for v0 — the
//! ecosystem already has battle-tested escape hatches for those formats
//! (custom API endpoints, dedicated reporting platforms) and lifting them
//! into the IR would broaden the wire contract without a pilot signal.
//!
//! ## Filename templating
//!
//! [`ReportFilenamePattern`] keeps the **literal source string** alongside
//! the **parsed token sequence**. The runtime resolves `tokens` against the
//! request context; the codegen emits the literal so adapter pattern engines
//! (e.g. Go's `text/template`) can re-tokenize without re-parsing IR. Token
//! catalog ([`FilenameToken`]) is closed — unrecognized tokens are caught by
//! `REPORT-FILENAME-TOKEN-UNKNOWN-001`.
//!
//! ## Signed visibility
//!
//! When [`Report::visibility`] is `Signed`, [`Report::signed_ttl`] is
//! required (duration literal — `1h`, `15m`, `30s`, `7d`). Doctor enforces.
//!
//! ## See also
//!
//! - `docs/proposals/report-vocab.md` v0.2 — full design
//! - `runtime/go/lazuli/report` — byte-level writer adapter
//! - [`crate::AuditSpec`] — canonical audit block shared with
//!   commands/queries/jobs/webhooks (v0.2 forbids `emit_to` on reports)

use serde::{Deserialize, Serialize};

use crate::{
    AuditSpec, FileVisibility, PolicyExpr, PolicyRef, QualifiedName, RateLimitSpec, SpanRef,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub name: String,
    pub source: ReportSource,
    pub columns: Vec<ReportColumn>,
    pub formats: Vec<ReportFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<QualifiedName>,
    pub visibility: FileVisibility,
    /// Duration literal (`1h`, `15m`, `30s`, `7d`) preserved verbatim.
    /// Required when `visibility == Signed`; doctor enforces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_ttl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<ReportFilenamePattern>,
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form (see `Command.policy_expr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// `rate_limit "<N per period per scope>"` with optional
    /// env-qualified overrides per `ir-rate-limit-env-aware` (cell 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
    /// Canonical audit block reused from commands/queries/jobs/webhooks.
    /// v0.2 forbids `emit_to` on reports; the doctor layer rejects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ReportSource {
    /// `source <feature>.query.<name>` or `query.<name>` (same feature).
    /// Doctor cross-checks the kind (`query.list` or `query.sql` only;
    /// `query.lookup` rejected by `REPORT-SOURCE-KIND-001`).
    Query(QualifiedName),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportColumn {
    pub name: String,
    pub source: ReportColumnSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Value format hint (`yyyy-mm-dd`, `currency:BRL`, ...). Closed
    /// catalog at runtime; the IR keeps the source verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed two-variant column source. The earlier `Constant(String)`
/// variant was rejected at architect review v0.2 (no pilot evidence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ReportColumnSource {
    /// `row.<field>` — project a field from the source query's record.
    RowField(String),
    /// `@fn.<name>(args)` — call a user-defined function. `args` is the
    /// verbatim argument list (comma-split, trimmed).
    Fn(FnInvocation),
}

/// `@fn.<name>(arg, arg, ...)` invocation site for a report column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FnInvocation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

/// Closed catalog of writer formats. JSON / Parquet / PDF are explicitly
/// out of scope for v0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Csv,
    Xlsx,
}

impl ReportFormat {
    /// Parse a token from `formats csv, xlsx`. Returns `None` outside
    /// the closed catalog — caller emits `REPORT-FORMAT-UNKNOWN-001`.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "csv" => Some(Self::Csv),
            "xlsx" => Some(Self::Xlsx),
            _ => None,
        }
    }

    /// Canonical lowercase token (`csv` / `xlsx`).
    pub fn token(&self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }
}

/// Lowered filename pattern. `literal` preserves the source string;
/// `tokens` is the parsed `{...}` placeholder sequence. The runtime
/// resolves `tokens` against `ctx`; the codegen emits the literal so
/// adapter pattern engines (e.g. `text/template`) can re-tokenize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFilenamePattern {
    pub literal: String,
    pub tokens: Vec<FilenameToken>,
}

/// Closed catalog of recognised filename pattern tokens. Anything
/// outside this set is `REPORT-FILENAME-TOKEN-UNKNOWN-001`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum FilenameToken {
    /// `{format}` — replaced with `csv` / `xlsx` at request time.
    Format,
    /// `{ctx.now:<strftime>}` — runtime formats the current time. The
    /// string holds the strftime suffix (`yyyymmdd`, `yyyymm`, etc.).
    /// Closed sub-catalog: `yyyy`, `mm`, `dd`, `HH`, `MM`, `ss`.
    CtxNowStrftime(String),
    /// `{ctx.user.id}` — opaque request user id.
    CtxUserId,
    /// `{ctx.tenant.id}` — opaque tenant id.
    CtxTenantId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_round_trip() {
        assert_eq!(ReportFormat::from_token("csv"), Some(ReportFormat::Csv));
        assert_eq!(ReportFormat::from_token("xlsx"), Some(ReportFormat::Xlsx));
        assert_eq!(ReportFormat::from_token("json"), None);
        assert_eq!(ReportFormat::from_token(""), None);
        assert_eq!(ReportFormat::Csv.token(), "csv");
        assert_eq!(ReportFormat::Xlsx.token(), "xlsx");
    }

    #[test]
    fn format_token_is_canonical_lowercase() {
        for fmt in [ReportFormat::Csv, ReportFormat::Xlsx] {
            assert_eq!(fmt.token(), fmt.token().to_lowercase());
        }
    }
}
