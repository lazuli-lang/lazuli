//! Naming + literal-format helpers shared across the Command emitter
//! and its sibling submodules (`effects`, `lifecycle`, `policy`, `scope`,
//! `semantic`, `tier4`). Extracted from the monolithic
//! `command/mod.rs` so the entry-point + per-effect bodies stay
//! legible.
//!
//! Symbols are `pub(super)` and re-exported from `command/mod.rs` for
//! the cross-emitter consumers (`api`, `query/header`, `register`,
//! `error_resolver`, `auto_photo`, `handlers/collect/commands`) that
//! address them as `super::command::<name>`.

use lazuli_ir::{Expr, NamedArg, Path, QualifiedName};

use super::super::imports::ImportSet;
use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};

pub(crate) fn format_path(path: &Path) -> String {
    path.segments.join(".")
}

pub(crate) fn format_args_key(args: &[NamedArg]) -> String {
    sorted_arg_strings(args).join("\u{1f}")
}

pub(crate) fn sorted_arg_strings(args: &[NamedArg]) -> Vec<String> {
    let mut out: Vec<String> = args
        .iter()
        .map(|arg| format!("{}={}", arg.name, format_expr(&arg.value)))
        .collect();
    out.sort();
    out
}

pub(crate) fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => format_path(path),
        Expr::String(value) => format!("\"{}\"", escape_string(value)),
        Expr::Integer(value) => value.to_string(),
        Expr::Boolean(value) => value.to_string(),
        Expr::Enum(literal) => match &literal.type_name {
            Some(qname) => format!("{}.{}", format_qname(None, qname), literal.variant),
            None => literal.variant.clone(),
        },
        Expr::Nil => "nil".to_owned(),
        // Diagnostic-only render for FnCall; binding sites use the
        // typed `format_binding_source` path instead.
        Expr::FnCall(call) => {
            let args: Vec<String> = call.args.iter().map(format_expr).collect();
            format!("@fn.{}({})", call.name.name, args.join(", "))
        }
    }
}

pub(crate) fn format_qname(default_feature: Option<&str>, qname: &QualifiedName) -> String {
    if qname.name.contains('.') && qname.feature.is_none() {
        return qname.name.clone();
    }
    match qname.feature.as_deref().or(default_feature) {
        Some(feature) => format!("{feature}.{}", qname.name),
        None => qname.name.clone(),
    }
}

pub(crate) fn pascal_case(s: &str) -> String {
    super::super::casing::pascal_case(s)
}

pub(crate) fn lower_camel(s: &str) -> String {
    super::super::casing::lower_camel(s)
}

/// Escape backslashes and double-quotes so a Go string literal stays
/// well-formed. Backticks are not used here because every literal
/// we emit is double-quoted (single-line strings).
pub(crate) fn escape_string(raw: &str) -> String {
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

pub(crate) fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

/// Walk a `TypeRef` and register every surfaced import on the
/// file-level `ImportSet`. Mirrors `resource.rs::register_imports_for_type`.
pub(crate) fn register_imports_for_type(
    type_ref: &lazuli_ir::TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let lazuli_ir::TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}
