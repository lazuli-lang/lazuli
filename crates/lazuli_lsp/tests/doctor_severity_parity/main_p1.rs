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
