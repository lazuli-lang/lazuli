//! Small JSON-walking + name-mangling helpers shared by every
//! lifecycle-gate emitter cluster.
//!
//! Conceptually these belong to two groups:
//!
//! 1. **JSON walkers** — `features`, `array_field`, `string_field`,
//!    `lookup_resource*`, `pick_resource_for_query`,
//!    `surface_platform*`. The emitter reads IR through serialized
//!    JSON (cells land in parallel; see crate-root note), so every
//!    field access goes through `Value::get`.
//!
//! 2. **Name mangling** — `pascal_case`, `lower_camel`,
//!    `route_const_name`, `query_ident`, `sdk_import_path`,
//!    `ts_string`, `canonical`, `is_acronym`. Each produces a TS
//!    identifier or string literal from the IR's snake-case names.
//!    `query_ident` carries the verb-prefix dedup that fixed the
//!    `lookup<Feature>ByLookupMy<R>` regression.

use std::collections::BTreeMap;

use serde_json::Value;

use super::ResourceLifecycle;

pub(super) fn features(root: &Value) -> Vec<&Value> {
    array_field(root, "features")
}

pub(super) fn array_field<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

pub(super) fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(super) fn lookup_resource(
    resources: &BTreeMap<(String, String), ResourceLifecycle>,
    feature: &str,
    resource: &str,
) -> Option<ResourceLifecycle> {
    resources
        .get(&(feature.to_owned(), canonical(resource)))
        .cloned()
}

pub(super) fn lookup_resource_by_name(
    resources: &BTreeMap<(String, String), ResourceLifecycle>,
    resource: &str,
) -> Option<ResourceLifecycle> {
    let key = canonical(resource);
    resources
        .iter()
        .find(|((_, name), _)| name == &key)
        .map(|(_, value)| value.clone())
}

pub(super) fn only_resource_for_feature<'a>(
    resources: &'a BTreeMap<(String, String), ResourceLifecycle>,
    feature: &str,
) -> Option<&'a str> {
    let mut matches = resources
        .values()
        .filter(|resource| resource.feature == feature)
        .map(|resource| resource.name.as_str());
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub(super) fn pick_resource_for_query<'a>(
    query_name: &str,
    resources: &[&'a ResourceLifecycle],
) -> Option<&'a ResourceLifecycle> {
    let q = canonical(query_name);
    resources
        .iter()
        .copied()
        .find(|resource| q.contains(&canonical(&resource.name)))
        .or_else(|| resources.first().copied())
}

pub(super) fn surface_platform(surface: &Value) -> Option<&str> {
    string_field(surface, "target")
        .or_else(|| string_field(surface, "platform"))
        .and_then(surface_platform_label)
}

pub(super) fn surface_platform_label(raw: &str) -> Option<&str> {
    let lc = raw.to_ascii_lowercase();
    if lc.contains("mobile") {
        Some("mobile")
    } else if lc.contains("web") || lc.contains("vite") || lc.contains("tanstack") {
        Some("web")
    } else {
        None
    }
}

pub(super) fn surface_feature(surface: &str) -> Option<String> {
    surface
        .split([' ', '.'])
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(super) fn route_name_feature(name: &str) -> String {
    name.split(['_', '-'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("app")
        .to_owned()
}

pub(super) fn parse_to_view(raw: &str) -> Option<(String, String)> {
    let target = raw.split('(').next()?.trim();
    let (feature, rest) = target.split_once(".view.")?;
    let view = rest.split('.').next()?.trim();
    Some((feature.to_owned(), view.to_owned()))
}

pub(super) fn query_ident(feature: &str, query_name: &str) -> String {
    // Verb-prefix dedup: query names produced by `conventions [me]` /
    // `[crud]` already begin with `lookup_` (e.g. `lookup_my_host`).
    // The legacy `lookup<Feature>By<PascalShort>` shape would emit
    // `lookupHostByLookupMyHost` — drop the `<Feature>By` infix in
    // that case and emit just `lookup<RestPascal>`. Defensive fallback
    // to the legacy shape when the remainder is empty.
    if let Some(rest) = crate::lzx::strip_verb_prefix(query_name, "lookup_") {
        return format!("lookup{}", pascal_case(rest));
    }
    let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
    format!("lookup{}By{}", pascal_case(feature), pascal_case(stripped))
}

pub(super) fn sdk_import_path(current_feature: &str, source_feature: &str) -> String {
    if current_feature == source_feature {
        format!("./{}.gen.js", current_feature)
    } else {
        format!("../{}/{}.gen.js", source_feature, source_feature)
    }
}

pub(super) fn route_const_name(name: &str) -> String {
    format!("{}Route", lower_camel(name))
}

pub(super) fn ts_string(value: &str) -> String {
    // `serde_json::to_string(&str)` only fails on non-UTF8 byte sequences,
    // which Rust `&str` cannot hold. Empty-literal fallback keeps output
    // syntactically valid TS even on the impossible-error path.
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

pub(super) fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn pascal_case(value: &str) -> String {
    let mut out = String::new();
    for word in value.split(['_', '-', ' ']) {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for ch in first.to_uppercase() {
                out.push(ch);
            }
        }
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    if out.is_empty() {
        let mut chars = value.chars();
        if let Some(first) = chars.next() {
            for ch in first.to_uppercase() {
                out.push(ch);
            }
            out.push_str(chars.as_str());
        }
    }
    out
}

pub(super) fn lower_camel(value: &str) -> String {
    let pascal = pascal_case(value);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::new();
            for ch in first.to_lowercase() {
                out.push(ch);
            }
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}
