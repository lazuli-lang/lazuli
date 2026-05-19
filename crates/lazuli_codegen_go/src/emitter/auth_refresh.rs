//! IR Auth Refresh CODEGEN-1 — per-feature framework handler wire.
//! Emits `<feature>/auth.refresh.gen.go` only when `auth sessions`
//! enables refresh-token rotation. The runtime owns the handler body;
//! generated code only registers the function pointer for the
//! framework-emitted `auth.refresh` command.

use lazuli_ir::Feature;

use super::imports::ImportSet;
use super::patterns::{emit_pattern_header, PATTERN_AUTH_REFRESH};
use super::printer::GoPrinter;

/// Emit `<feature>/auth.refresh.gen.go`, or `None` when:
/// - the feature has no `auth` block,
/// - the auth block has no `sessions` declaration, or
/// - `auth.sessions.rotation` is absent.
pub fn emit_auth_refresh_file(source_label: &str, feature: &Feature) -> Option<String> {
    let sessions = feature.auth.as_ref()?.sessions.as_ref()?;
    if !sessions.is_rotation_enabled() {
        return None;
    }

    let feature_pascal = super::casing::pascal_case(&feature.name);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/auth");

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    emit_section_banner(
        &mut p,
        &[
            format!("Auth refresh command: {}", feature.name),
            "  wires the framework-emitted auth.refresh handler".to_owned(),
        ],
    );
    emit_pattern_header(&mut p, PATTERN_AUTH_REFRESH);
    p.line("func init() {");
    p.indent();
    let _ = feature_pascal;
    p.line(&format!(
        "lazuli.RegisterFn(\"{}.auth.refresh\", auth.RefreshHandler)",
        escape_string(&feature.name),
    ));
    p.dedent();
    p.line("}");

    Some(p.finish())
}

fn emit_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}
