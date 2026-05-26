//! TS literal formatting helpers shared across `.lzx` emitters.
//!
//! Each `.lzx` view-kind emitter renders a typed-spec literal that
//! eventually feeds the runtime hook (`useAdminSlugListView`, etc.).
//! This module centralises the TS-shape primitives — `cells`,
//! `columns` / `search` / `filter` / `fields` arrays, `onSuccess`
//! options — so the emitters stay focused on their per-kind shape.

use super::ir::{CellBinding, OnSuccessSpec};

/// Format the `cells` literal for the spec const — a TS object literal
/// like `{ tags: "@client.type_badge" as const }`. Empty input emits an
/// empty object so the spec field is always present.
pub(crate) fn format_cells_literal(cells: &[CellBinding]) -> String {
    if cells.is_empty() {
        return "{}".to_owned();
    }
    let parts: Vec<String> = cells
        .iter()
        .map(|c| format!("{}: \"@client.{}\" as const", c.field, c.slot))
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

/// Format a `["a", "b", "c"]` string literal array with each entry
/// quoted. Used for `columns` / `search` / `filter` / `fields`.
pub(crate) fn format_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_owned();
    }
    let parts: Vec<String> = items.iter().map(|s| format!("\"{}\"", s)).collect();
    format!("[{}]", parts.join(", "))
}

pub(crate) fn format_on_success_options(spec: &OnSuccessSpec, host_feature: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if spec.back {
        parts.push("back: true".to_owned());
    }
    if let Some(redirect) = &spec.redirect {
        parts.push(format!("redirect: \"{}\"", escape_ts_string(redirect)));
    }
    if let Some(flash) = &spec.flash {
        parts.push(format!(
            "flash: {{ kind: \"{}\", messageKey: \"{}\" }}",
            escape_ts_string(&flash.kind),
            escape_ts_string(&flash.message_key.key)
        ));
    }
    if !spec.invalidates.is_empty() {
        parts.push(format!(
            "invalidates: {}",
            format_string_array(&on_success_invalidates(spec, host_feature))
        ));
    }
    if spec.replace {
        parts.push("replace: true".to_owned());
    }
    format!("{{ {} }}", parts.join(", "))
}

fn on_success_invalidates(spec: &OnSuccessSpec, host_feature: &str) -> Vec<String> {
    spec.invalidates
        .iter()
        .map(|invalidates| {
            let feature = invalidates
                .query
                .feature
                .as_deref()
                .unwrap_or(host_feature);
            format!("{}.{}", feature, invalidates.query.name)
        })
        .collect()
}

pub(crate) fn escape_ts_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_cells_literal_handles_empty_and_populated() {
        assert_eq!(format_cells_literal(&[]), "{}");
        let cells = vec![CellBinding {
            field: "tags".to_owned(),
            slot: "type_badge".to_owned(),
        }];
        assert_eq!(
            format_cells_literal(&cells),
            "{ tags: \"@client.type_badge\" as const }"
        );
    }

    #[test]
    fn format_string_array_handles_empty_and_populated() {
        assert_eq!(format_string_array(&[]), "[]");
        let items = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(format_string_array(&items), "[\"a\", \"b\", \"c\"]");
    }
}
