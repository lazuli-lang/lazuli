//! Report vocab — `<feature>/reports.gen.go` emission.
//!
//! Walks every `report <name>` declaration on a feature and emits one
//! `report.Contract` value per declaration. Per proposal §Codegen
//! worked example, the emitter is wire-thin: it imports the report +
//! storage runtime, declares the typed Contract, and emits a
//! `Run<Name>` entry point that delegates to `report.Run`.
//!
//! Determinism: reports are sorted by name; column / format ordering
//! preserves source order (the IR captures author intent).
//!
//! See `docs/proposals/report-vocab.md` v0.2.

use lazuli_ir::{
    Feature, FileVisibility, PolicyAtom, PolicyExpr, PolicyRef, Report, ReportColumnSource,
    ReportFormat, ReportSource,
};

use super::casing::pascal_case;
use super::imports::ImportSet;
use super::patterns::{PATTERN_REPORT_AUTOMOUNT, PATTERN_REPORT_RUN, emit_pattern_header};
use super::printer::GoPrinter;

/// Emit `<feature>/reports.gen.go` for a feature, or `None` when the
/// feature declares no reports.
pub fn emit_reports_file(source_label: &str, feature: &Feature) -> Option<String> {
    if feature.reports.is_empty() {
        return None;
    }

    let mut reports: Vec<&Report> = feature.reports.iter().collect();
    reports.sort_by(|a, b| a.name.cmp(&b.name));

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/report");
    imports.add("lazuli.dev/runtime/lazuli/storage");
    if reports.iter().any(|r| r.signed_ttl.is_some()) {
        imports.add("time");
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    for (i, report) in reports.iter().enumerate() {
        if i > 0 {
            p.blank();
        }
        emit_report(&mut p, feature, report);
    }

    // Auto-mount registration. One `init()` per file registers every
    // contract with the runtime registry; `lazuli.Mux()` mounts
    // `GET /api/reports/<name>.<format>` routes by walking that
    // registry. The registered runner reads the package-level
    // `<Name>Runner` variable each request so user code can override
    // the stub once source + store are wired (typically from `main`
    // after `lazuli.Boot`). See
    // `runtime/go/lazuli/report/{registry,mount}.go` +
    // `docs/proposals/report-vocab.md` §Open questions.
    p.blank();
    emit_pattern_header(&mut p, PATTERN_REPORT_AUTOMOUNT);
    p.line("func init() {");
    p.indent();
    for report in &reports {
        let var_name = format!("{}Report", pascal_case(&report.name));
        let runner_name = format!("{}Runner", pascal_case(&report.name));
        p.line(&format!(
            "report.Register({var_name}, func(ctx context.Context, format report.Format) (string, error) {{"
        ));
        p.indent();
        p.line(&format!("if {runner_name} != nil {{"));
        p.indent();
        p.line(&format!("return {runner_name}(ctx, format)"));
        p.dedent();
        p.line("}");
        p.line(&format!(
            "return \"\", report.ErrRunnerNotWired({:?})",
            report.name
        ));
        p.dedent();
        p.line("})");
    }
    p.dedent();
    p.line("}");

    Some(p.finish())
}

fn emit_report(p: &mut GoPrinter, feature: &Feature, report: &Report) {
    let var_name = format!("{}Report", pascal_case(&report.name));
    let runner_name = format!("{}Runner", pascal_case(&report.name));

    p.line(&format!(
        "// {}: {} (auto-mounts `GET /api/reports/{}.<format>` per format).",
        var_name, report.name, report.name
    ));
    p.line(&format!("var {} = report.Contract{{", var_name));
    p.indent();
    p.line(&format!("Feature: {:?},", feature.name));
    p.line(&format!("Name: {:?},", report.name));
    p.line(&format!("Source: {:?},", source_string(&report.source)));

    p.line("Columns: []report.Column{");
    p.indent();
    for col in &report.columns {
        emit_column(p, col);
    }
    p.dedent();
    p.line("},");

    p.line("Formats: []report.Format{");
    p.indent();
    for fmt in &report.formats {
        p.line(&format!("report.{},", format_const(*fmt)));
    }
    p.dedent();
    p.line("},");

    if let Some(storage) = &report.storage {
        p.line(&format!("Storage: {:?},", storage.name));
    } else {
        p.line("Storage: \"\",");
    }

    p.line(&format!(
        "Visibility: storage.{},",
        visibility_const(report.visibility)
    ));

    if let Some(ttl) = &report.signed_ttl {
        if let Some(duration) = render_duration(ttl) {
            p.line(&format!("SignedTTL: {},", duration));
        }
    }

    if let Some(filename) = &report.filename {
        p.line(&format!(
            "Filename: report.MustPattern({:?}),",
            filename.literal
        ));
    }

    p.line(&format!("Policy: {:?},", policy_atom(&report.policy)));

    // R.C.4 — emit resolved policy atoms so the auto-mount route can
    // call the policy evaluator before invoking the runner.
    if let Some(atoms) = report_policy_atoms(feature, &report.policy, report.policy_expr.as_ref()) {
        if atoms.is_empty() {
            p.line("Atoms: nil,");
        } else {
            p.line("Atoms: []report.PolicyAtom{");
            p.indent();
            for atom in atoms {
                p.line(&atom);
            }
            p.dedent();
            p.line("},");
        }
    }

    if let Some(rate_limit) = &report.rate_limit {
        // `ir-rate-limit-env-aware` Cell 2 — `report.Contract.RateLimit`
        // is a plain string in the report subpackage (its own contract,
        // not the env-aware `lazuli.RateLimit` struct). Emit the
        // unqualified default verbatim; env-aware reports would need a
        // follow-up to extend `report.Contract`.
        p.line(&format!("RateLimit: {:?},", rate_limit.default));
    }

    p.dedent();
    p.line("}");

    // Per-report Runner hook. User code overrides this variable from
    // `main` once the SourceFn + ObjectStore are wired. The init()
    // block at the end of this file registers a stub that reads the
    // current value on every request, so overriding takes effect
    // without re-registering. A typical user wire-up is:
    //
    //   <feature>.<Name>Runner = func(ctx context.Context, format report.Format) (string, error) {
    //       return <feature>.Run<Name>(lazuli.NewCtx(ctx), format, source, store)
    //   }
    //
    // Until overridden, the auto-mounted route returns HTTP 501 with a
    // message pointing at this site.
    p.blank();
    p.line(&format!(
        "// {runner_name} is the auto-mount runner override hook. See {var_name}'s",
    ));
    p.line(&format!(
        "// auto-mount registration in `init()` at the end of this file."
    ));
    p.line(&format!("var {runner_name} report.Runner"));

    // Per proposal §Codegen worked example, emit a Run<Name> entry
    // point that the auto-mounted HTTP handler calls. The actual HTTP
    // mount is wired by the report runtime + http_recover; this stub
    // pins the signature so the user can override.
    p.blank();
    p.line(&format!(
        "// Run{} executes the report and returns a signed/public URL.",
        pascal_case(&report.name)
    ));
    p.line(&format!(
        "// The auto-mounted HTTP handler at `/api/reports/{}.<format>`",
        report.name
    ));
    p.line(&format!(
        "// calls this entry point. SourceFn wiring is the user's",
    ));
    p.line(&format!(
        "// responsibility (typically `{}.List(ctx, args)`).",
        feature.name
    ));
    emit_pattern_header(p, PATTERN_REPORT_RUN);
    p.line(&format!(
        "func Run{}(ctx *lazuli.Ctx, format report.Format, source report.SourceFn, store storage.ObjectStore) (string, error) {{",
        pascal_case(&report.name)
    ));
    p.indent();
    p.line(&format!(
        "return report.Run(ctx, {}, format, source, store)",
        var_name
    ));
    p.dedent();
    p.line("}");
}

fn emit_column(p: &mut GoPrinter, col: &lazuli_ir::ReportColumn) {
    let from = match &col.source {
        ReportColumnSource::RowField(field) => format!("report.RowField({:?})", field),
        ReportColumnSource::Fn(invocation) => {
            let mut parts = vec![format!("{:?}", invocation.name)];
            for arg in &invocation.args {
                parts.push(format!("{:?}", arg));
            }
            format!("report.FnCall({})", parts.join(", "))
        }
    };

    let mut adornments = String::new();
    if let Some(label) = &col.label {
        adornments.push_str(&format!(", Label: {:?}", label));
    }
    if let Some(format) = &col.format {
        adornments.push_str(&format!(", Format: {:?}", format));
    }
    p.line(&format!(
        "{{Name: {:?}, From: {}{}}},",
        col.name, from, adornments
    ));
}

fn source_string(source: &ReportSource) -> String {
    let ReportSource::Query(qn) = source;
    // Wire registry key: `<feature>.<query_name>` (cell B1 dropped `.query.` infix).
    // The `None` branch (same-feature shorthand) is currently unreachable at
    // emit time because report sources are required to carry a feature, but
    // we keep a stable bare-name fallback for forward-compat.
    match &qn.feature {
        Some(feature) => format!("{}.{}", feature, qn.name),
        None => qn.name.clone(),
    }
}

fn format_const(format: ReportFormat) -> &'static str {
    match format {
        ReportFormat::Csv => "CSV",
        ReportFormat::Xlsx => "XLSX",
    }
}

fn visibility_const(visibility: FileVisibility) -> &'static str {
    match visibility {
        FileVisibility::Public => "VisibilityPublic",
        FileVisibility::Private => "VisibilityPrivate",
        FileVisibility::Signed => "VisibilitySigned",
    }
}

/// Render a duration literal (`1h`, `30s`, `15m`, `7d`) into a Go
/// `time.Duration` expression. Returns `None` for unrecognized shapes
/// so the codegen omits the field rather than emitting invalid Go.
fn render_duration(literal: &str) -> Option<String> {
    let bytes = literal.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let unit_idx = bytes.iter().rposition(|c| c.is_ascii_alphabetic())?;
    let unit = &literal[unit_idx..];
    let amount_str = literal[..unit_idx].trim();
    let amount: u64 = amount_str.parse().ok()?;
    let mult = match unit {
        "s" => "time.Second",
        "m" => "time.Minute",
        "h" => "time.Hour",
        "d" => "24 * time.Hour",
        _ => return None,
    };
    Some(format!("{} * {}", amount, mult))
}

fn policy_atom(policy: &PolicyRef) -> String {
    match policy {
        // `lower_policy_atom` strips the leading `@`; re-prepend it so
        // generated code carries the canonical authored form.
        PolicyRef::Atom(s) => format!("@{}", s),
        PolicyRef::Local(name) => format!("@policy.{}", name),
        PolicyRef::External { feature, name } => format!("{}.@policy.{}", feature, name),
        PolicyRef::Unresolved(s) => s.clone(),
        PolicyRef::None => String::new(),
    }
}

/// R.C.4 — resolve a report's policy into the closed atom slice the
/// runtime evaluator consumes. Returns `None` when no atoms are
/// derivable (the route ships open w.r.t. the auto-mount evaluator;
/// the verbatim `Policy: "@policy.<name>"` field stays for audit and
/// for future feature-local resolution).
///
/// Structured `policy_expr` takes precedence (it carries the full
/// `has_role` / `has_permission` / combinator tree). When only the
/// simple `policy @<ns>.<name>` form is present, lower a single atom
/// directly; feature-local `@policy.<name>` references resolve through
/// the feature's named policy categories when present.
fn report_policy_atoms(
    feature: &Feature,
    policy: &PolicyRef,
    policy_expr: Option<&PolicyExpr>,
) -> Option<Vec<String>> {
    if let Some(expr) = policy_expr {
        let mut out = Vec::new();
        walk_report_policy_expr_atoms(expr, &mut out);
        return Some(out);
    }
    match policy {
        PolicyRef::Atom(atom) => {
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            if let Some(local) = stripped.strip_prefix("policy.") {
                return report_local_policy_atoms(feature, local);
            }
            report_policy_atom_literal(atom).map(|atom| vec![atom])
        }
        PolicyRef::Local(name) => report_local_policy_atoms(feature, name),
        PolicyRef::Unresolved(raw) => {
            let stripped = raw.strip_prefix('@').unwrap_or(raw);
            if let Some(local) = stripped.strip_prefix("policy.") {
                return report_local_policy_atoms(feature, local);
            }
            None
        }
        PolicyRef::External { .. } | PolicyRef::None => None,
    }
}

fn report_local_policy_atoms(feature: &Feature, name: &str) -> Option<Vec<String>> {
    let category = feature
        .policies
        .categories
        .iter()
        .find(|category| category.name == name)?;
    let mut out = Vec::new();
    for atom in &category.atoms {
        if let Some(expr) = report_policy_atom_expr(atom) {
            walk_report_policy_expr_atoms(&expr, &mut out);
        }
    }
    Some(out)
}

fn report_policy_atom_literal(atom: &str) -> Option<String> {
    let expr = report_policy_atom_expr(atom)?;
    let mut out = Vec::new();
    walk_report_policy_expr_atoms(&expr, &mut out);
    out.into_iter().next()
}

fn report_policy_atom_expr(atom: &str) -> Option<PolicyExpr> {
    let stripped = atom.strip_prefix('@').unwrap_or(atom);
    if stripped.starts_with("policy.") {
        return None;
    }
    let mut parts = stripped.splitn(2, '.');
    let namespace = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if namespace.is_empty() || name.is_empty() {
        return None;
    }
    Some(PolicyExpr::Atom(PolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args: None,
    }))
}

fn walk_report_policy_expr_atoms(expr: &PolicyExpr, out: &mut Vec<String>) {
    match expr {
        PolicyExpr::Authenticated => {
            out.push("{Namespace: \"predicate\", Name: \"authenticated\"},".to_owned());
        }
        PolicyExpr::HasRole(name) => {
            out.push(format!("{{Namespace: \"rbac.role\", Name: {:?}}},", name));
        }
        PolicyExpr::HasPermission(perm) => {
            out.push(format!(
                "{{Namespace: \"rbac.permission\", Name: {:?}}},",
                perm
            ));
        }
        PolicyExpr::Atom(atom) => {
            out.push(format!(
                "{{Namespace: {:?}, Name: {:?}}},",
                atom.namespace, atom.name
            ));
        }
        PolicyExpr::And(terms) => {
            out.push("{Namespace: \"predicate\", Name: \"(\"},".to_owned());
            for (i, term) in terms.iter().enumerate() {
                if i > 0 {
                    out.push("{Namespace: \"predicate\", Name: \"and\"},".to_owned());
                }
                walk_report_policy_expr_atoms(term, out);
            }
            out.push("{Namespace: \"predicate\", Name: \")\"},".to_owned());
        }
        PolicyExpr::Or(terms) => {
            out.push("{Namespace: \"predicate\", Name: \"(\"},".to_owned());
            for (i, term) in terms.iter().enumerate() {
                if i > 0 {
                    out.push("{Namespace: \"predicate\", Name: \"or\"},".to_owned());
                }
                walk_report_policy_expr_atoms(term, out);
            }
            out.push("{Namespace: \"predicate\", Name: \")\"},".to_owned());
        }
        PolicyExpr::Not(inner) => {
            out.push("{Namespace: \"predicate\", Name: \"not\"},".to_owned());
            walk_report_policy_expr_atoms(inner, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, FileVisibility, Policies, PolicyCategory, PolicyRef, QualifiedName, Report,
        ReportColumn, ReportColumnSource, ReportFormat, ReportSource,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn mk_report(name: &str, formats: Vec<ReportFormat>) -> Report {
        Report {
            name: name.to_owned(),
            source: ReportSource::Query(QualifiedName {
                feature: None,
                name: "list".to_owned(),
            }),
            columns: vec![ReportColumn {
                name: "id".to_owned(),
                source: ReportColumnSource::RowField("id".to_owned()),
                label: None,
                format: None,
                span_ref: None,
            }],
            formats,
            storage: None,
            visibility: FileVisibility::Signed,
            signed_ttl: Some("1h".to_owned()),
            filename: None,
            policy: PolicyRef::Local("global_read".to_owned()),
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn empty_feature_emits_nothing() {
        let feature = base_feature("customer");
        assert!(emit_reports_file("examples/x.lzi", &feature).is_none());
    }

    #[test]
    fn single_report_emits_contract_runner_and_init() {
        let mut feature = base_feature("customer");
        feature.reports.push(mk_report(
            "monthly_audit",
            vec![ReportFormat::Csv, ReportFormat::Xlsx],
        ));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");

        // Contract and Runner var still declared per report.
        assert!(out.contains("var MonthlyAuditReport = report.Contract{"));
        assert!(out.contains("var MonthlyAuditRunner report.Runner"));

        // Auto-mount init() block — registers the contract with a
        // closure that calls the package-level Runner each request.
        assert!(out.contains("func init() {"));
        assert!(out.contains(
            "report.Register(MonthlyAuditReport, func(ctx context.Context, format report.Format) (string, error) {"
        ));
        assert!(out.contains("if MonthlyAuditRunner != nil {"));
        assert!(out.contains("return MonthlyAuditRunner(ctx, format)"));
        assert!(out.contains("return \"\", report.ErrRunnerNotWired(\"monthly_audit\")"));

        // Context import threaded through for the closure signature.
        assert!(out.contains("\"context\""));

        // Existing Run<Name> entry point preserved (still 4-arg form).
        assert!(out.contains(
            "func RunMonthlyAudit(ctx *lazuli.Ctx, format report.Format, source report.SourceFn, store storage.ObjectStore) (string, error) {"
        ));
    }

    #[test]
    fn two_reports_emit_one_init_with_both_registrations() {
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        feature
            .reports
            .push(mk_report("daily_summary", vec![ReportFormat::Xlsx]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");

        // Only ONE init() block holds both registrations.
        assert_eq!(out.matches("func init() {").count(), 1);
        assert!(out.contains("report.Register(MonthlyAuditReport,"));
        assert!(out.contains("report.Register(DailySummaryReport,"));
        assert!(out.contains("MonthlyAuditRunner"));
        assert!(out.contains("DailySummaryRunner"));

        // Reports stay sorted by name (daily before monthly).
        let daily_pos = out.find("DailySummaryReport").expect("daily declared");
        let monthly_pos = out.find("MonthlyAuditReport").expect("monthly declared");
        assert!(daily_pos < monthly_pos, "expected sorted output");
    }

    // R.C.4 — Atoms emission from `policy_expr`.

    #[test]
    fn report_with_policy_expr_emits_atoms_slice() {
        let mut feature = base_feature("customer");
        let mut report = mk_report("monthly_audit", vec![ReportFormat::Csv]);
        report.policy_expr = Some(PolicyExpr::And(vec![
            PolicyExpr::Authenticated,
            PolicyExpr::HasPermission("reports:read".to_owned()),
        ]));
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \"(\"}"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \"authenticated\"}"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \"and\"}"));
        assert!(out.contains("{Namespace: \"rbac.permission\", Name: \"reports:read\"}"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \")\"}"));
    }

    #[test]
    fn report_with_policy_atom_emits_single_resolved_atom() {
        let mut feature = base_feature("customer");
        let mut report = mk_report("public_data", vec![ReportFormat::Csv]);
        report.policy = PolicyRef::Atom("scope.public".to_owned());
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"scope\", Name: \"public\"}"));
    }

    #[test]
    fn report_with_local_policy_ref_emits_feature_policy_atoms() {
        let mut feature = base_feature("customer");
        feature.policies.categories.push(PolicyCategory {
            name: "global_read".to_owned(),
            atoms: vec!["@role.admin".to_owned(), "@scope.same_org".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
        });
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"role\", Name: \"admin\"}"));
        assert!(out.contains("{Namespace: \"scope\", Name: \"same_org\"}"));
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn report_with_legacy_policy_atom_ref_emits_feature_policy_atoms() {
        let mut feature = base_feature("customer");
        feature.policies.categories.push(PolicyCategory {
            name: "global_read".to_owned(),
            atoms: vec!["@role.admin".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
        });
        let mut report = mk_report("monthly_audit", vec![ReportFormat::Csv]);
        report.policy = PolicyRef::Atom("policy.global_read".to_owned());
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"role\", Name: \"admin\"}"));
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn report_with_unresolved_legacy_policy_ref_emits_feature_policy_atoms() {
        let mut feature = base_feature("customer");
        feature.policies.categories.push(PolicyCategory {
            name: "global_read".to_owned(),
            atoms: vec!["@role.admin".to_owned()],
            previous_names: Vec::new(),
            when_denied: None,
        });
        let mut report = mk_report("monthly_audit", vec![ReportFormat::Csv]);
        report.policy = PolicyRef::Unresolved("@policy.global_read".to_owned());
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"role\", Name: \"admin\"}"));
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn report_with_unresolved_policy_local_skips_atoms() {
        // PolicyRef::Local("global_read") with no matching feature
        // category should NOT synthesise atoms (would deny incorrectly).
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(!out.contains("Atoms: []report.PolicyAtom{"));
        // Verbatim Policy: field stays for audit.
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("zebra", vec![ReportFormat::Csv]));
        feature
            .reports
            .push(mk_report("alpha", vec![ReportFormat::Xlsx]));
        let a = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        let b = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert_eq!(a, b);
    }
}
