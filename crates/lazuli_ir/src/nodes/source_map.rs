//! Source-map + symbol-origin sidecars and the cross-feature
//! `PublicContract` annotation.
//!
//! `SourceMap` resolves `SpanRef` byte offsets to `(file, line,
//! column)`. It's authored as a sidecar to `Module` per ADR-3 (not
//! embedded in `Module` to avoid cascading IR JSON size + snapshot
//! churn across the 30+ `SpanRef` use sites).
//!
//! `SymbolOriginIndex` resolves cross-feature symbol references —
//! the LSP / inspect consumers route through it instead of
//! re-walking every feature on every query. Built by
//! `lazuli_analyzer::build_symbol_origin_index`.
//!
//! `PublicContract` annotates a symbol that crosses feature
//! boundaries under `architecture mode microservices`; the
//! `version` field is monotonic per symbol and drives the
//! `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` diagnostic.

use serde::{Deserialize, Serialize};

use crate::{FileId, SpanRef};

/// SourceMap is the IR companion that resolves `SpanRef` byte
/// offsets to (file, line, column). Sidecar to `Module` — passed
/// alongside in codegen, serialized to `<module>.sourcemap.json`
/// when `--with-source` is requested. NOT embedded in `Module`
/// itself per ADR-3 (avoids cascading IR JSON size + snapshot
/// churn across 30+ SpanRef use sites).
///
/// EXPERIMENTAL: shape may grow additive fields before 1.0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

/// One file entry inside a [`SourceMap`]. Carries the stable id, the
/// canonical relative path (forward slashes), and the line-offset
/// table the IDE / diagnostic consumers use to translate byte spans
/// into `(line, column)` pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub id: FileId,
    /// Canonical relative path, e.g. `features/customer.lzi`.
    pub path: String,
    /// Byte offset of each line start, plus one EOF sentinel.
    pub line_offsets: Vec<u32>,
}

/// Sidecar to `Module`. Resolves cross-feature symbol references.
/// Built by `lazuli_analyzer::build_symbol_origin_index`.
/// EXPERIMENTAL: shape may grow additive fields before 1.0.
///
/// See `docs/proposals/lsp-symbol-origin.md` §6.2.
///
/// `symbols` keys are formatted `<feature>.<name>` (e.g. `account.Gender`)
/// so the index serializes to JSON without custom key adapters. The Rust
/// caller can recover a `QualifiedName` via `QualifiedName::parse_dotted`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolOriginIndex {
    pub symbols: std::collections::BTreeMap<String, SymbolOrigin>,
    pub imports: std::collections::BTreeMap<String, Vec<ImportEdge>>,
}

/// Resolved declaration site for one cross-feature symbol. Carries
/// the feature/name pair, the symbol kind, the source location, any
/// previous names (for rename tolerance), and the optional public
/// contract version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolOrigin {
    pub feature: String,
    pub name: String,
    pub kind: SymbolKind,
    pub defined_at: SourceLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// Cross-feature contract version per
    /// `docs/proposals/cross-feature-contracts.md` §5.1, populated by
    /// `lazuli_analyzer::build_symbol_origin_index` when the origin's
    /// declaration carries `public contract <Symbol> as v<N>`.
    /// `None` when no contract is declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_version: Option<u16>,
}

/// One import edge in the symbol-origin graph. Names the importing
/// symbol, the imported symbol, and the source location of the
/// import statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportEdge {
    pub importer: String,
    pub imported: String,
    pub uses_at: SourceLocation,
}

/// Closed catalog of symbol kinds tracked in the origin index. The
/// closed set lets doctor + LSP cross-check kind-mismatched references
/// without re-parsing every source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// `enum <Name>` declaration.
    Enum,
    /// `resource <Name>` declaration.
    Resource,
    /// `record <Name>` declaration.
    Record,
    /// Scalar alias (reserved; populated post-L0 #4 scalar aliases).
    Scalar,
    /// Semantic type from canonical catalog (Email/Phone/Url/Uuid/
    /// Currency/GeoPoint/Money + plugin BrazilianCPF/CNPJ/CEP).
    Semantic,
    /// `command <name>` declaration.
    Command,
    /// `query.<kind> <name>` declaration.
    Query,
    /// `event <name>` declaration.
    Event,
    /// `aggregate <Name>` declaration.
    Aggregate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_kind_round_trips() {
        let s = serde_json::to_string(&SymbolKind::Command).unwrap();
        assert_eq!(s, "\"command\"");
    }
}

/// Where a symbol is defined. Discriminated by `source`:
/// - `{ "source": "file", "file": "...", "line": N, "column": N }` for user-authored symbols
/// - `{ "source": "builtin" }` for compiler-provided types (Money, Email, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SourceLocation {
    File {
        file: String,
        line: u32,
        column: u32,
    },
    Builtin,
}

/// Cross-feature contract version annotation per
/// `docs/proposals/cross-feature-contracts.md` §5.1.
///
/// When a symbol is referenced from another feature under
/// `architecture mode microservices`, the origin feature MUST declare
/// this contract via `public contract <Symbol> as v<N>` adjacent to the
/// symbol's site. Doctor enforces.
///
/// Authored via the parser at `crates/lazuli_syntax/src/parser.rs`;
/// lowered by `lazuli_analyzer` from `PublicContractDeclAst`.
///
/// `version` is monotonic per symbol. `span_ref` anchors the
/// `public contract` source line for diagnostic origin reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicContract {
    pub version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}
