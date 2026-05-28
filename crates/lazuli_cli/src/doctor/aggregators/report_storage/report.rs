//! REPORT-* rule fan-out per `docs/proposals/report-vocab.md` v0.2.
//!
//! Builds a synthetic `ir::Feature` view from each `Tier3FeatureFacts`
//! row and dispatches the closed REPORT-* catalog: column shape,
//! signed-URL TTL, source kind, rate-limit gating, path collisions,
//! signed-no-storage, storage-ambiguous, format-unknown.
//!
//! Lifted from the parent `report_storage` god-file in the rails-style
//! split.

use lazuli_ir::AppManifest;

use crate::doctor::report;
use crate::doctor::{
    collect_object_storage_caps, DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity,
    Tier3FeatureFacts,
};

/// Run the 10 `REPORT-*` doctor rules per
/// `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP, aggregating
/// findings into typed `DoctorDiagnostic` rows.
pub(crate) fn report_diagnostics(
    facts: &[Tier3FeatureFacts],
    app: Option<&AppManifest>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let storage_caps = collect_object_storage_caps(app, registry);

    for fact in facts {
        if fact.reports.is_empty() {
            continue;
        }
        let mut feature_for_rules = make_synthetic_feature_for_reports(fact);

        // Local-only rules consume the synthesized Feature view.
        for finding in report::report_columns_empty_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_columns_empty_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_signed_ttl_missing_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_signed_ttl_missing_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in
            report::report_signed_ttl_forbidden_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_signed_ttl_forbidden_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in
            report::report_filename_token_unknown_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_filename_token_unknown_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_source_kind_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_source_kind_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // W5 GAP-REPORT-01 — REPORT-INPUT-UNBOUND-001. Warning: a
        // declared `input` param the local source query never consumes
        // is suspicious but does not break codegen.
        for finding in report::report_input_unbound_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: report::report_input_unbound_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in
            report::report_policy_public_no_rate_limit_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_policy_public_no_rate_limit_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_column_mismatch_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_column_mismatch_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_path_collision_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_path_collision_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_signed_no_storage_001::check(
            &feature_for_rules,
            &storage_caps,
            &fact.path,
        ) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_signed_no_storage_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_storage_ambiguous_001::check(
            &feature_for_rules,
            &storage_caps,
            &fact.path,
        ) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_storage_ambiguous_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // AST-based rule (REPORT-FORMAT-UNKNOWN-001) reads the raw
        // ReportDecl text because lowering drops unknown format tokens.
        for finding in
            report::report_format_unknown_001::check(&fact.feature, &fact.report_decls, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_format_unknown_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // Drop the unused borrow to satisfy the borrow checker on the
        // synthetic feature value (none of the rule branches return
        // borrowed data beyond their iteration).
        let _ = &mut feature_for_rules;
    }

    diagnostics
}

/// Build a minimal `ir::Feature` view from a `Tier3FeatureFacts` row so
/// the report rule modules (which take `&Feature`) can be invoked
/// without re-lowering. Only the slots the rule modules read are
/// populated; everything else stays at default.
pub(crate) fn make_synthetic_feature_for_reports(fact: &Tier3FeatureFacts) -> lazuli_ir::Feature {
    lazuli_ir::Feature {
        name: fact.feature.clone(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: lazuli_ir::Defaults::default(),
        // GAP-07 — carry the feature's `uses` so MANY-THROUGH-ENDPOINT-001
        // can resolve cross-feature partner endpoints (a `many_through`
        // partner living in a dependency feature). Previously dropped; other
        // domain rules don't read `uses`, so this is additive.
        uses: fact.uses.clone(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: fact.resources.clone(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: lazuli_ir::Policies::default(),
        errors: None,
        commands: Vec::new(),
        apis: fact.apis.clone(),
        records: fact.records.clone(),
        queries: fact.queries.clone(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: fact.agents.clone(),
        reports: fact.reports.clone(),
        pollers: vec![],
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: fact.aggregates.clone(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}

