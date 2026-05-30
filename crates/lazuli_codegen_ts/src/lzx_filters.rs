//! Filter-state emitter for `.lzx view list` hooks.
//!
//! The output is intentionally just hook wiring: `.lzx` declares typed
//! filter state, the runtime helper owns the small React state machine.

use std::collections::BTreeSet;

use crate::lzx::{FilterCardinality, FilterDecl, Surface};

/// Emit a `filters` return field backed by `useFilterState(...)`.
///
/// Returns an empty string when the view declares no filters — the
/// caller's emitter knows to skip the trailing comma when this is
/// empty.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx_filters::emit_filters_block;
/// use lazuli_codegen_ts::lzx::{FilterDecl, Surface};
///
/// let filters: Vec<FilterDecl> = vec![];
/// let surface: Surface = /* … */ unimplemented!();
/// assert_eq!(emit_filters_block(&filters, &surface), "");
/// ```
pub fn emit_filters_block(filters: &[FilterDecl], _surface: &Surface) -> String {
    if filters.is_empty() {
        return String::new();
    }

    format!("filters: useFilterState({}),", emit_filter_config(filters))
}

/// Emit the hook-local `const filters = useFilterState(...)` declaration.
pub(crate) fn emit_filters_const(filters: &[FilterDecl], surface: &Surface) -> String {
    let block = emit_filters_block(filters, surface);
    if block.is_empty() {
        return String::new();
    }
    // `emit_filters_block` always returns `filters: <expr>,` when non-empty,
    // so the strip pair below succeeds in practice. Fall back to the raw
    // block if the contract is violated — TS will still parse it.
    let invocation = block
        .strip_prefix("filters: ")
        .and_then(|s| s.strip_suffix(','))
        .unwrap_or(block.as_str());
    format!("  const filters = {invocation};\n")
}

/// Feature-SDK enum value constants needed by these filter configs.
pub(crate) fn enum_value_imports(filters: &[FilterDecl]) -> Vec<String> {
    filters
        .iter()
        .filter(|filter| !is_scalar_type_ref(&filter.type_ref))
        .map(|filter| enum_value_constant_name(&filter.type_ref))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn has_url_synced_filters(filters: &[FilterDecl]) -> bool {
    filters.iter().any(|filter| filter.url_sync)
}

fn emit_filter_config(filters: &[FilterDecl]) -> String {
    let entries: Vec<String> = filters.iter().map(emit_filter_entry).collect();
    format!("{{ {} }}", entries.join(", "))
}

fn emit_filter_entry(filter: &FilterDecl) -> String {
    // GAP-UX-07 — date_range lowers to a paired from/to picker via a dedicated
    // emitter (the runtime `useFilterState` helper reads `fromKey`/`toKey` for
    // `mode: "date_range"` and surfaces `<name>_from` / `<name>_to`). Routing
    // it from the match arm keeps the match exhaustive — a new cardinality
    // variant becomes a compile error here, not a runtime `unreachable!`.
    let mode = match filter.cardinality {
        FilterCardinality::Single => "single",
        FilterCardinality::Multi => "multi",
        FilterCardinality::DateRange => return emit_date_range_filter_entry(filter),
    };

    let mut fields = vec![format!("mode: \"{mode}\"")];
    if !is_scalar_type_ref(&filter.type_ref) {
        fields.push(format!(
            "values: [...{}] as const",
            enum_value_constant_name(&filter.type_ref)
        ));
    }
    if filter.url_sync {
        fields.push(format!("urlKey: \"{}\"", filter.name));
        fields.push("params".to_owned());
        fields.push("setParams".to_owned());
    } else {
        fields.push("urlKey: undefined".to_owned());
    }

    format!("{}: {{ {} }}", filter.name, fields.join(", "))
}

/// Emit the paired-date-picker config for a `date_range` filter. Always
/// surfaces `fromKey`/`toKey` (the two query params); `params`/`setParams`
/// are threaded only when the filter is URL-synced (`from query`).
fn emit_date_range_filter_entry(filter: &FilterDecl) -> String {
    let mut fields = vec![
        "mode: \"date_range\"".to_owned(),
        format!("fromKey: \"{}_from\"", filter.name),
        format!("toKey: \"{}_to\"", filter.name),
    ];
    if filter.url_sync {
        fields.push("params".to_owned());
        fields.push("setParams".to_owned());
    }
    format!("{}: {{ {} }}", filter.name, fields.join(", "))
}

fn is_scalar_type_ref(type_ref: &str) -> bool {
    matches!(
        type_ref,
        "" | "Text"
            | "String"
            | "Int"
            | "Integer"
            | "Decimal"
            | "Float"
            | "Boolean"
            | "Bool"
            | "Date"
            | "DateTime"
            | "Time"
            | "Json"
            | "JSON"
            | "Id"
            | "ID"
            | "@semantic.Email"
            | "@semantic.Money"
            | "@semantic.Phone"
            | "@semantic.GeoPoint"
    )
}

fn enum_value_constant_name(type_ref: &str) -> String {
    let local = type_ref
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(type_ref);
    let mut out = String::with_capacity(local.len() + "_VALUES".len());
    let mut prev_lower_or_digit = false;

    for ch in local.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            prev_lower_or_digit = false;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }
    out.push_str("_VALUES");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::ir::*;

    fn surface() -> Surface {
        Surface {
            feature: "item".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![],
            span_ref: None,
        }
    }

    fn filter(
        name: &str,
        type_ref: &str,
        cardinality: FilterCardinality,
        url_sync: bool,
    ) -> FilterDecl {
        FilterDecl {
            name: name.to_owned(),
            type_ref: type_ref.to_owned(),
            cardinality,
            url_sync,
            span_ref: None,
        }
    }

    #[test]
    fn empty_filters_emit_empty_string() {
        assert_eq!(emit_filters_block(&[], &surface()), "");
    }

    #[test]
    fn single_enum_filter_without_url_sync_emits_values_and_undefined_url_key() {
        let out = emit_filters_block(
            &[filter("type", "ItemType", FilterCardinality::Single, false)],
            &surface(),
        );

        assert_eq!(
            out,
            "filters: useFilterState({ type: { mode: \"single\", values: [...ITEM_TYPE_VALUES] as const, urlKey: undefined } }),"
        );
    }

    #[test]
    fn multi_scalar_filter_with_url_sync_emits_params_pair() {
        let out = emit_filters_block(
            &[filter("tags", "Text", FilterCardinality::Multi, true)],
            &surface(),
        );

        assert_eq!(
            out,
            "filters: useFilterState({ tags: { mode: \"multi\", urlKey: \"tags\", params, setParams } }),"
        );
    }

    #[test]
    fn enum_value_imports_are_deduplicated_and_sorted() {
        let filters = vec![
            filter("type", "ItemType", FilterCardinality::Single, false),
            filter("status", "ItemStatus", FilterCardinality::Single, false),
            filter("other", "ItemType", FilterCardinality::Single, false),
        ];

        assert_eq!(
            enum_value_imports(&filters),
            vec![
                "ITEM_STATUS_VALUES".to_owned(),
                "ITEM_TYPE_VALUES".to_owned()
            ]
        );
    }

    #[test]
    fn scalar_filter_omits_values_field() {
        let out = emit_filters_block(
            &[filter("slug", "Text", FilterCardinality::Single, false)],
            &surface(),
        );

        assert!(!out.contains("values:"));
        assert!(out.contains("slug: { mode: \"single\", urlKey: undefined }"));
    }

    #[test]
    fn enum_constant_name_splits_camel_case() {
        assert_eq!(enum_value_constant_name("ItemType"), "ITEM_TYPE_VALUES");
        assert_eq!(
            enum_value_constant_name("catalog.ItemStatus"),
            "ITEM_STATUS_VALUES"
        );
        assert_eq!(enum_value_constant_name("Confidence"), "CONFIDENCE_VALUES");
    }

    #[test]
    fn date_range_filter_emits_paired_from_to_keys() {
        let out = emit_filters_block(
            &[filter(
                "created",
                "Date",
                FilterCardinality::DateRange,
                false,
            )],
            &surface(),
        );

        assert_eq!(
            out,
            "filters: useFilterState({ created: { mode: \"date_range\", fromKey: \"created_from\", toKey: \"created_to\" } }),"
        );
    }

    #[test]
    fn date_range_filter_with_url_sync_threads_params_pair() {
        let out = emit_filters_block(
            &[filter(
                "created",
                "DateTime",
                FilterCardinality::DateRange,
                true,
            )],
            &surface(),
        );

        assert_eq!(
            out,
            "filters: useFilterState({ created: { mode: \"date_range\", fromKey: \"created_from\", toKey: \"created_to\", params, setParams } }),"
        );
    }

    #[test]
    fn date_range_filter_omits_enum_value_imports() {
        let filters = vec![filter(
            "created",
            "Date",
            FilterCardinality::DateRange,
            false,
        )];
        assert!(enum_value_imports(&filters).is_empty());
    }

    #[test]
    fn detects_url_synced_filters() {
        let filters = vec![
            filter("type", "ItemType", FilterCardinality::Single, false),
            filter("tags", "Text", FilterCardinality::Multi, true),
        ];

        assert!(has_url_synced_filters(&filters));
    }
}
