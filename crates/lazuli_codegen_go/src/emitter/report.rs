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
    Feature, FileVisibility, PolicyRef, Report, ReportColumnSource, ReportFormat, ReportSource,
};

use super::casing::pascal_case;
use super::imports::ImportSet;
use super::patterns::{PATTERN_REPORT_RUN, emit_pattern_header};
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
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/report");
    imports.add("lazuli.dev/runtime/lazuli/storage");
    if reports.iter().any(|r| r.signed_ttl.is_some()) {
        imports.add("time");
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    for (i, report) in reports.iter().enumerate() {
        if i > 0 {
            p.blank();
        }
        emit_report(&mut p, feature, report);
    }

    Some(p.finish())
}

fn emit_report(p: &mut GoPrinter, feature: &Feature, report: &Report) {
    let var_name = format!("{}Report", pascal_case(&report.name));

    p.line(&format!(
        "// {}: {} (auto-mounts `GET /api/reports/{}.<format>` per format).",
        var_name, report.name, report.name
    ));
    p.line(&format!("var {} = report.Contract{{", var_name));
    p.indent();
    p.line(&format!(
        "Feature: {:?},",
        feature.name
    ));
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

    if let Some(rate_limit) = &report.rate_limit {
        p.line(&format!("RateLimit: {:?},", rate_limit));
    }

    p.dedent();
    p.line("}");

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
    match &qn.feature {
        Some(feature) => format!("{}.query.{}", feature, qn.name),
        None => format!("query.{}", qn.name),
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
