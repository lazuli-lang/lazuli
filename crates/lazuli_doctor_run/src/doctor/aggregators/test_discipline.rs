//! Test-discipline aggregator.
//!
//! Surfaces the eleven TDD/BDD-first rules that fire per feature in
//! `lazuli doctor`. Each rule lives in `lazuli_doctor::test_discipline`
//! (plus the `handler_missing_001` correctness rule that pairs with
//! `test_handler_missing_001`) and returns a typed `Finding`; this
//! aggregator turns each `Finding` into the canonical `DoctorDiagnostic`
//! envelope with severity resolved through the active
//! `[doctor.test_discipline] preset` and the security profile.
//!
//! Resolution order (per rule):
//! 1. Per-rule preset override (`tdd-iron-hand` flips every rule to
//!    Error; other presets either uniform-override or defer).
//! 2. Production profile bumps `HANDLER-MISSING-001` and
//!    `TEST-HANDLER-MISSING-001` from Warning to Error.
//! 3. Per-rule default Severity (`Error` for fixture/literal/drift/
//!    migration-uniqueness, `Warning` for restates/missing-authored/
//!    stub, `Info` for predicate-uncovered).
//!
//! `Prototype` short-circuits to an empty vec — TDD discipline is opt-in
//! at the prototype profile (you author the feature first, tests come
//! when you promote it).
//!
//! See `docs/proposals/tdd-bdd-first-2026-05-23.md` §3 (the eleven
//! TEST-* / MIGRATION-* / RUNTIME-* rules) for the closed catalog and
//! `docs/proposals/tdd-bdd-first-2026-05-23.md` §"Founding principle"
//! for the preset machinery this aggregator dispatches through.

use std::path::Path;

use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::doctor::{DoctorDiagnostic, DoctorSeverity};

/// Aggregate every test-discipline finding for one feature into the
/// canonical `DoctorDiagnostic` envelope. Returns an empty vec when the
/// active profile is `Prototype` (TDD discipline is opt-in there).
pub(crate) fn diagnostics(
    path: &Path,
    project_root: &Path,
    app_root: &Path,
    feature: &lazuli_ir::Feature,
    source: &str,
    security_profile: SecurityProfile,
    preset: Option<lazuli_doctor::test_discipline::preset::TestDisciplinePreset>,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::correctness::{handler_missing_001, predicate_eq_operator_001};
    use lazuli_doctor::test_discipline::preset::preset_rule_severity;
    use lazuli_doctor::test_discipline::{
        migration_dsl_unique_001, runtime_update_builder_jsonb_001,
        test_command_assertion_drift_001, test_fixture_literal_001, test_handler_missing_001,
        test_missing_authored_001, test_predicate_uncovered_001, test_restates_effect_001,
        test_restates_policy_001, test_stub_001,
    };
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    // W1.5 — resolve effective severity per rule: preset wins over the
    // per-rule default when it has an opinion. `tdd-iron-hand` returns
    // `Error` for every TEST-* / DOCTOR-* / MIGRATION-* / RUNTIME-* code;
    // other presets either uniform-override or defer (None).
    let resolve_severity = |default: DoctorSeverity, code: &str| -> DoctorSeverity {
        if let Some(preset) = preset {
            if let Some(override_sev) = preset_rule_severity(preset, code) {
                return override_sev.into();
            }
        }
        default
    };

    let mut out: Vec<DoctorDiagnostic> = Vec::new();

    for finding in test_missing_authored_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Warning,
                test_missing_authored_001::Finding::CODE,
            ),
            code: test_missing_authored_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    for finding in test_predicate_uncovered_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Info,
                test_predicate_uncovered_001::Finding::CODE,
            ),
            code: test_predicate_uncovered_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    for finding in test_restates_effect_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Warning,
                test_restates_effect_001::Finding::CODE,
            ),
            code: test_restates_effect_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    for finding in test_restates_policy_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Warning,
                test_restates_policy_001::Finding::CODE,
            ),
            code: test_restates_policy_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    for finding in test_fixture_literal_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Error,
                test_fixture_literal_001::Finding::CODE,
            ),
            code: test_fixture_literal_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    for finding in migration_dsl_unique_001::check(feature, path, project_root) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Error,
                migration_dsl_unique_001::Finding::CODE,
            ),
            code: migration_dsl_unique_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    for finding in runtime_update_builder_jsonb_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Warning,
                runtime_update_builder_jsonb_001::Finding::CODE,
            ),
            code: runtime_update_builder_jsonb_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    // Wave 3 — TEST-STUB-001: catches `@TODO authored:` markers in generated scaffolds.
    for finding in test_stub_001::check(source, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: finding.column,
            severity: resolve_severity(DoctorSeverity::Warning, test_stub_001::Finding::CODE),
            code: test_stub_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    // SPEC-05 — PREDICATE-EQ-OPERATOR-001: bare `=` used as equality in a
    // closed-predicate context (source scan; the analyzer drops such
    // predicates to Unparsed, so this restores the precise `=`→`==` fix-it).
    for finding in predicate_eq_operator_001::check(source, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Error,
                predicate_eq_operator_001::Finding::CODE,
            ),
            code: predicate_eq_operator_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    // Wave 4 + §7.1 — TEST-COMMAND-ASSERTION-DRIFT-001: catches the
    // leave_host_reply pattern (denies declared in tests block but handler
    // WHERE clause doesn't enforce it).
    for finding in test_command_assertion_drift_001::check(feature, path) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_severity(
                DoctorSeverity::Error,
                test_command_assertion_drift_001::Finding::CODE,
            ),
            code: test_command_assertion_drift_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    let _ = project_root;
    // Wave 5 — HANDLER-MISSING-001 (correctness category, fulfills CLAUDE.md:105
    // dormant 'Doctor enforces' promise). Walks @fn HandlerRef sites; errors
    // when <app_root>/features/<f>/handlers/<n>.go is missing.
    let production = matches!(security_profile, SecurityProfile::Production);
    for finding in handler_missing_001::check(feature, path, app_root) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: if production {
                DoctorSeverity::Error
            } else {
                DoctorSeverity::Warning
            },
            code: handler_missing_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    // Wave 5 — TEST-HANDLER-MISSING-001 (test_discipline category). Twin of
    // HANDLER-MISSING-001; fires only when .go exists but _test.go is missing
    // (avoids double-fire). v0.1 narrowed to @fn Command+lifecycle sites.
    for finding in test_handler_missing_001::check(feature, path, app_root) {
        let message = finding.message();
        out.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: if production {
                DoctorSeverity::Error
            } else {
                DoctorSeverity::Warning
            },
            code: test_handler_missing_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    out
}
