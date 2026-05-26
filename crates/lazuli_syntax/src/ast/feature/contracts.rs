//! Cross-feature contract annotations + `uses` clauses.
//!
//! Per `docs/proposals/cross-feature-contracts.md` §5.1, a
//! `public contract <Symbol> as v<N>` line sits IMMEDIATELY ABOVE the
//! declaration of `<Symbol>`. Captured during parse; the analyzer resolves
//! the version into the IR `PublicContract`.
//!
//! `uses` clauses (consumer-side version pins) live in this module too:
//! every cross-feature import authored at feature scope yields one
//! `UsesClauseAst` per imported feature.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// Cross-feature contract annotation per
/// `docs/proposals/cross-feature-contracts.md` §5.1. Appears as the
/// line `public contract <Symbol> as v<N>` IMMEDIATELY ABOVE the
/// declaration of `<Symbol>`. Captured during parse; the analyzer
/// resolves the version into the IR `PublicContract`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicContractDeclAst {
    /// Version number from `as v<N>`. Monotonic per symbol.
    pub version: u16,
    pub span: Span,
}

/// One `uses` clause: a cross-feature import with an optional version pin.
/// Authored at feature scope as `uses account` or `uses account version v1`.
/// Multiple comma-separated entries on one `uses` line yield multiple
/// `UsesClause` instances, each carrying its own optional pin.
///
/// Consumer-side pin per
/// `docs/proposals/cross-feature-contracts.md` §5.4 + the consumer-side-pin
/// follow-up. When `version` is `Some(N)`, the doctor
/// `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` rule checks each referenced
/// symbol's origin `public_contract.version` against `N`. When `None`,
/// the consumer floats with the origin's current version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsesClauseAst {
    pub feature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u16>,
    pub span: Span,
}
