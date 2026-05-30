//! Route-param parser emitter for `.lzx` detail views.
//!
//! Each generated file is per feature + target:
//!
//! ```text
//! dist/ts-web/<feature>/<feature>.routes.gen.ts
//! dist/ts-mobile/<feature>/<feature>.routes.gen.ts
//! ```
//!
//! Parsers are pure and return `null` on invalid coercion. Runtime
//! redirect/error behavior lives in `useLazuliRouteParams`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lazuli_ir as ir;

use crate::lzx::{banner, pascal_case};

/// Emit the per-feature `.routes.gen.ts` parser file for one platform.
///
/// Collects every detail view declared for the target's surfaces,
/// derives a typed `Params` interface and a pure validator per view,
/// and returns the full file contents. Returns `None` if the feature
/// has no detail views on that target — the caller should skip writing
/// the file rather than emit an empty stub.
///
/// `target_prefix` must be either `"ts-web"` or `"ts-mobile"`; any
/// other value is treated as "no matching surfaces" and yields `None`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx_route_params::emit_route_params_ts;
/// use lazuli_ir::{Feature, Module};
///
/// let module: Module = /* … */ unimplemented!();
/// let feature: &Feature = &module.features[0];
/// if let Some(src) = emit_route_params_ts(feature, &module, "ts-web") {
///     assert!(src.contains("export"));
/// }
/// ```
pub fn emit_route_params_ts(
    feature: &ir::Feature,
    module: &ir::Module,
    target_prefix: &str,
) -> Option<String> {
    let specs = collect_specs(feature, target_prefix);
    if specs.is_empty() {
        return None;
    }

    let mut s = String::new();
    s.push_str(banner());
    write_imports(&mut s, &specs, module);
    write_enum_value_consts(&mut s, &specs, module);
    let spec_count = specs.len();
    for (index, spec) in specs.into_iter().enumerate() {
        write_params_interface(&mut s, &spec, module);
        write_parser(&mut s, &spec, module);
        if index + 1 < spec_count {
            writeln!(s).ok();
        }
    }
    Some(s)
}

fn collect_specs(feature: &ir::Feature, target_prefix: &str) -> Vec<RouteParserSpec> {
    let target = match target_prefix {
        "ts-web" => ir::SurfaceTarget::Web,
        "ts-mobile" => ir::SurfaceTarget::Mobile,
        _ => return Vec::new(),
    };
    let mut by_view = BTreeMap::new();
    for surface in &feature.surfaces {
        if surface.target != target {
            continue;
        }
        for audience in &surface.audiences {
            for view in &audience.views {
                let ir::View::Detail(detail) = view else {
                    continue;
                };
                if detail.route_params.is_empty() {
                    continue;
                }
                by_view
                    .entry(detail.name.clone())
                    .or_insert_with(|| detail.route_params.clone());
            }
        }
    }
    by_view
        .into_iter()
        .map(|(view_name, params)| RouteParserSpec { view_name, params })
        .collect()
}

#[derive(Debug, Clone)]
struct RouteParserSpec {
    view_name: String,
    params: Vec<ir::RouteParam>,
}

fn write_imports(s: &mut String, specs: &[RouteParserSpec], module: &ir::Module) {
    let needs_id = specs.iter().any(|spec| {
        spec.params
            .iter()
            .any(|param| matches!(param_kind(&param.type_ref, module), ParamKind::Id))
    });
    if needs_id {
        writeln!(s, "import {{ parseId, type ID }} from \"@lazuli/runtime\";").ok();
        writeln!(s).ok();
    }
}

fn write_enum_value_consts(s: &mut String, specs: &[RouteParserSpec], module: &ir::Module) {
    let mut emitted = BTreeSet::new();
    for spec in specs {
        for param in &spec.params {
            if let ParamKind::Enum(enum_ref) = param_kind(&param.type_ref, module) {
                if !emitted.insert(enum_ref.type_name.clone()) {
                    continue;
                }
                let values = enum_ref
                    .values
                    .iter()
                    .map(|value| format_ts_string(value))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    s,
                    "const {} = [{}] as const;",
                    enum_ref.values_const, values
                )
                .ok();
                writeln!(
                    s,
                    "type {} = typeof {}[number];",
                    enum_ref.type_name, enum_ref.values_const
                )
                .ok();
            }
        }
    }
    if !emitted.is_empty() {
        writeln!(s).ok();
    }
}

fn write_params_interface(s: &mut String, spec: &RouteParserSpec, module: &ir::Module) {
    writeln!(
        s,
        "export interface {}Params {{",
        pascal_case(&spec.view_name)
    )
    .ok();
    for param in &spec.params {
        let key = ts_param_name(&param.name);
        writeln!(
            s,
            "  {}: {};",
            ts_property_key(&key),
            ts_type_for_param(&param.type_ref, module)
        )
        .ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_parser(s: &mut String, spec: &RouteParserSpec, module: &ir::Module) {
    let pascal = pascal_case(&spec.view_name);
    writeln!(
        s,
        "export function parse{}Params(raw: Record<string, string>): {}Params | null {{",
        pascal, pascal
    )
    .ok();
    for param in &spec.params {
        write_param_parse_line(s, param, module);
    }
    let fields = spec
        .params
        .iter()
        .map(|param| ts_param_name(&param.name))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(s, "  return {{ {} }};", fields).ok();
    writeln!(s, "}}").ok();
}

fn write_param_parse_line(s: &mut String, param: &ir::RouteParam, module: &ir::Module) {
    let local = ts_param_name(&param.name);
    let raw = raw_access(&param.name);
    match param_kind(&param.type_ref, module) {
        ParamKind::Id => {
            writeln!(
                s,
                "  const {local} = parseId({raw}); if ({local} == null) return null;"
            )
            .ok();
        }
        ParamKind::Text => {
            writeln!(
                s,
                "  const {local} = {raw}; if (typeof {local} !== \"string\" || {local}.length === 0) return null;"
            )
            .ok();
        }
        ParamKind::Integer => {
            writeln!(
                s,
                "  const {local} = Number.parseInt({raw} ?? \"\", 10); if (Number.isNaN({local})) return null;"
            )
            .ok();
        }
        ParamKind::Decimal => {
            writeln!(
                s,
                "  const {local} = Number({raw}); if (!Number.isFinite({local})) return null;"
            )
            .ok();
        }
        ParamKind::Boolean => {
            writeln!(
                s,
                "  const {local} = {raw} === \"true\" ? true : {raw} === \"false\" ? false : null; if ({local} == null) return null;"
            )
            .ok();
        }
        ParamKind::Date | ParamKind::DateTime => {
            writeln!(
                s,
                "  const {local} = new Date({raw} ?? \"\"); if (Number.isNaN({local}.getTime())) return null;"
            )
            .ok();
        }
        ParamKind::Enum(enum_ref) => {
            writeln!(
                s,
                "  const {local} = {values}.includes(({raw} ?? \"\") as {ty}) ? {raw} as {ty} : null; if ({local} == null) return null;",
                values = enum_ref.values_const,
                ty = enum_ref.type_name,
            )
            .ok();
        }
    }
}

fn ts_type_for_param(raw: &str, module: &ir::Module) -> String {
    match param_kind(raw, module) {
        ParamKind::Id => "ID".to_owned(),
        ParamKind::Text => "string".to_owned(),
        ParamKind::Integer | ParamKind::Decimal => "number".to_owned(),
        ParamKind::Boolean => "boolean".to_owned(),
        ParamKind::Date | ParamKind::DateTime => "Date".to_owned(),
        ParamKind::Enum(enum_ref) => enum_ref.type_name,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParamKind {
    Id,
    Text,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Enum(EnumRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumRef {
    type_name: String,
    values_const: String,
    values: Vec<String>,
}

fn param_kind(raw: &str, module: &ir::Module) -> ParamKind {
    let trimmed = raw.trim();
    let local = local_type_name(trimmed);
    let normalized = local.to_ascii_lowercase();
    if normalized == "id" || trimmed.to_ascii_lowercase().ends_with(".id") {
        return ParamKind::Id;
    }
    match normalized.as_str() {
        "text" | "string" => ParamKind::Text,
        "integer" | "int" => ParamKind::Integer,
        "decimal" | "float" | "number" => ParamKind::Decimal,
        "boolean" | "bool" => ParamKind::Boolean,
        "date" => ParamKind::Date,
        "datetime" | "date_time" => ParamKind::DateTime,
        _ => find_enum(trimmed, module)
            .map(ParamKind::Enum)
            .unwrap_or(ParamKind::Text),
    }
}

fn find_enum(raw: &str, module: &ir::Module) -> Option<EnumRef> {
    let local = local_type_name(raw);
    module
        .features
        .iter()
        .flat_map(|feature| feature.enums.iter())
        .find(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&local))
        .map(|enum_decl| {
            let type_name = type_pascal_case(&enum_decl.name);
            EnumRef {
                values_const: enum_value_constant_name(&enum_decl.name),
                values: enum_decl.variants.iter().map(enum_variant_value).collect(),
                type_name,
            }
        })
}

fn local_type_name(raw: &str) -> String {
    if let Some(inner) = raw
        .strip_prefix("EnumRef(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return local_type_name(inner);
    }
    raw.rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(raw)
        .trim()
        .to_owned()
}

fn enum_value_constant_name(type_ref: &str) -> String {
    let mut out = String::with_capacity(type_ref.len() + "_VALUES".len());
    let mut prev_lower_or_digit = false;
    for ch in type_ref.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
            prev_lower_or_digit = false;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out.push_str("_VALUES");
    out
}

fn enum_variant_value(variant: &ir::EnumVariant) -> String {
    match &variant.storage_value {
        Some(ir::StorageValue::String(value)) => value.clone(),
        Some(ir::StorageValue::Integer(value)) => value.to_string(),
        None => variant.name.to_ascii_lowercase(),
    }
}

fn ts_param_name(raw: &str) -> String {
    if raw.contains('_') || raw.contains('-') {
        ts_lower_camel(raw)
    } else {
        raw.to_owned()
    }
}

fn ts_lower_camel(raw: &str) -> String {
    let mut out = String::new();
    for (index, word) in raw
        .split(|ch: char| ch == '_' || ch == '-' || ch == ' ')
        .filter(|word| !word.is_empty())
        .enumerate()
    {
        if index == 0 {
            out.push_str(&word.to_ascii_lowercase());
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(&chars.as_str().to_ascii_lowercase());
            }
        }
    }
    out
}

fn type_pascal_case(raw: &str) -> String {
    if !raw.contains('_') && !raw.contains('-') && !raw.contains(' ') {
        let mut chars = raw.chars();
        let Some(first) = chars.next() else {
            return String::new();
        };
        let mut out = String::new();
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
        return out;
    }
    let mut out = String::new();
    for word in raw
        .split(|ch: char| ch == '_' || ch == '-' || ch == ' ')
        .filter(|word| !word.is_empty())
    {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    out
}

fn raw_access(raw: &str) -> String {
    if is_ts_identifier(raw) {
        format!("raw.{raw}")
    } else {
        format!("raw[{}]", format_ts_string(raw))
    }
}

fn ts_property_key(key: &str) -> String {
    if is_ts_identifier(key) {
        key.to_owned()
    } else {
        format_ts_string(key)
    }
}

fn is_ts_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn format_ts_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests;
