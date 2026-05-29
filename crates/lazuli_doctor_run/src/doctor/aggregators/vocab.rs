//! Vocabulary-fitness aggregator.
//!
//! Surfaces the `VOCAB-*` rule catalog that ships under
//! `lazuli_doctor::vocab` but whose rule modules had no dispatcher entry
//! point. The proposal in `docs/proposals/doctor-vocabulary-lints.md`
//! §"Implementation status (post-wave)" documents this as the deferred
//! follow-up cell (~+500 LOC of IR loading + Finding → DoctorDiagnostic
//! adapter); this aggregator closes that gap.
//!
//! ## Scope
//!
//! Wires the 15 vocab rules that read a per-feature `&Feature` view
//! (plus, for two cluster/shadow rules, a synthetic `&Module`). The
//! already-wired rules are deliberately left in place:
//!
//! - `vocab_grammar_form_001` runs from `dispatch::diagnostics` over
//!   `DoctorFile` (text-walker, not IR-driven).
//! - `vocab_tests_missing_001`, `money_compare_001`, `money_arithmetic_001`
//!   run per-feature inside `DoctorPackage::load`'s `.lzi` loop.
//! - `vocab_context_purpose_001`, `vocab_context_nongoals_001`,
//!   `vocab_context_ctxmd_001` dispatch through
//!   `package_methods::context_vocab_diagnostics`, which threads the
//!   iron-hand coverage-preset machinery the context family needs.
//!
//! This aggregator dispatches the remaining 15 rules from the
//! single-pass Tier 3 walk so the dispatcher stays uniform and the rule
//! modules don't need bespoke per-rule wiring sites.
//!
//! ## Severity policy
//!
//! Every rule defers to `doctor_severity_for` with
//! `RuleCategory::Vocabulary`. The default mapping is `Prototype` ->
//! suppressed (vocabulary refactors are opt-in at prototype), `Strict`
//! -> Warning, `Production` -> Error. Per-rule overrides via
//! `[doctor.vocab].severity_override` are honored once the manifest
//! schema gains that block; callers pass an empty override map today,
//! matching the existing wired vocab rules.
//!
//! ## Synthetic Module note
//!
//! `vocab_resource_wide_cluster_001` and `vocab_shadow_record_001` need
//! a `&lazuli_ir::Module` to resolve FK types via
//! `is_universal_column`. We synthesize a single-feature module from
//! the current Tier 3 fact bundle. Cross-feature FK resolution falls
//! back to "unresolved", which is the correct conservative default —
//! the rule under-fires rather than over-fires on cross-feature
//! references.
//!
//! See `docs/proposals/doctor-vocabulary-lints.md` for the per-rule
//! catalog and detection heuristics.

use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use super::correctness::make_synthetic_feature_for_correctness;
use crate::doctor::{
    DoctorDiagnostic, Tier3FeatureFacts, vocab_audit_001_diagnostics, vocab_audit_002_diagnostics,
    vocab_derived_read_001_diagnostics, vocab_event_orphan_001_diagnostics,
    vocab_event_payload_001_diagnostics, vocab_event_producer_001_diagnostics,
    vocab_handler_heavy_001_diagnostics, vocab_json_typed_001_diagnostics,
    vocab_lifecycle_001_diagnostics, vocab_money_multi_currency_001_diagnostics,
    vocab_resource_wide_cluster_001_diagnostics, vocab_shadow_record_001_diagnostics,
    vocab_union_001_diagnostics, vocab_union_002_diagnostics,
};

/// Aggregate every IR-driven `VOCAB-*` finding across the package's
/// Tier 3 facts into the canonical `DoctorDiagnostic` envelope.
///
/// The text-derived `VOCAB-CAP-MISSING-001` rule lives in
/// [`cap_missing_diagnostics`] below because it reads raw `.lzi`
/// source rather than the lifted IR.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    let mut out: Vec<DoctorDiagnostic> = Vec::new();
    for fact in facts {
        let feature = make_synthetic_feature_for_correctness(fact);
        let module = synth_module(feature.clone());
        let header_line = fact.feature_line;
        let path = &fact.path;

        out.extend(vocab_audit_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_audit_002_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_derived_read_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_event_orphan_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_event_payload_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_event_producer_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_handler_heavy_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_json_typed_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        // VOCAB-LIFECYCLE-001 — lifecycle IR shipped in v0.2 and the
        // rule module is now published (`vocab/mod.rs:45`). Wired
        // 2026-05-27 per
        // `docs/proposals/lifecycle-vocab-architect-audit-2026-05-27.md`
        // §"Cell A".
        out.extend(vocab_lifecycle_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_money_multi_currency_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_resource_wide_cluster_001_diagnostics(
            path,
            &feature,
            &module,
            header_line,
            security_profile,
        ));
        out.extend(vocab_shadow_record_001_diagnostics(
            path,
            &feature,
            &module,
            header_line,
            security_profile,
        ));
        out.extend(vocab_union_001_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
        out.extend(vocab_union_002_diagnostics(
            path,
            &feature,
            header_line,
            security_profile,
        ));
    }
    out
}

/// VOCAB-CAP-MISSING-001 source-walker — reads raw `.lzi` text per
/// loaded file (the IR currently drops trailing `@pii.*` decorators).
/// Wired separately because the rule takes `DoctorFile` rather than
/// `Tier3FeatureFacts`.
pub(crate) fn cap_missing_diagnostics(
    files: &[crate::doctor::DoctorFile],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    use crate::doctor::parsers::is_lzi_path;
    use crate::doctor::vocab_cap_missing_001_diagnostics;

    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    files
        .iter()
        .filter(|file| is_lzi_path(&file.path))
        .flat_map(|file| {
            vocab_cap_missing_001_diagnostics(&file.path, &file.source, 1, security_profile)
        })
        .collect()
}

/// Build a minimal `Module` containing just the synthesized feature.
///
/// The two cross-feature-aware vocab rules
/// (`vocab_resource_wide_cluster_001` / `vocab_shadow_record_001`) walk
/// `Module::features` while resolving FK type references through
/// `is_universal_column`. A single-feature module is enough for the
/// intra-feature case (the common one) and degrades cross-feature FK
/// resolution to "unresolved", which the rule treats as "not
/// universal" -> conservative under-fire. Once doctor lifts a true
/// multi-feature module fact bundle this synthesis can lift its head
/// out without touching the bridges.
fn synth_module(feature: lazuli_ir::Feature) -> lazuli_ir::Module {
    lazuli_ir::Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature],
    }
}
