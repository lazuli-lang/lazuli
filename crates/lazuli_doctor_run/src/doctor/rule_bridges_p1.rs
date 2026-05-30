pub(crate) fn vocab_grammar_form_diagnostics(
    files: &[DoctorFile],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    let severity = doctor_rule_severity(security_profile);
    files
        .iter()
        .filter(|file| is_lzi_path(&file.path))
        .flat_map(|file| {
            vocab::vocab_grammar_form_001::check(&file.source, &file.path)
                .into_iter()
                .map(move |finding| {
                    let message = finding.message();
                    DoctorDiagnostic {
                        path: finding.path,
                        line: finding.line,
                        column: finding.column,
                        severity,
                        code: vocab::vocab_grammar_form_001::Finding::CODE.to_owned(),
                        message,
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    }
                })
        })
        .collect()
}

/// MONEY-1 §3.2 — bridge `lazuli_doctor::vocab::money_compare_001` into the
/// CLI's `DoctorDiagnostic` shape. Fixed `Error` severity per the proposal:
/// mixed-currency comparisons silently lose money, which is a bug
/// regardless of `prototype`/`strict`/`production` posture.
pub(crate) fn money_compare_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::vocab::money_compare_001;
    money_compare_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: money_compare_001::Finding::CODE.to_owned(),
                message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// Wave 0 — bridge `lazuli_doctor::vocab::vocab_tests_missing_001`
/// into the CLI's `DoctorDiagnostic` shape. The detector has shipped
/// since 2026-05-15 but was never dispatched (see Issue Zero of
/// `docs/proposals/tdd-bdd-first-2026-05-23.md`); this helper closes
/// that gap.
///
/// Severity follows the legacy global mapping (warning at strict,
/// error at production). The rule's `RuleCategory` is `Vocabulary`
/// (matches the module path); Wave 1 will land separate `TEST-*`
/// rules under `RuleCategory::TestDiscipline`. Prototype profile
/// suppresses the warning so quick spikes are not blocked by
/// test-vocabulary discipline.
pub(crate) fn vocab_tests_missing_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_tests_missing_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &std::collections::BTreeMap::new(),
    );
    vocab::vocab_tests_missing_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_tests_missing_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: Some(finding.feature),
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// MONEY-1 §3.2 — bridge `lazuli_doctor::vocab::money_arithmetic_001` into
/// the CLI's `DoctorDiagnostic` shape. Same fixed-`Error` policy as the
/// comparison check: cross-currency or Money-times-Money arithmetic is a
/// structural bug.
pub(crate) fn money_arithmetic_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::vocab::money_arithmetic_001;
    money_arithmetic_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: money_arithmetic_001::Finding::CODE.to_owned(),
                message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// VOCAB-* bridges — wire the remaining vocab rule modules that ship under
// `lazuli_doctor::vocab` but have no dispatcher entry. Each bridge mirrors
// the shape of `vocab_tests_missing_001_diagnostics` above: take the
// already-loaded Feature (+ Path + line anchor + SecurityProfile), forward
// to the rule's `check()`, and wrap every `Finding` in the canonical
// `DoctorDiagnostic` envelope under `RuleCategory::Vocabulary`.
//
// Severity policy: defer to `doctor_severity_for` which honors any user
// override in `[doctor.<category>].severity_override.<CODE>` and otherwise
// returns the legacy mapping (Strict/Prototype → Warning, Production →
// Error). `SecurityProfile::Prototype` suppresses the entire family —
// vocabulary-fitness lints are opt-in at prototype profile (you sketch
// the feature first, vocabulary refactors come when you promote it).
//
// See `docs/proposals/doctor-vocabulary-lints.md` §"Implementation status
// (post-wave)" for the deferred wiring cell this closes.
// ─────────────────────────────────────────────────────────────────────────

/// Build the empty-override map the new vocab bridges pass into
/// `doctor_severity_for`. No `[doctor.vocab]` TOML section ships today;
/// when one lands the caller threads the parsed overrides in here.
fn empty_overrides() -> std::collections::BTreeMap<String, DoctorSeverityOverride> {
    std::collections::BTreeMap::new()
}

/// VOCAB-AUDIT-001 — `invalidate_session` command without an authored
/// audit record. See `docs/proposals/doctor-vocabulary-lints.md`.
pub(crate) fn vocab_audit_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_audit_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_audit_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_audit_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// VOCAB-AUDIT-002 — `invalidate_session` over a session that carries
/// sensitive capability-tagged fields without `audit { ... }`.
pub(crate) fn vocab_audit_002_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_audit_002::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_audit_002::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_audit_002::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// VOCAB-CAP-MISSING-001 — `@pii.*` field with no `@cap.*` wrapper.
/// Sources from the raw `.lzi` text (`check_source`) because the IR
/// lowering currently drops trailing `@pii.*` decorators.
pub(crate) fn vocab_cap_missing_001_diagnostics(
    path: &Path,
    source: &str,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_cap_missing_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_cap_missing_001::check_source(source, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_cap_missing_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// VOCAB-DERIVED-READ-001 — handler-computed read-only field that should
/// use the `derived from` primitive instead.
pub(crate) fn vocab_derived_read_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_derived_read_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_derived_read_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_derived_read_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// VOCAB-EVENT-ORPHAN-001 — feature-level event with no producer command.
pub(crate) fn vocab_event_orphan_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_event_orphan_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_event_orphan_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_event_orphan_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// VOCAB-EVENT-PAYLOAD-001 — event without a typed payload contract.
pub(crate) fn vocab_event_payload_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_event_payload_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_event_payload_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_event_payload_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}
