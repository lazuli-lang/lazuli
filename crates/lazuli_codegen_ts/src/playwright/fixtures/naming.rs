//! Naming + parsing helpers used by the Playwright-fixture emitter:
//! route-target parsing, casing normalisation, and TS-literal escaping.

use lazuli_ir::View;

pub(super) fn route_target_feature(value: Option<&str>) -> Option<String> {
    value?
        .split_once(".view.")
        .map(|(feature, _)| feature.to_owned())
}

pub(super) fn route_target_view_name(value: Option<&str>) -> Option<String> {
    let (_, tail) = value?.split_once(".view.")?;
    let name = tail.split('(').next().unwrap_or(tail).trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

pub(super) fn surface_feature(value: &str) -> Option<String> {
    value.split_whitespace().next().map(str::to_owned)
}

pub(super) fn route_feature_from_name(value: &str) -> String {
    value.split('_').next().unwrap_or("app").to_owned()
}

pub(super) fn view_route(view: &View) -> Option<&str> {
    match view {
        View::List(view) => view.route.as_deref(),
        View::Detail(view) => view.route.as_deref(),
        View::Create(view) => view.route.as_deref(),
    }
}

pub(super) fn lifecycle_type_name(role: &str) -> String {
    format!("{}LifecycleState", pascal_case(role))
}

pub(super) fn canonical(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .map(|ch| match ch {
            '-' => '_',
            other => other,
        })
        .collect()
}

pub(super) fn pascal_case(value: &str) -> String {
    let mut out = String::new();
    for word in value.split(|ch: char| ch == '_' || ch == '-' || ch == ' ') {
        if word.is_empty() {
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    out
}

pub(super) fn escape_ts_single_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}
