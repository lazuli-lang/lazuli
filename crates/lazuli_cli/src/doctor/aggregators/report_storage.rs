//! Report / storage / query.view aggregator — three loosely-coupled
//! diagnostic families that share the same fact-bundle plumbing:
//!
//! 1. **`report_diagnostics`** — REPORT-* rule fan-out per
//!    `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP. Builds a
//!    synthetic `ir::Feature` view from each `Tier3FeatureFacts` row
//!    (`make_synthetic_feature_for_reports`) and dispatches the closed
//!    REPORT-* catalog: column shape, signed-URL TTL, source kind,
//!    rate-limit gating, path collisions, signed-no-storage,
//!    storage-ambiguous, format-unknown.
//!
//! 2. **`cap_file_storage_diagnostics`** — five `@cap.File(...)`
//!    checks against the lifted file-capability facts:
//!    `cap_file_visibility_undeclared`,
//!    `cap_file_visibility_signed_ttl_mismatch`,
//!    `cap_file_mime_family_unknown`,
//!    `cap_file_size_unit_invalid`,
//!    `cap_file_accept_input_output_mismatch`. The MIME catalog
//!    (`KNOWN_MIME_FAMILIES`) is the closed top-level IANA family
//!    list; subtype `*` and family `*` are accepted at the shape
//!    level and emitted under the wildcard match.
//!
//! 3. **`query_view_sql_file_diagnostics`** — Wave B.4 `query.view`
//!    follow-up. Resolves each `@file.<name>.sql` path against the
//!    project root, requires the file to exist
//!    (`QUERY-VIEW-SQL-FILE-001`), and runs a best-effort unsafe-SQL
//!    scan (`QUERY-VIEW-SQL-UNSAFE-001`) for `%s` formatting markers
//!    and `+` near `$<n>` placeholders.
//!
//! Extracted from `doctor/mod.rs` in rails-style R6-2.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::AppManifest;

use crate::doctor::parsers::{format_accept_list, format_visibility, mime_sets_intersect};
use crate::doctor::report;
use crate::doctor::{
    DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity, FileCapabilityBinding, FileCapabilityFact,
    OperationalFacts, Tier3FeatureFacts, collect_object_storage_caps,
};

/// IANA top-level MIME families recognised by Lazuli's `@cap.File(accept:)`
/// closed catalog. Subtype `*` and family `*` are also accepted at the
/// shape level, but emitted under the wildcard match.
const KNOWN_MIME_FAMILIES: &[&str] = &[
    "text",
    "image",
    "application",
    "audio",
    "video",
    "font",
    "*",
];

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
        uses: Vec::new(),
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

pub(crate) fn cap_file_storage_diagnostics(
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // (1) cap_file_visibility_undeclared — api output without `visibility:`.
    // (3) cap_file_visibility_signed_ttl_mismatch — visibility/signed_ttl
    //     coherence (api outputs and resource fields both).
    // (5) cap_file_mime_family_unknown — MIME family outside the IANA
    //     closed catalog.
    for fact in &operational.file_capability_facts {
        if matches!(fact.binding, FileCapabilityBinding::ApiOutput { .. })
            && fact.capability.visibility.is_none()
        {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_visibility_undeclared".to_owned(),
                message: format!(
                    "api `{}` output declares `@cap.File(...)` without `visibility:`; ambiguous visibility on a file URL is a security contract gap. Declare `visibility:` as `public`, `private`, or `signed`.",
                    match &fact.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    }
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        match (
            fact.capability.visibility,
            fact.capability.signed_ttl.as_deref(),
        ) {
            (Some(lazuli_ir::FileVisibility::Signed), None) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message:
                        "`@cap.File(visibility:signed)` requires `signed_ttl:<duration>` (e.g. `1h`); signed URLs without a TTL contract leak forever."
                            .to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            (Some(other), Some(_)) if !matches!(other, lazuli_ir::FileVisibility::Signed) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message: format!(
                        "`@cap.File(visibility:{})` forbids `signed_ttl`; `signed_ttl` only applies when `visibility:signed`.",
                        format_visibility(other),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            _ => {}
        }

        for mime in &fact.capability.accept {
            if !KNOWN_MIME_FAMILIES.contains(&mime.family.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Warning,
                    code: "cap_file_mime_family_unknown".to_owned(),
                    message: format!(
                        "`@cap.File(accept:{}/{}` uses unknown MIME family `{}`; known families: {}.",
                        mime.family,
                        mime.subtype,
                        mime.family,
                        KNOWN_MIME_FAMILIES.join(", "),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    // (4) cap_file_size_unit_invalid — typed promotion. The IR rejects
    //     unknown units at parse time (the analyzer falls through to
    //     `UserDefined`), so any line that matched `@cap.File(...)`
    //     literally but did NOT produce a typed `FileCapability` fact
    //     is the candidate. We re-walk operational.file_capabilities
    //     (the text-pattern facts) and cross-reference; sites that
    //     have NO typed fact for the same path:line are typing
    //     failures — promote with a typed error.
    for text_fact in &operational.file_capabilities {
        let has_typed = operational
            .file_capability_facts
            .iter()
            .any(|tf| tf.path == text_fact.path && tf.line == text_fact.line);
        if !has_typed {
            diagnostics.push(DoctorDiagnostic {
                path: text_fact.path.clone(),
                line: text_fact.line,
                column: text_fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_size_unit_invalid".to_owned(),
                message:
                    "`@cap.File(max_size:<literal>)` must use a positive integer with unit `kb`, `mb`, or `gb`; the surrounding `@cap.File(...)` shape did not lower to typed IR."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // (2) cap_file_accept_input_output_mismatch — per-feature, pair
    //     resource-field `@cap.File` inputs with api-output `@cap.File`
    //     outputs and require the accept sets to intersect.
    let mut by_feature: BTreeMap<&str, (Vec<&FileCapabilityFact>, Vec<&FileCapabilityFact>)> =
        BTreeMap::new();
    for fact in &operational.file_capability_facts {
        let entry = by_feature.entry(fact.feature.as_str()).or_default();
        match fact.binding {
            FileCapabilityBinding::ResourceField { .. } => entry.0.push(fact),
            FileCapabilityBinding::ApiOutput { .. } => entry.1.push(fact),
        }
    }
    for (_, (inputs, outputs)) in by_feature {
        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }
        for output in &outputs {
            for input in &inputs {
                if !mime_sets_intersect(&output.capability.accept, &input.capability.accept) {
                    let api_name = match &output.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    };
                    let (resource_name, field_name) = match &input.binding {
                        FileCapabilityBinding::ResourceField { resource, field } => {
                            (resource.as_str(), field.as_str())
                        }
                        _ => ("<unknown>", "<unknown>"),
                    };
                    diagnostics.push(DoctorDiagnostic {
                        path: output.path.clone(),
                        line: output.line,
                        column: output.column,
                        severity: DoctorSeverity::Error,
                        code: "cap_file_accept_input_output_mismatch".to_owned(),
                        message: format!(
                            "api `{api_name}` output declares `@cap.File(accept:{})` but resource `{resource_name}.{field_name}` declares `@cap.File(accept:{})`; accept lists must intersect for the round-trip to be valid.",
                            format_accept_list(&output.capability.accept),
                            format_accept_list(&input.capability.accept),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        }
    }

    diagnostics
}

/// Wave B.4 — `query.view` is a typed SQL-backed screen-read primitive.
/// The analyzer lowers `source @file.<name>.sql` into the canonical
/// project-relative file path; doctor owns the filesystem and best-effort
/// unsafe-SQL checks.
pub(crate) fn query_view_sql_file_diagnostics(
    facts: &[Tier3FeatureFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for feature in facts {
        for query in &feature.queries {
            let lazuli_ir::Query::Sql(query) = query else {
                continue;
            };
            if query.sql_kind != lazuli_ir::SqlQueryKind::View {
                continue;
            }

            let line = feature
                .query_lines
                .get(query.name.as_str())
                .copied()
                .unwrap_or(feature.feature_line);
            let sql_path = resolve_query_view_sql_path(project_root, &query.sql_path);
            if !sql_path.is_file() {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "QUERY-VIEW-SQL-FILE-001".to_owned(),
                    message: format!(
                        "`query.view {}` references SQL source `{}` but the file does not exist.",
                        query.name, query.sql_path
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            }

            let Ok(sql) = fs::read_to_string(&sql_path) else {
                continue;
            };
            if let Some((sql_line, reason)) = query_view_unsafe_sql_line(&sql) {
                diagnostics.push(DoctorDiagnostic {
                    path: sql_path,
                    line: sql_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "QUERY-VIEW-SQL-UNSAFE-001".to_owned(),
                    message: format!(
                        "`query.view {}` SQL looks like it builds user-influenced text instead of binding parameters: {reason}.",
                        query.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

fn resolve_query_view_sql_path(project_root: &Path, sql_path: &str) -> PathBuf {
    let path = Path::new(sql_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn query_view_unsafe_sql_line(sql: &str) -> Option<(usize, &'static str)> {
    for (idx, line) in sql.lines().enumerate() {
        if line.contains("'%s'") || line.contains("\"%s\"") || line.contains("%s") {
            return Some((idx + 1, "`%s` formatting marker"));
        }
        if plus_near_dollar_placeholder(line) {
            return Some((idx + 1, "`+` near a `$<n>` placeholder"));
        }
    }
    None
}

fn plus_near_dollar_placeholder(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte != b'$'
            || !bytes
                .get(idx + 1)
                .map(|b| b.is_ascii_digit())
                .unwrap_or(false)
        {
            continue;
        }
        let start = idx.saturating_sub(48);
        let end = (idx + 48).min(bytes.len());
        let window = &bytes[start..end];
        if window.contains(&b'+') && window.contains(&b'\'') {
            return true;
        }
    }
    false
}
