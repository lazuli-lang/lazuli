//! Semantic-type and cross-feature type aggregator.
//!
//! Combines two related families of cross-feature type diagnostics:
//!
//! * `semantic_type_unknown_diagnostics_for_*` — closed catalog
//!   `@semantic.<Name>` check. The catalog is DERIVED from the single
//!   source of truth `lazuli_keywords::SEMANTIC_TYPES` (via
//!   `lazuli_keywords::is_semantic_type`) so it can never drift away from
//!   the parser/analyzer/codegen catalog — adding a scalar there teaches
//!   every surface at once. Plugin-declared semantic
//!   aliases ride the `SEMANTIC-PLUGIN-001/002` rules in
//!   `aggregators::lazurite_manifest` instead. Two flavours: a
//!   pre-IR `syntax_feature::*` walk that runs against the
//!   `FeatureSkeleton` (catches parse-level type text) and an IR-level
//!   `feature::*` walk that runs against the lowered `Feature`.
//! * `cross_feature::*` — walks every `TypeRef::User` site,
//!   resolves owner via the `DoctorCrossFeatureTypeIndex` lookup
//!   built from per-feature records / enums / resources, and emits
//!   the `unresolved-type` / `uses-missing` diagnostics when the
//!   referenced type belongs to a feature the current one doesn't
//!   list under `uses`.
//!
//! Smaller helpers:
//!
//! * `shared::*` — the closed catalog constants, the
//!   `@semantic.<Name>` recognisers, and the line/column anchors
//!   shared by both syntax + IR walkers.
//! * `policy_ref_surface_text` — render a `PolicyRef` to the surface
//!   text the retired `scan_block_for_policy` walker captured. Lives
//!   here because `populate_feature_symbols_from_ir`
//!   (in `facts::feature_ir`) is the sole consumer; the function ties
//!   the IR-driven walker to the policy-string format that
//!   `agent_tool_policy_diagnostics` matches against.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9; further
//! sub-divided into `syntax_feature` / `feature` / `cross_feature` /
//! `shared` in rails-style R7-3.

mod cross_feature;
mod feature;
mod shared;
mod syntax_feature;

pub(crate) use cross_feature::{
    cross_feature_type_unresolved_diagnostics, feature_uses_missing_diagnostics, span_line,
};
pub(crate) use feature::semantic_type_unknown_diagnostics_for_feature;
use lazuli_ir as ir;
pub(crate) use shared::SEMANTIC_TYPE_UNKNOWN_CODE;
pub(crate) use syntax_feature::semantic_type_unknown_diagnostics_for_syntax_feature;

/// Render a `PolicyRef` into the same surface text the retired
/// `scan_block_for_policy` walker captured verbatim from `policy <text>`
/// child lines. `PolicyRef::None` returns `None` so the IR-driven
/// populator skips empty entries the way the walker skipped missing
/// `policy` clauses.
pub(crate) fn policy_ref_surface_text(p: &ir::PolicyRef) -> Option<String> {
    match p {
        ir::PolicyRef::Local(name) => Some(format!("@policy.{name}")),
        ir::PolicyRef::Atom(atom) => Some(atom.clone()),
        ir::PolicyRef::External { feature, name } => Some(format!("{feature}.{name}")),
        ir::PolicyRef::Unresolved(text) => Some(text.clone()),
        ir::PolicyRef::None => None,
    }
}
