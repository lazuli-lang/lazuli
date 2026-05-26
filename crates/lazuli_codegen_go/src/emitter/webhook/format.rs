//! Webhook emitter formatting helpers.
//!
//! Holds pure formatting / escaping / shaping utilities lifted from the
//! original `webhook.rs` god file. Keeps `mod.rs` focused on walking the
//! `Webhook` IR and the surrounding orchestrator.

use lazuli_ir::{PolicyRef, TypeRef, Webhook};

use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};

pub(super) fn format_policy_string(policy: &PolicyRef) -> Option<String> {
    match policy {
        PolicyRef::Local(name) => Some(format!("@policy.{}", name)),
        PolicyRef::Atom(atom) => {
            if atom.starts_with('@') {
                Some(atom.clone())
            } else {
                Some(format!("@{}", atom))
            }
        }
        PolicyRef::External { feature, name } => Some(format!("{}.policy.{}", feature, name)),
        PolicyRef::Unresolved(raw) => Some(raw.clone()),
        PolicyRef::None => None,
    }
}

pub(super) fn return_type_name(return_type: &TypeRef, ctx: &TypeCtx<'_>) -> String {
    let (go_type, _import) = types::go_type_for(return_type, ctx);
    go_type
}

pub(super) fn format_string_slice(values: &[String]) -> String {
    let entries: Vec<String> = values
        .iter()
        .map(|value| format!("\"{}\"", escape_string(value)))
        .collect();
    format!("[]string{{{}}},", entries.join(", "))
}

pub(super) fn emit_runtime_gaps(p: &mut GoPrinter, webhook: &Webhook) {
    if webhook.structured_verify.is_none() {
        p.line(&format!(
            "// TODO(runtime): legacy verifier path \"{}\" is not represented by WebhookContract v0.",
            escape_string(&webhook.verify.path)
        ));
    }
}

pub(super) fn path_to_string(path: &lazuli_ir::Path) -> String {
    path.segments.join(".")
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

pub(super) fn pascal_case(s: &str) -> String {
    super::super::casing::pascal_case(s)
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
