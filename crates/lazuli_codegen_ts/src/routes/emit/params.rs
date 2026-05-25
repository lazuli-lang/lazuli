//! Typed-param lowering for routes that declared
//! `route <name>: <Type>` lines. Two concerns:
//! - `classify_route_param_type` + `nav_arg_*` translate the IR's
//!   type-ref string into a TS type label and decide whether the
//!   `nav.<name>(params)` helper needs to coerce the value to a
//!   string at the wire boundary.
//! - `emit_route_params_interface` + `emit_route_params_parser`
//!   write per-route `<Route>Params` interfaces and parsers so route
//!   screens can drop the legacy `Number(params.id)` /
//!   `as unknown as number` ceremony.

use std::fmt::Write;

use super::super::spec::RouteSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypedParamKind {
    Id,
    Text,
    Integer,
    Decimal,
    Boolean,
}

/// Classify a `route <name>: <Type>` type reference. Conservative —
/// any unrecognised label falls back to `Text` (string), matching
/// the per-feature view-route emitter's behavior.
pub(super) fn classify_route_param_type(raw: &str) -> TypedParamKind {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized == "id" || normalized.ends_with(".id") {
        return TypedParamKind::Id;
    }
    match normalized.as_str() {
        "integer" | "int" => TypedParamKind::Integer,
        "decimal" | "float" | "number" => TypedParamKind::Decimal,
        "boolean" | "bool" => TypedParamKind::Boolean,
        _ => TypedParamKind::Text,
    }
}

pub(super) fn nav_arg_ts_type(raw: &str) -> String {
    match classify_route_param_type(raw) {
        TypedParamKind::Id => "ID".to_owned(),
        TypedParamKind::Integer | TypedParamKind::Decimal => "number".to_owned(),
        TypedParamKind::Boolean => "boolean".to_owned(),
        TypedParamKind::Text => "string".to_owned(),
    }
}

pub(super) fn nav_arg_needs_string_coercion(raw: &str) -> bool {
    matches!(
        classify_route_param_type(raw),
        TypedParamKind::Id | TypedParamKind::Integer | TypedParamKind::Decimal | TypedParamKind::Boolean
    )
}

/// Wave §2 — emit the typed `<Route>Params` interface for a route
/// that declared `route <name>: <Type>` lines. The shape mirrors
/// the per-feature view-route emitter so consumers can rely on a
/// consistent name pattern across both surfaces.
pub(super) fn emit_route_params_interface(out: &mut String, spec: &RouteSpec) {
    let pascal = super::super::pascal_case(&spec.name);
    writeln!(out, "export interface {pascal}Params {{").ok();
    for param in &spec.route_params {
        let ty = match classify_route_param_type(&param.type_ref) {
            TypedParamKind::Id => "ID".to_owned(),
            TypedParamKind::Integer | TypedParamKind::Decimal => "number".to_owned(),
            TypedParamKind::Boolean => "boolean".to_owned(),
            TypedParamKind::Text => "string".to_owned(),
        };
        writeln!(out, "  {}: {ty};", param.name).ok();
    }
    writeln!(out, "}}").ok();
    writeln!(out).ok();
}

pub(super) fn emit_route_params_parser(out: &mut String, spec: &RouteSpec) {
    let pascal = super::super::pascal_case(&spec.name);
    writeln!(
        out,
        "export function parse{pascal}Params(raw: Record<string, string>): {pascal}Params | null {{"
    )
    .ok();
    for param in &spec.route_params {
        let raw_access = format!("raw.{}", param.name);
        match classify_route_param_type(&param.type_ref) {
            TypedParamKind::Id => {
                writeln!(
                    out,
                    "  const {n} = parseId({raw_access}); if ({n} == null) return null;",
                    n = param.name
                )
                .ok();
            }
            TypedParamKind::Integer => {
                writeln!(
                    out,
                    "  const {n} = Number.parseInt({raw_access} ?? \"\", 10); if (Number.isNaN({n})) return null;",
                    n = param.name
                )
                .ok();
            }
            TypedParamKind::Decimal => {
                writeln!(
                    out,
                    "  const {n} = Number({raw_access}); if (!Number.isFinite({n})) return null;",
                    n = param.name
                )
                .ok();
            }
            TypedParamKind::Boolean => {
                writeln!(
                    out,
                    "  const {n} = {raw_access} === \"true\" ? true : {raw_access} === \"false\" ? false : null; if ({n} == null) return null;",
                    n = param.name
                )
                .ok();
            }
            TypedParamKind::Text => {
                writeln!(
                    out,
                    "  const {n} = {raw_access}; if (typeof {n} !== \"string\" || {n}.length === 0) return null;",
                    n = param.name
                )
                .ok();
            }
        }
    }
    let fields = spec
        .route_params
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "  return {{ {fields} }};").ok();
    writeln!(out, "}}").ok();
    writeln!(out).ok();
}
