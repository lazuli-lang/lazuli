//! Anti-drift severity-parity proof (Wave D4).
//!
//! Proves the SINGLE-SOURCE property mechanically: the LSP and the CLI
//! `lazuli doctor` resolve **identical** severities because both reduce to
//! the one shared resolver in `lazuli_doctor_config`
//! (`effective_severity` / `effective_severity_over_base`). If a future
//! change hardcodes a severity on either side instead of routing through
//! the shared resolver, the concrete per-cell assertions below fail the
//! build.
//!
//! ## The ownership partition decides WHICH resolver a code uses
//!
//! D3 split every doctor finding into two buckets (`is_lsp_owned` /
//! `is_doctor_owned`, total + disjoint). In-editor, the bucket decides the
//! severity path — and crucially BOTH paths reduce to the same
//! `lazuli_doctor_config` function as the CLI:
//!
//! - **Doctor-owned** (SCREAMING-KEBAB rule-catalog codes — `HOOK-*`,
//!   `VOCAB-*`, `TEST-*`, …): published in-editor by the package ENGINE
//!   (`doctor_engine::doctor_owned_for_document` → `run_package`). The
//!   engine resolves their severity with `effective_severity` — the *exact*
//!   call the CLI's `doctor_severity_for` /
//!   `package_methods::context_vocab_diagnostics` make
//!   (`crates/lazuli_doctor_run/src/doctor/diagnostic.rs:225`,
//!   `.../package_methods.rs:87-96`). Same function ⇒ identical severity;
//!   only the final `DoctorSeverity → DiagnosticSeverity` map is applied,
//!   and Property 0 pins that map equal to the CLI's inverse.
//!
//! - **LSP-owned** (file-local kebab "contract/shape" codes —
//!   `env-schema-contract`, …): published in-editor by the synchronous
//!   file-local pass via the REAL
//!   `doctor_local::doctor_class_lsp_severity` =
//!   `effective_severity_over_base(code, base, from_code_prefix(code),
//!   cfg).map(lsp_severity)`
//!   (`crates/lazuli_lsp/src/diagnostics/doctor_local/mod.rs:63-77`). The
//!   CLI emits these same file-local codes at the same intrinsic base, so
//!   `effective_severity_over_base` is the matching CLI computation.
//!
//! Either way the answer comes from `lazuli_doctor_config`; this test calls
//! the **real** LSP-side function (`lazuli_lsp::test_surface`) and the
//! **real** shared resolver, then asserts concrete per-cell severities so a
//! one-sided hardcode is caught.

use lazuli_doctor_config::{
    DoctorProfile, DoctorSeverity, ResolvedDoctorConfig, RuleCategory, SeverityOverride,
    effective_severity, effective_severity_over_base,
};
use lazuli_lsp::test_surface::{
    doctor_class_lsp_severity, is_doctor_owned, is_lsp_owned, lsp_severity,
};
use tower_lsp::lsp_types::DiagnosticSeverity;

/// `DoctorSeverity -> DiagnosticSeverity`, the inverse of the CLI's
/// `from_lsp`. Property 0 asserts this matches the REAL LSP map
/// (`lazuli_lsp::test_surface::lsp_severity`) for every variant, so the
/// cross-path comparison is apples-to-apples in `DiagnosticSeverity` units.
fn doctor_to_lsp_severity(severity: DoctorSeverity) -> DiagnosticSeverity {
    match severity {
        DoctorSeverity::Error => DiagnosticSeverity::ERROR,
        DoctorSeverity::Warning => DiagnosticSeverity::WARNING,
        DoctorSeverity::Info => DiagnosticSeverity::INFORMATION,
        DoctorSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

// ---------------------------------------------------------------------------
// CLI / engine resolution — the public `effective_severity` both the CLI's
// `doctor_severity_for` and the package engine call.
// ---------------------------------------------------------------------------

/// Severity `lazuli doctor` (CLI) emits for a doctor-owned `code` under
/// `cfg`, mapped to editor units. Verbatim mirror of
/// `doctor_severity_for` (diagnostic.rs:225) /
/// `context_vocab_diagnostics` (package_methods.rs:87) — both call
/// `effective_severity` with the prefix-derived category and the
/// unreachable `Warning` level-4 base.
fn cli_doctor_owned(code: &str, cfg: &ResolvedDoctorConfig) -> Option<DiagnosticSeverity> {
    let category = RuleCategory::from_code_prefix(code);
    effective_severity(code, DoctorSeverity::Warning, category, cfg).map(doctor_to_lsp_severity)
}

/// Severity the in-editor package ENGINE publishes for a doctor-owned
/// `code` — `effective_severity` (resolved by the aggregator) then mapped
/// via the REAL `lazuli_lsp` `lsp_severity`. Identical inputs to
/// `cli_doctor_owned`, so equality is the single-source property.
fn lsp_doctor_owned(code: &str, cfg: &ResolvedDoctorConfig) -> Option<DiagnosticSeverity> {
    let category = RuleCategory::from_code_prefix(code);
    effective_severity(code, DoctorSeverity::Warning, category, cfg).map(lsp_severity)
}

/// Severity `lazuli doctor` (CLI) emits for an LSP-owned (file-local)
/// `code` at intrinsic `base` — the aggregator emits at a hardcoded base
/// that levels 1-3 may move, i.e. `effective_severity_over_base`.
fn cli_lsp_owned(
    code: &str,
    base: DiagnosticSeverity,
    cfg: &ResolvedDoctorConfig,
) -> Option<DiagnosticSeverity> {
    let doc_base = lsp_to_doctor_severity(base);
    effective_severity_over_base(code, doc_base, RuleCategory::from_code_prefix(code), cfg)
        .map(doctor_to_lsp_severity)
}

/// Severity the in-editor synchronous file-local pass publishes for an
/// LSP-owned `code` — the REAL `doctor_class_lsp_severity`.
fn lsp_lsp_owned(
    code: &str,
    base: DiagnosticSeverity,
    cfg: &ResolvedDoctorConfig,
) -> Option<DiagnosticSeverity> {
    doctor_class_lsp_severity(code, base, cfg)
}

fn lsp_to_doctor_severity(base: DiagnosticSeverity) -> DoctorSeverity {
    match base {
        DiagnosticSeverity::ERROR => DoctorSeverity::Error,
        DiagnosticSeverity::WARNING => DoctorSeverity::Warning,
        DiagnosticSeverity::INFORMATION => DoctorSeverity::Info,
        DiagnosticSeverity::HINT => DoctorSeverity::Hint,
        _ => DoctorSeverity::Warning,
    }
}

// ---------------------------------------------------------------------------
// Matrix axes.
// ---------------------------------------------------------------------------

const PROFILES: [DoctorProfile; 3] = [
    DoctorProfile::Prototype,
    DoctorProfile::Strict,
    DoctorProfile::Production,
];

/// Coverage presets: none + the four named presets {tdd-strict,
/// tdd-mature, tdd-iron-hand, off}. The `str` is the `[doctor.coverage]
/// preset` TOML value.
const COVERAGE_PRESETS: [Option<&str>; 5] = [
    None,
    Some("tdd-strict"),
    Some("tdd-mature"),
    Some("tdd-iron-hand"),
    Some("off"),
];

/// A sampled doctor-owned (SCREAMING-KEBAB) code, one per category family
/// the resolver special-cases, plus the VOCAB-CONTEXT trio.
struct DoctorOwnedSample {
    code: &'static str,
    expect_category: RuleCategory,
}

fn doctor_owned_samples() -> Vec<DoctorOwnedSample> {
    vec![
        // Correctness, ERROR-base in the catalog.
        DoctorOwnedSample {
            code: "HOOK-TARGET-001",
            expect_category: RuleCategory::Correctness,
        },
        // Vocab, non-context (governed by no coverage-preset escalation).
        DoctorOwnedSample {
            code: "VOCAB-TESTS-MISSING-001",
            expect_category: RuleCategory::Vocabulary,
        },
        // TestDiscipline (escalated by `[doctor.test_discipline] preset`,
        // not the coverage preset).
        DoctorOwnedSample {
            code: "TEST-MISSING-AUTHORED-001",
            expect_category: RuleCategory::TestDiscipline,
        },
        // VOCAB-CONTEXT trio — the only family the coverage preset map
        // governs (iron-hand escalates; off suppresses).
        DoctorOwnedSample {
            code: "VOCAB-CONTEXT-PURPOSE-001",
            expect_category: RuleCategory::Vocabulary,
        },
        DoctorOwnedSample {
            code: "VOCAB-CONTEXT-NONGOALS-001",
            expect_category: RuleCategory::Vocabulary,
        },
        DoctorOwnedSample {
            code: "VOCAB-CONTEXT-CTXMD-001",
            expect_category: RuleCategory::Vocabulary,
        },
    ]
}

/// LSP-owned (file-local, kebab contract) codes + the intrinsic base the
/// aggregator emits. These flow through `effective_severity_over_base`.
const LSP_OWNED_SAMPLES: [(&str, DiagnosticSeverity); 3] = [
    ("env-schema-contract", DiagnosticSeverity::ERROR),
    ("app-env-contract", DiagnosticSeverity::ERROR),
    ("auth-password-no-session", DiagnosticSeverity::WARNING),
];

fn config_for(profile: DoctorProfile, coverage: Option<&str>) -> ResolvedDoctorConfig {
    match coverage {
        None => ResolvedDoctorConfig::resolve(None, profile).unwrap(),
        Some(preset) => {
            let toml = format!("[doctor.coverage]\npreset = \"{preset}\"\n");
            ResolvedDoctorConfig::resolve(Some(&toml), profile).unwrap()
        }
    }
}

// ===========================================================================
// Property 0 — the LSP severity map IS the inverse the CLI uses.
//
// Anchors the whole proof: the editor map (the REAL
// `lazuli_lsp::test_surface::lsp_severity`) and the map the CLI mirror
// applies must agree for every variant, or "equal `DiagnosticSeverity`"
// would be meaningless.
// ===========================================================================

#[test]
fn lsp_severity_map_is_cli_inverse_for_every_variant() {
    for sev in [
        DoctorSeverity::Error,
        DoctorSeverity::Warning,
        DoctorSeverity::Info,
        DoctorSeverity::Hint,
    ] {
        assert_eq!(
            lsp_severity(sev),
            doctor_to_lsp_severity(sev),
            "the real LSP severity map diverged from the CLI inverse for {sev:?}",
        );
    }
}

// ===========================================================================
// Property 1 — resolver identity across the matrix (doctor-owned codes).
//
// codes × profiles × coverage presets: in-editor (engine) severity ==
// CLI severity, with the concrete expected value asserted per cell so a
// future hardcode on either side is caught.
// ===========================================================================

#[test]
fn lsp_equals_cli_for_doctor_owned_across_profile_x_coverage() {
    for sample in doctor_owned_samples() {
        // Keep the sample honest about its family AND its ownership.
        assert_eq!(
            RuleCategory::from_code_prefix(sample.code),
            sample.expect_category,
            "{}: from_code_prefix drifted from the sampled category",
            sample.code,
        );
        assert!(
            is_doctor_owned(sample.code),
            "{}: sampled as doctor-owned but the partition disagrees",
            sample.code,
        );

        for profile in PROFILES {
            for coverage in COVERAGE_PRESETS {
                let cfg = config_for(profile, coverage);
                let lsp = lsp_doctor_owned(sample.code, &cfg);
                let cli = cli_doctor_owned(sample.code, &cfg);

                // CORE single-source assertion: both reduce to the same
                // `effective_severity`, so they agree per cell.
                assert_eq!(
                    lsp, cli,
                    "DIVERGENCE: code={} profile={:?} coverage={:?} -> lsp={:?} cli={:?}",
                    sample.code, profile, coverage, lsp, cli,
                );

                // Concrete value lock — the load-bearing anti-hardcode
                // anchor. `None` = "rely on lsp==cli + resolver crate tests".
                if let Some(expected) = expected_doctor_owned_cell(sample.code, profile, coverage) {
                    assert_eq!(
                        cli, expected,
                        "cell value drift: code={} profile={:?} coverage={:?}",
                        sample.code, profile, coverage,
                    );
                }
            }
        }
    }
}

/// Hand-derived expected severity for the deterministic doctor-owned cells.
/// `Some(Some(sev))` pins a literal; `Some(None)` pins SUPPRESSION; an outer
/// `None` means "don't pin a literal here".
fn expected_doctor_owned_cell(
    code: &str,
    profile: DoctorProfile,
    coverage: Option<&str>,
) -> Option<Option<DiagnosticSeverity>> {
    use DiagnosticSeverity as DS;
    let vocab_default = match profile {
        // IronHand mirrors Production for category defaults (doctor config
        // lib.rs: `(_, Production | IronHand) => Error`).
        DoctorProfile::Production | DoctorProfile::IronHand => DS::ERROR,
        DoctorProfile::Prototype | DoctorProfile::Strict => DS::WARNING,
    };
    match code {
        // VOCAB-CONTEXT trio: iron-hand escalates to ERROR (level 2); off
        // SUPPRESSES (level 2 -> None); otherwise the per-profile vocab
        // category default (level 4).
        "VOCAB-CONTEXT-PURPOSE-001" | "VOCAB-CONTEXT-NONGOALS-001" | "VOCAB-CONTEXT-CTXMD-001" => {
            Some(match coverage {
                Some("tdd-iron-hand") => Some(DS::ERROR),
                Some("off") => None,
                _ => Some(vocab_default),
            })
        }
        // Plain vocab code: per-profile vocab default at every coverage
        // preset (no coverage escalation governs it; `off` is family-scoped
        // to the VOCAB-CONTEXT trio, so this code still surfaces).
        "VOCAB-TESTS-MISSING-001" => Some(Some(vocab_default)),
        // Correctness code via the CLI (`effective_severity`) gets the
        // per-profile category default (WARNING at proto/strict, ERROR at
        // production) — the coverage preset does not govern it.
        "HOOK-TARGET-001" => Some(Some(match profile {
            DoctorProfile::Production | DoctorProfile::IronHand => DS::ERROR,
            DoctorProfile::Prototype | DoctorProfile::Strict => DS::WARNING,
        })),
        // TestDiscipline default per profile: Info@proto, Warning@strict,
        // Error@production (its own posture). Coverage preset does not
        // govern it; `[doctor.test_discipline] preset` would (tested in
        // Property 3's sibling below), but here only the coverage axis varies.
        "TEST-MISSING-AUTHORED-001" => Some(Some(match profile {
            DoctorProfile::Prototype => DS::INFORMATION,
            DoctorProfile::Strict => DS::WARNING,
            DoctorProfile::Production | DoctorProfile::IronHand => DS::ERROR,
        })),
        _ => None,
    }
}

// ===========================================================================
// Property 1b — resolver identity for LSP-owned (file-local) codes.
//
// These flow through the REAL `doctor_class_lsp_severity`
// (`effective_severity_over_base`). The CLI emits them at the same
// intrinsic base, so the two agree across the matrix.
// ===========================================================================

#[test]
fn lsp_equals_cli_for_lsp_owned_across_profile_x_coverage() {
    for (code, base) in LSP_OWNED_SAMPLES {
        assert!(
            is_lsp_owned(code),
            "{code}: sampled as lsp-owned but the partition disagrees",
        );
        for profile in PROFILES {
            for coverage in COVERAGE_PRESETS {
                let cfg = config_for(profile, coverage);
                let lsp = lsp_lsp_owned(code, base, &cfg);
                let cli = cli_lsp_owned(code, base, &cfg);
                assert_eq!(
                    lsp, cli,
                    "DIVERGENCE (lsp-owned): code={code} profile={profile:?} \
                     coverage={coverage:?} -> lsp={lsp:?} cli={cli:?}",
                );
                // These kebab codes are governed by no preset / override, so
                // the intrinsic base is the floor at every cell.
                assert_eq!(
                    lsp,
                    Some(base),
                    "lsp-owned base floor drift: code={code} profile={profile:?} \
                     coverage={coverage:?}",
                );
            }
        }
    }
}

// ===========================================================================
// Property 2 — silent-under-preset: the VOCAB-CONTEXT trio under the `off`
// coverage preset resolves to `None`; the LSP emits NO diagnostic and the
// CLI emits none. Both sides agree on suppression.
// ===========================================================================

#[test]
fn vocab_context_silent_under_off_on_both_sides() {
    let cfg = config_for(DoctorProfile::Strict, Some("off"));
    for code in [
        "VOCAB-CONTEXT-PURPOSE-001",
        "VOCAB-CONTEXT-NONGOALS-001",
        "VOCAB-CONTEXT-CTXMD-001",
    ] {
        let lsp = lsp_doctor_owned(code, &cfg);
        let cli = cli_doctor_owned(code, &cfg);
        assert_eq!(lsp, None, "{code}: LSP should publish nothing under off");
        assert_eq!(cli, None, "{code}: CLI should emit nothing under off");
        assert_eq!(lsp, cli, "{code}: both sides must agree on suppression");
    }
}

#[test]
fn non_governed_code_not_suppressed_under_off() {
    // A vocab code NOT in the coverage-governed trio still resolves normally
    // under `off` on both sides (the `off` short-circuit is family-scoped).
    let cfg = config_for(DoctorProfile::Strict, Some("off"));
    let lsp = lsp_doctor_owned("VOCAB-TESTS-MISSING-001", &cfg);
    let cli = cli_doctor_owned("VOCAB-TESTS-MISSING-001", &cfg);
    assert!(
        lsp.is_some(),
        "non-governed code must still surface in editor"
    );
    assert!(cli.is_some(), "non-governed code must still surface in CLI");
    assert_eq!(lsp, cli);
}

// ===========================================================================
// Property 3 — override precedence: a `[doctor.…].severity_override` moves a
// code's severity; BOTH the LSP and the CLI reflect the moved severity
// (override wins over preset / profile default).
// ===========================================================================

#[test]
fn manifest_override_wins_on_both_sides() {
    // iron-hand would escalate VOCAB-CONTEXT-CTXMD-001 to ERROR; the
    // override pulls it back down to WARNING. Both sides must honor it.
    let toml = r#"
[doctor.test_discipline.severity_override."VOCAB-CONTEXT-CTXMD-001"]
severity = "warning"
reason = "backfill scheduled"

[doctor.coverage]
preset = "tdd-iron-hand"
"#;
    let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
    let lsp = lsp_doctor_owned("VOCAB-CONTEXT-CTXMD-001", &cfg);
    let cli = cli_doctor_owned("VOCAB-CONTEXT-CTXMD-001", &cfg);
    assert_eq!(
        cli,
        Some(DiagnosticSeverity::WARNING),
        "override must beat the iron-hand escalation on the CLI side",
    );
    assert_eq!(
        lsp,
        Some(DiagnosticSeverity::WARNING),
        "override must beat the iron-hand escalation on the LSP side",
    );
    assert_eq!(lsp, cli, "override resolution must agree across both paths");

    // An UPWARD override (warning-base code forced to error) also moves both
    // sides — proving precedence, not a coincidental floor.
    let up = r#"
[doctor.test_discipline.severity_override."VOCAB-CONTEXT-PURPOSE-001"]
severity = "error"
reason = "promoted for this project"
"#;
    let cfg_up = ResolvedDoctorConfig::resolve(Some(up), DoctorProfile::Strict).unwrap();
    let lsp_up = lsp_doctor_owned("VOCAB-CONTEXT-PURPOSE-001", &cfg_up);
    let cli_up = cli_doctor_owned("VOCAB-CONTEXT-PURPOSE-001", &cfg_up);
    assert_eq!(cli_up, Some(DiagnosticSeverity::ERROR));
    assert_eq!(lsp_up, Some(DiagnosticSeverity::ERROR));
    assert_eq!(lsp_up, cli_up);
}

#[test]
fn override_constructed_directly_moves_lsp_owned_code() {
    // Same proof on an LSP-owned (file-local) code via the REAL
    // `doctor_class_lsp_severity`: inject the override straight into the
    // resolved config (the shape `doctor_severity_for` builds).
    let mut cfg = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
    cfg.overrides.insert(
        "env-schema-contract".to_owned(),
        SeverityOverride {
            severity: "hint".to_owned(),
            reason: Some("downgraded for this milestone".to_owned()),
        },
    );
    let lsp = lsp_lsp_owned("env-schema-contract", DiagnosticSeverity::ERROR, &cfg);
    let cli = cli_lsp_owned("env-schema-contract", DiagnosticSeverity::ERROR, &cfg);
    assert_eq!(cli, Some(DiagnosticSeverity::HINT));
    assert_eq!(lsp, Some(DiagnosticSeverity::HINT));
    assert_eq!(lsp, cli);
}

// ===========================================================================
// Property 4 — partition sanity (references D3's predicates; does not
// re-derive them). Over the sampled codes the ownership partition is total +
// disjoint, so no code is resolved by BOTH paths with a conflicting severity.
// ===========================================================================

#[test]
fn ownership_partition_total_and_disjoint_over_sampled_codes() {
    let mut codes: Vec<&str> = doctor_owned_samples().iter().map(|s| s.code).collect();
    codes.extend(LSP_OWNED_SAMPLES.iter().map(|(c, _)| *c));
    codes.push("diagnostic"); // the code-less parser-error fallback (LSP-owned)

    for code in codes {
        // Total + disjoint: exactly one predicate holds.
        assert!(
            is_lsp_owned(code) ^ is_doctor_owned(code),
            "{code}: ownership must be exactly one of lsp/doctor (total + disjoint)",
        );
    }

    // Each sample lands in the bucket whose resolver Property 1 / 1b
    // exercised — so no code is double-resolved with a conflicting severity.
    for sample in doctor_owned_samples() {
        assert!(
            is_doctor_owned(sample.code),
            "{}: doctor-owned",
            sample.code
        );
    }
    for (code, _) in LSP_OWNED_SAMPLES {
        assert!(is_lsp_owned(code), "{code}: lsp-owned");
    }
}
