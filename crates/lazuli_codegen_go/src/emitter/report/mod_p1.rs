/// Emit `<feature>/reports.gen.go` for a feature, or `None` when the
/// feature declares no reports.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_reports_file("billing.lzi", &feature);
/// ```
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

    // W5 GAP-REPORT-01 — emit the declared `input { … }` params so the
    // auto-mount route can parse + validate them off the request query
    // string and thread them to the SourceFn via the request context.
    if !report.input.is_empty() {
        p.line("Inputs: []report.Input{");
        p.indent();
        for slot in &report.input {
            p.line(&format!(
                "{{Name: {:?}, Type: {:?}, Required: {}}},",
                slot.name,
                input_type_token(&slot.type_ref),
                slot.required
            ));
        }
        p.dedent();
        p.line("},");
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

/// Render a report input param's [`TypeRef`] into the verbatim authoring
/// token carried on `report.Input.Type`. The runtime treats this as an
/// opaque hint (required-ness is enforced from the bool; coercion stays
/// in the SourceFn), so the token mirrors the authored spelling rather
/// than a Go type. Builtins map to their canonical names; user-defined /
/// enum / unresolved references pass their terminal name through.
fn input_type_token(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Builtin(b) => builtin_token(b).to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Unresolved(s) => s.clone(),
        TypeRef::Many(inner) => format!("Many<{}>", input_type_token(inner)),
        TypeRef::Capability(_) => "Capability".to_owned(),
    }
}

fn builtin_token(b: &BuiltinType) -> &'static str {
    match b {
        BuiltinType::Id => "ID",
        BuiltinType::Text => "Text",
        BuiltinType::Boolean => "Boolean",
        BuiltinType::Integer => "Integer",
        BuiltinType::Decimal => "Decimal",
        BuiltinType::Date => "Date",
        BuiltinType::DateTime => "DateTime",
        BuiltinType::Json => "Json",
        BuiltinType::SemanticEmail => "@semantic.Email",
        BuiltinType::SemanticMoney { .. } => "@semantic.Money",
        BuiltinType::SemanticPhone => "@semantic.Phone",
        BuiltinType::SemanticUrl => "@semantic.Url",
        BuiltinType::SemanticUuid => "@semantic.Uuid",
        BuiltinType::SemanticCurrency => "@semantic.Currency",
        BuiltinType::SemanticGeoPoint => "@semantic.GeoPoint",
        BuiltinType::SemanticHexColor => "@semantic.HexColor",
        BuiltinType::SemanticPercentage => "@semantic.Percentage",
        BuiltinType::SemanticPluginType { .. } => "@semantic.Plugin",
        BuiltinType::CapSecret => "@cap.Secret",
        BuiltinType::CapFile => "@cap.File",
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
