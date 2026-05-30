//! Small formatting helpers shared by the job emitter.
//!
//! Owns the trigger / qname / path / arg-key / expr renderers used
//! across the per-section emitters, plus the duration parser for
//! `Timeout:` field rendering, the section banner writer used at
//! the top of every job's emission block, the policy / effect
//! summary stringifiers, and the casing utilities (`pascal_case`,
//! `lower_camel`, `job_var_name`) that name the package-level var.
//!
//! Every helper is pure — no `GoPrinter` mutation, no IR walking
//! beyond the immediate value being rendered.

use lazuli_ir::{
    BackoffStrategy, CommandEffect, FanoutScope, Feature, JobTrigger, NamedArg, Path, PolicyRef,
};

use crate::emitter::printer::GoPrinter;
use crate::emitter::types::{self, TypeCtx};

pub(super) fn format_trigger(feature: &Feature, trigger: &JobTrigger) -> String {
    match trigger {
        JobTrigger::Event { event } => format!(
            "jobs.JobTrigger{{Kind: \"event\", Event: \"{}\"}},",
            escape_string(&format_qname(Some(&feature.name), event))
        ),
        JobTrigger::Schedule { cron } => format!(
            "jobs.JobTrigger{{Kind: \"schedule\", Cron: \"{}\"}},",
            escape_string(cron)
        ),
    }
}

pub(super) fn backoff_const(backoff: BackoffStrategy) -> &'static str {
    match backoff {
        BackoffStrategy::Fixed => "jobs.BackoffFixed",
        BackoffStrategy::Exponential => "jobs.BackoffExponential",
    }
}

pub(super) fn fanout_scope(scope: FanoutScope) -> &'static str {
    match scope {
        FanoutScope::Tenants => "tenants",
    }
}

pub(super) fn policy_string(policy: &PolicyRef) -> String {
    match policy {
        PolicyRef::Local(name) => format!("@policy.{}", escape_string(name)),
        PolicyRef::Atom(atom) => {
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            format!("@{}", escape_string(stripped))
        }
        PolicyRef::External { feature, name } => {
            format!("{}.policy.{}", escape_string(feature), escape_string(name))
        }
        PolicyRef::Unresolved(raw) => escape_string(raw),
        PolicyRef::None => String::new(),
    }
}

pub(super) fn effect_summary(effect: &CommandEffect, ctx: &TypeCtx<'_>) -> String {
    match effect {
        CommandEffect::Creates(create) => format!("creates {}", create.resource.name),
        CommandEffect::Updates(update) => format!("updates {}", update.resource.name),
        CommandEffect::Deletes(delete) => format!("deletes {}", delete.resource.name),
        CommandEffect::Reorders(reorder) => {
            format!(
                "reorders {} by {}",
                reorder.resource.name, reorder.position_field
            )
        }
        CommandEffect::Returns(ret) => {
            let (go_type, _import) = types::go_type_for(&ret.return_type, ctx);
            format!("returns {go_type}")
        }
        CommandEffect::None => "none".to_owned(),
    }
}

pub(super) fn timeout_expr(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let split_at = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    if split_at == 0 || split_at == raw.len() {
        return None;
    }

    let amount = &raw[..split_at];
    if !amount.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let unit = &raw[split_at..];
    let go_unit = match unit {
        "ns" => "Nanosecond",
        "us" => "Microsecond",
        "ms" => "Millisecond",
        "s" => "Second",
        "m" => "Minute",
        "h" => "Hour",
        _ => return None,
    };

    if amount == "0" {
        return Some("0".to_owned());
    }
    if amount == "1" {
        return Some(format!("time.{go_unit}"));
    }
    Some(format!("{amount} * time.{go_unit}"))
}

pub(super) fn format_qname(
    default_feature: Option<&str>,
    qname: &lazuli_ir::QualifiedName,
) -> String {
    if qname.name.contains('.') && qname.feature.is_none() {
        return qname.name.clone();
    }
    match qname.feature.as_deref().or(default_feature) {
        Some(feature) => format!("{feature}.{}", qname.name),
        None => qname.name.clone(),
    }
}

pub(super) fn format_path(path: &Path) -> String {
    path.segments.join(".")
}

pub(super) fn format_args_key(args: &[NamedArg]) -> String {
    sorted_arg_strings(args).join("\u{1f}")
}

pub(super) fn sorted_arg_strings(args: &[NamedArg]) -> Vec<String> {
    let mut out: Vec<String> = args
        .iter()
        .map(|arg| format!("{}={}", arg.name, format_expr(&arg.value)))
        .collect();
    out.sort();
    out
}

pub(super) fn format_expr(expr: &lazuli_ir::Expr) -> String {
    match expr {
        lazuli_ir::Expr::Path(path) => format_path(path),
        lazuli_ir::Expr::String(value) => format!("\"{}\"", escape_string(value)),
        lazuli_ir::Expr::Integer(value) => value.to_string(),
        lazuli_ir::Expr::Boolean(value) => value.to_string(),
        lazuli_ir::Expr::Enum(literal) => match &literal.type_name {
            Some(qname) => format!("{}.{}", format_qname(None, qname), literal.variant),
            None => literal.variant.clone(),
        },
        lazuli_ir::Expr::Nil => "nil".to_owned(),
        // Job-level format_expr is used for trigger inputs / log
        // rendering; FnCall isn't expected here today, fall back to a
        // best-effort textual rendering for diagnostics.
        lazuli_ir::Expr::FnCall(call) => {
            let args: Vec<String> = call.args.iter().map(format_expr).collect();
            format!("@fn.{}({})", call.name.name, args.join(", "))
        }
    }
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

pub(super) fn job_var_name(feature_name: &str, job_name: &str) -> String {
    format!("{}{}Job", lower_camel(feature_name), pascal_case(job_name))
}

pub(super) fn pascal_case(s: &str) -> String {
    crate::emitter::casing::pascal_case(s)
}

pub(super) fn lower_camel(s: &str) -> String {
    crate::emitter::casing::lower_camel(s)
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

pub(super) fn escape_comment(raw: &str) -> String {
    raw.replace('\n', " ").replace('\r', " ")
}
