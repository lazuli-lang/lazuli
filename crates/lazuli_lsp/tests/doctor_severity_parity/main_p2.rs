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
