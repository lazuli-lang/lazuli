//! Vocabulary lint rules (`VOCAB-*`).
//!
//! Each rule is a sub-module exposing a `check` function. Full dispatch
//! into `DoctorPackage::diagnostics()` is a separate cell (~+500 LOC of
//! IR loading + Finding → DoctorDiagnostic adapter). Until that ships,
//! each rule's inline `#[cfg(test)] mod tests` exercises the logic.
//!
//! v0.1 (4 rules): audit, derived_read, event_payload, union.
//! v0.2 catalog (3 rules): cap_missing, grammar_form, union_002.
//! v0.3 catalog (4 rules): event_orphan, event_producer, audit_002,
//! json_typed.
//! v0.4 catalog (2 rules): handler_heavy, tests_missing.
//! v0.5 catalog (1 rule): money_multi_currency.
//! v0.6 shared helpers: universal_columns (used by shadow-record /
//! resource-wide-cluster lints landing in subsequent cells).

pub mod conventions;
// VOCAB-CRUD-SYNTH-AVAILABLE-001 (spec 0002) — the `crud` synth run
// backwards: nudges a resource that hand-rolls the crud surface to adopt
// `conventions [crud]`. Advisory (warning, non-gating), suppressible.
pub mod crud_synth_available;
pub mod money_arithmetic_001;
pub mod money_compare_001;
pub mod owner_axis;
pub mod rate_limit;
pub mod universal_columns;
pub mod vocab_audit_001;
pub mod vocab_audit_002;
pub mod vocab_cap_missing_001;
// Iron-hand context vocabulary (v0.7). Three rules surface from a
// single meta-bundle preset; see
// `docs/canonical-semantics.md#feature-context-vocabulary` and
// `crates/lazuli_doctor/src/coverage/mod.rs:preset_severity_overrides`.
pub mod vocab_context_ctxmd_001;
pub mod vocab_context_nongoals_001;
// CUT 1b — the drift-killer. Fires when a feature's co-located
// `<feature>.ctx.md` prose SHADOWS (duplicates as a markdown table) what the
// IR already knows about a resource's fields. Doctrine-enforcement over the
// `<feature>.ctx.md` convention (no net-new vocab); sibling of
// `vocab_context_ctxmd_001` (which enforces the sidecar's PRESENCE).
pub mod vocab_context_prose_shadows_ir_001;
pub mod vocab_context_purpose_001;
pub mod vocab_derived_read_001;
pub mod vocab_event_orphan_001;
pub mod vocab_event_payload_001;
pub mod vocab_event_producer_001;
pub mod vocab_grammar_form_001;
pub mod vocab_handler_heavy_001;
pub mod vocab_json_typed_001;
// Knowledge-sector vocabulary (`## Preview vocab`). Five rules cross-check
// `knowledge <sector>` against the committed `knowledge/<sector>/` document
// vault (grammar↔file↔IR + write-gate + decay), mirroring the
// `VOCAB-CONTEXT-*` family that governs `purpose`/`non_goals`/`attach_ctx`.
// Shared disk/frontmatter/git scanner lives in `knowledge_vault`.
// See `docs/proposals/knowledge-sector-field.md` §Doctor.
pub mod knowledge_vault;
pub mod vocab_knowledge_dangling_cite_001;
pub mod vocab_knowledge_dup_topic_001;
pub mod vocab_knowledge_sector_unknown_001;
// Package-level (cross-feature) member of the family: fires when a
// `knowledge <sector>` slug is declared by exactly ONE feature across the
// whole package (a solo declaration is a smell — a sector is meant to be a
// SHARED 1:N corpus, so feature-private context likely belongs in
// `<feature>.ctx.md` instead). See docs/proposals/knowledge-sector-field.md.
pub mod vocab_knowledge_single_feature_001;
pub mod vocab_knowledge_stale_001;
pub mod vocab_knowledge_ungated_write_001;
// VOCAB-LIFECYCLE-001 — was deferred per the original proposal note
// "until the lifecycle vocabulary actually exists". Lifecycle IR has
// shipped (`crates/lazuli_ir/src/nodes/lifecycle.rs`), so the rule
// now activates. Wired into the dispatcher via `rule_bridges.rs` +
// `aggregators/vocab.rs` 2026-05-27.
pub mod vocab_lifecycle_001;
pub mod vocab_money_multi_currency_001;
pub mod vocab_resource_wide_cluster_001;
pub mod vocab_shadow_record_001;
pub mod vocab_tests_missing_001;
pub mod vocab_union_001;
pub mod vocab_union_002;
