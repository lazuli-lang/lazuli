//! Raw-payload line parsing and Go-type inference for event groups.
//! Pre-Phase L Tier 3 fixtures express payload shape as free-form
//! `name = expr [when ...]` lines, so we parse the line, pick a Go
//! type by expression head + IR resource lookup, and fall back to
//! suffix-based name heuristics (`_id`, `_at`).
//!
//! Outputs are consumed by `payload::group_payload_fields`.

use lazuli_ir::Resource;

use super::{is_payload_name, register_imports_for_type};
use crate::emitter::imports::ImportSet;
use crate::emitter::types::{self, TypeCtx};

pub(super) struct RawPayloadLine {
    pub(super) name: String,
    pub(super) expr: Option<String>,
    pub(super) optional: bool,
}

pub(super) fn parse_raw_payload_line(line: &str) -> Option<RawPayloadLine> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }

    if let Some((lhs, rhs)) = trimmed.split_once('=') {
        let name = lhs.trim();
        if !is_payload_name(name) {
            return None;
        }
        let (expr, optional) = split_when(rhs.trim());
        return Some(RawPayloadLine {
            name: name.to_owned(),
            expr: Some(expr.to_owned()),
            optional,
        });
    }

    if let Some((lhs, _rhs)) = trimmed.split_once(':') {
        let name = lhs.trim();
        if !is_payload_name(name) {
            return None;
        }
        return Some(RawPayloadLine {
            name: name.to_owned(),
            expr: None,
            optional: false,
        });
    }

    let name = trimmed.split_whitespace().next().unwrap_or("");
    if !is_payload_name(name) {
        return None;
    }
    Some(RawPayloadLine {
        name: name.to_owned(),
        expr: None,
        optional: false,
    })
}

fn split_when(raw: &str) -> (&str, bool) {
    if let Some((expr, _when)) = raw.split_once(" when ") {
        (expr.trim(), true)
    } else {
        (raw.trim(), false)
    }
}

pub(super) struct InferredType {
    pub(super) go_type: String,
}

pub(super) fn infer_group_payload_type(
    parsed: &RawPayloadLine,
    resource: Option<&Resource>,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) -> InferredType {
    let Some(expr) = parsed.expr.as_deref() else {
        return infer_by_name(&parsed.name, imports);
    };
    let expr = expr.trim();
    if expr == "id" || expr.ends_with(".id") || parsed.name.ends_with("_id") {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.ID".to_owned(),
        };
    }
    if expr == "ctx.now" || parsed.name.ends_with("_at") {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.Time".to_owned(),
        };
    }
    if let Some(resource) = resource {
        let head = expr.split('.').next().unwrap_or(expr);
        if let Some(field) = resource.fields.iter().find(|field| field.name == head) {
            register_imports_for_type(&field.type_ref, ctx, imports);
            let (go_type, _import) = types::go_type_for(&field.type_ref, ctx);
            return InferredType { go_type };
        }
    }
    infer_by_name(&parsed.name, imports)
}

fn infer_by_name(name: &str, imports: &mut ImportSet) -> InferredType {
    if name.ends_with("_id") || name == "id" {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.ID".to_owned(),
        };
    }
    if name.ends_with("_at") {
        imports.add("lazuli.dev/runtime/lazuli");
        return InferredType {
            go_type: "lazuli.Time".to_owned(),
        };
    }
    InferredType {
        go_type: "any".to_owned(),
    }
}

