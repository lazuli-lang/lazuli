fn terminal_has_outgoing_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::terminal_has_outgoing::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::terminal_has_outgoing::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::terminal_has_outgoing::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn timestamp_type_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::timestamp_type::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::timestamp_type::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::timestamp_type::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn transition_from_undeclared_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::transition_from_undeclared::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::transition_from_undeclared::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::transition_from_undeclared::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn transition_to_undeclared_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::transition_to_undeclared::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::transition_to_undeclared::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::transition_to_undeclared::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn unreachable_state_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::unreachable_state::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::unreachable_state::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::unreachable_state::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn policy_required_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::policy_required::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::policy_required::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::policy_required::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn no_jump_needs_linear_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::no_jump_needs_linear::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::no_jump_needs_linear::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::no_jump_needs_linear::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn initial_ambiguous_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::initial_ambiguous::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::initial_ambiguous::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::initial_ambiguous::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn invariant_catalog_mismatch_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::invariant_catalog_mismatch::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::invariant_catalog_mismatch::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::invariant_catalog_mismatch::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}
