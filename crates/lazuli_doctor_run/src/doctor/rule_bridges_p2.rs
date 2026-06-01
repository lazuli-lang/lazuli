/// VOCAB-EVENT-PRODUCER-001 — command emits an event that has no
/// matching feature-level event declaration.
pub(crate) fn vocab_event_producer_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_event_producer_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_event_producer_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_event_producer_001::Finding::CODE.to_owned(),
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

/// VOCAB-HANDLER-HEAVY-001 — too many handler-only commands; feature
/// should refactor toward declarative vocabulary.
pub(crate) fn vocab_handler_heavy_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_handler_heavy_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_handler_heavy_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_handler_heavy_001::Finding::CODE.to_owned(),
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

/// VOCAB-JSON-TYPED-001 — untyped JSON field paired with an orphan
/// same-feature enum that should be the field's type.
pub(crate) fn vocab_json_typed_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_json_typed_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_json_typed_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_json_typed_001::Finding::CODE.to_owned(),
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

/// VOCAB-LIFECYCLE-001 — resource still spells a lifecycle as N transition
/// commands plus a status enum. Suggests refactoring into a resource-local
/// `lifecycle` block. Lifecycle IR shipped in v0.2 and the rule module is
/// now published by `crates/lazuli_doctor/src/vocab/mod.rs:45`; this
/// bridge closes the wiring gap flagged by
/// `docs/proposals/lifecycle-vocab-architect-audit-2026-05-27.md` §"Cell
/// A".
pub(crate) fn vocab_lifecycle_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_lifecycle_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_lifecycle_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_lifecycle_001::Finding::CODE.to_owned(),
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

/// VOCAB-MONEY-MULTI-CURRENCY-001 — resource declares multiple Money
/// fields without per-field currency override.
pub(crate) fn vocab_money_multi_currency_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_money_multi_currency_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_money_multi_currency_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_money_multi_currency_001::Finding::CODE.to_owned(),
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

/// VOCAB-MONEY-SHAPE-001 — money modelled the hand-rolled way
/// (`_cents:Integer`+`_currency:Text`, bare money-named `Decimal`, or
/// string-tagged money with no currency sibling) instead of `Money`.
pub(crate) fn vocab_money_shape_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::money_field_shape_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::money_field_shape_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::money_field_shape_001::Finding::CODE.to_owned(),
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

/// VOCAB-RESOURCE-WIDE-CLUSTER-001 — resource whose field set clusters
/// around a shared token (e.g. `shipping_*` cluster) hinting at a
/// sub-resource extraction.
///
/// Module dependency: this rule walks across features to resolve FK
/// types via `is_universal_column`. Callers synthesize a single-feature
/// `Module` from the Tier3 facts so cross-feature FK resolution falls
/// back to "unresolved → not universal", which is the correct default
/// when only the current feature's IR is in hand.
pub(crate) fn vocab_resource_wide_cluster_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_resource_wide_cluster_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_resource_wide_cluster_001::check(feature, module, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_resource_wide_cluster_001::Finding::CODE.to_owned(),
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

/// VOCAB-SHADOW-RECORD-001 — record/resource pair that mirrors columns.
/// Same `Module` synthesis note as `vocab_resource_wide_cluster_001`.
pub(crate) fn vocab_shadow_record_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_shadow_record_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_shadow_record_001::check(feature, module, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_shadow_record_001::Finding::CODE.to_owned(),
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

/// VOCAB-UNION-001 — enum-tag + correlated-optional-fields. Suggests
/// the discriminated `union` vocabulary.
pub(crate) fn vocab_union_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_union_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_union_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_union_001::Finding::CODE.to_owned(),
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

/// VOCAB-UNION-002 — enum discriminator + untyped FK sibling that
/// should be expressed as a discriminated union.
pub(crate) fn vocab_union_002_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_union_002::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &empty_overrides(),
    );
    vocab::vocab_union_002::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_union_002::Finding::CODE.to_owned(),
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

/// Parse `design.lzi` at the project root into the lowered IR. Returns
/// `None` when the file is missing OR parse/lower fails — doctor's
/// `design-custom-*` rules then suppress (no false positives when the
/// file isn't authored yet). Mirrors the parse-then-lower pipeline used
/// by `lazuli build`.
pub(crate) fn read_design_ir(project_root: &Path) -> Option<lazuli_ir::Design> {
    let path = project_root.join("design.lzi");
    let source = std::fs::read_to_string(&path).ok()?;
    let ast = lazuli_syntax::parse_design_document(&source).ok()?;
    lazuli_analyzer::lower_design(&ast).ok()
}

pub(crate) fn doctor_rule_path(project_root: &Path, path: PathBuf) -> PathBuf {
    path.strip_prefix(project_root)
        .unwrap_or(&path)
        .to_path_buf()
}

/// CODEGEN-WRAP-001 - typed-error constructors forbidden in bucket source.
///
/// The one-wrap boundary (docs/proposals/bucket-ai-debug-loop-cycle.md §7.2)
/// requires that *lazuli.FieldError, *lazuli.PolicyError, etc. struct values
/// are constructed ONLY in codegen-emitted handlers (the .gen.go boundary),
/// never in hand-written bucket source under runtime/go/lazuli/<bucket>/.
pub(crate) fn check_codegen_wrap_001(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let bucket_root = project_root.join("runtime/go/lazuli");
    if !bucket_root.exists() {
        return diagnostics;
    }

    collect_codegen_wrap_001(&bucket_root, &bucket_root, &mut diagnostics);
    diagnostics
}
