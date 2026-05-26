//! API emitter formatting helpers.

use lazuli_ir::HttpMethod;

use super::super::casing::pascal_case;
use super::super::printer::GoPrinter;

pub(super) fn method_const_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "lazuli.MethodGet",
        HttpMethod::Post => "lazuli.MethodPost",
        HttpMethod::Put => "lazuli.MethodPut",
        HttpMethod::Patch => "lazuli.MethodPatch",
        HttpMethod::Delete => "lazuli.MethodDelete",
    }
}

pub(super) fn api_args_type_name(name: &str) -> String {
    // Suffix `ApiArgs` so the type doesn't collide with a sibling
    // `query.list <name>` (which uses `<Name>Args`). See the var-name
    // comment on `emit_api` for the full story.
    format!("{}ApiArgs", pascal_case(name))
}

pub(super) fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

pub(super) fn escape_string(raw: &str) -> String {
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
