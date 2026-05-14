//! Search-field emitter for `.lzx view list` hooks.
//!
//! `SearchMode::Columns` preserves the v1 free-text field shape. The
//! segmented mode is only wiring: generated hooks call `search-query-parser`
//! plus the runtime `canonicalizeSearch` / `parseSegments` helpers.

use std::fmt::Write;

use crate::lzx::{
    BindingRef, FilterCardinality, SearchDecl, SearchField, SearchMode, ViewList,
    format_string_array,
};

pub(crate) fn needs_react_hooks(search: Option<&SearchDecl>) -> bool {
    search.is_some()
}

pub(crate) fn needs_segmented_imports(search: Option<&SearchDecl>) -> bool {
    matches!(search.map(|decl| &decl.mode), Some(SearchMode::Segmented))
}

pub(crate) fn emit_hook_setup(s: &mut String, view: &ViewList) {
    let Some(search) = &view.search else {
        return;
    };

    match &search.mode {
        SearchMode::Columns { .. } => emit_columns_setup(s),
        SearchMode::Segmented => emit_segmented_setup(s, view, search),
    }
}

pub(crate) fn emit_return_field(s: &mut String, search: Option<&SearchDecl>) {
    let Some(search) = search else {
        return;
    };

    match &search.mode {
        SearchMode::Columns { .. } => {
            writeln!(
                s,
                "    search: {{ raw: searchRaw, setRaw: setRawSearch, clear: () => setRawSearch(\"\") }},"
            )
            .ok();
        }
        SearchMode::Segmented => {
            writeln!(s, "    search: {{").ok();
            writeln!(s, "      raw: searchRaw,").ok();
            writeln!(s, "      derivedFromFilters,").ok();
            writeln!(
                s,
                "      segments: parseSegments(searchRaw, SEARCH_KEYWORDS, SEARCH_ALWAYS_ARRAY),"
            )
            .ok();
            writeln!(s, "      setRaw: setRawSearch,").ok();
            writeln!(s, "      clear: () => setRawSearch(\"\"),").ok();
            writeln!(s, "    }},").ok();
        }
    }
}

fn emit_columns_setup(s: &mut String) {
    writeln!(s, "  const [searchRaw, setSearchRaw] = useState(\"\");").ok();
    writeln!(
        s,
        "  const setRawSearch = useCallback((input: string) => {{"
    )
    .ok();
    writeln!(s, "    setSearchRaw(input);").ok();
    writeln!(s, "  }}, []);").ok();
    writeln!(s).ok();
}

fn emit_segmented_setup(s: &mut String, view: &ViewList, search: &SearchDecl) {
    let keywords = search_keywords(search);
    let always_array = always_array_keywords(view, search);

    writeln!(
        s,
        "  const SEARCH_KEYWORDS = {} as const;",
        format_string_array(&keywords)
    )
    .ok();
    writeln!(
        s,
        "  const SEARCH_ALWAYS_ARRAY = {} as const;",
        format_string_array(&always_array)
    )
    .ok();
    writeln!(s, "  const [searchRaw, setSearchRaw] = useState(\"\");").ok();
    writeln!(
        s,
        "  const setRawSearch = useCallback((input: string) => {{"
    )
    .ok();
    writeln!(s, "    setSearchRaw(input);").ok();
    writeln!(
        s,
        "    const parsed = parseSearchQuery(input, {{ keywords: SEARCH_KEYWORDS, alwaysArray: SEARCH_ALWAYS_ARRAY }} as unknown as SearchParserOptions) as string | SearchParserResult;"
    )
    .ok();
    writeln!(s, "    if (typeof parsed !== \"string\") {{").ok();
    for field in &search.fields {
        emit_parsed_field_setter(s, view, field);
    }
    emit_free_text_setter(s, search);
    writeln!(s, "    }}").ok();
    writeln!(s, "  }}, [filters]);").ok();

    writeln!(
        s,
        "  const derivedFromFilters = useMemo(() => canonicalizeSearch({{"
    )
    .ok();
    for field in &search.fields {
        if let BindingRef::Filter { name } = &field.binds_to {
            writeln!(s, "    {}: filters.{}.value,", field.key, name).ok();
        }
    }
    writeln!(
        s,
        "  }}, {}), [{}]);",
        free_text_expr(search),
        dependency_list(search)
    )
    .ok();
    writeln!(s).ok();
}

fn search_keywords(search: &SearchDecl) -> Vec<String> {
    search
        .fields
        .iter()
        .map(|field| field.key.clone())
        .collect()
}

fn always_array_keywords(view: &ViewList, search: &SearchDecl) -> Vec<String> {
    search
        .fields
        .iter()
        .filter_map(|field| match &field.binds_to {
            BindingRef::Filter { name } if is_multi_filter(view, name) => Some(field.key.clone()),
            _ => None,
        })
        .collect()
}

fn is_multi_filter(view: &ViewList, name: &str) -> bool {
    view.filter
        .iter()
        .any(|filter| filter.name == name && filter.cardinality == FilterCardinality::Multi)
}

fn filter_type_ref<'a>(view: &'a ViewList, name: &str) -> Option<&'a str> {
    view.filter
        .iter()
        .find(|filter| filter.name == name)
        .map(|filter| filter.type_ref.as_str())
}

fn emit_parsed_field_setter(s: &mut String, view: &ViewList, field: &SearchField) {
    let BindingRef::Filter { name } = &field.binds_to else {
        return;
    };
    let parsed_access = format!("parsed.{}", field.key);
    if is_multi_filter(view, name) {
        let cast = scalar_cast(view, name);
        writeln!(s, "      if ({parsed_access}) {{").ok();
        writeln!(
            s,
            "        filters.{}.set((Array.isArray({}) ? {} : [{}]).map((value) => value as {}));",
            name, parsed_access, parsed_access, parsed_access, cast
        )
        .ok();
        writeln!(s, "      }}").ok();
    } else {
        writeln!(
            s,
            "      if ({}) filters.{}.set({} as {});",
            parsed_access,
            name,
            parsed_access,
            scalar_cast(view, name)
        )
        .ok();
    }
}

fn scalar_cast(view: &ViewList, filter_name: &str) -> String {
    match filter_type_ref(view, filter_name) {
        Some("Text" | "String" | "") | None => "string".to_owned(),
        Some(type_ref) => type_ref.to_owned(),
    }
}

fn emit_free_text_setter(s: &mut String, search: &SearchDecl) {
    if let Some(BindingRef::SourceInput { name }) = &search.free_text_target {
        writeln!(
            s,
            "      if (typeof parsed.text === \"string\") source.{}.set(parsed.text);",
            name
        )
        .ok();
    }
}

fn free_text_expr(search: &SearchDecl) -> String {
    match &search.free_text_target {
        Some(BindingRef::SourceInput { name }) => format!("source.{}.value", name),
        _ => "\"\"".to_owned(),
    }
}

fn dependency_list(search: &SearchDecl) -> String {
    let mut deps: Vec<String> = search
        .fields
        .iter()
        .filter_map(|field| match &field.binds_to {
            BindingRef::Filter { name } => Some(format!("filters.{}.value", name)),
            _ => None,
        })
        .collect();
    if let Some(BindingRef::SourceInput { name }) = &search.free_text_target {
        deps.push(format!("source.{}.value", name));
    }
    deps.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::ir::*;

    fn segmented_view(free_text_target: Option<BindingRef>) -> ViewList {
        ViewList {
            name: "item_terminal".to_owned(),
            route: None,
            source: QueryRef {
                feature: "item".to_owned(),
                kind: QueryKind::List,
                name: "search".to_owned(),
            },
            render: ListRender::Cells {
                slot: "item_card".to_owned(),
            },
            search: Some(SearchDecl {
                mode: SearchMode::Segmented,
                fields: vec![
                    SearchField {
                        key: "slug".to_owned(),
                        binds_to: BindingRef::Filter {
                            name: "slug".to_owned(),
                        },
                        span_ref: None,
                    },
                    SearchField {
                        key: "type".to_owned(),
                        binds_to: BindingRef::Filter {
                            name: "type".to_owned(),
                        },
                        span_ref: None,
                    },
                    SearchField {
                        key: "tag".to_owned(),
                        binds_to: BindingRef::Filter {
                            name: "tags".to_owned(),
                        },
                        span_ref: None,
                    },
                ],
                free_text_target,
                span_ref: None,
            }),
            filter: vec![
                FilterDecl {
                    name: "slug".to_owned(),
                    type_ref: "Text".to_owned(),
                    cardinality: FilterCardinality::Single,
                    url_sync: false,
                    span_ref: None,
                },
                FilterDecl {
                    name: "type".to_owned(),
                    type_ref: "ItemType".to_owned(),
                    cardinality: FilterCardinality::Single,
                    url_sync: false,
                    span_ref: None,
                },
                FilterDecl {
                    name: "tags".to_owned(),
                    type_ref: "Text".to_owned(),
                    cardinality: FilterCardinality::Multi,
                    url_sync: false,
                    span_ref: None,
                },
            ],
            cells: vec![],
            actions: vec![],
            drawer: None,
            sort: None,
            selection: None,
            settings: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn columns_mode_emits_v1_free_text_shape() {
        let view = ViewList {
            search: Some(SearchDecl {
                mode: SearchMode::Columns {
                    columns: vec!["key".to_owned(), "title".to_owned()],
                },
                fields: vec![],
                free_text_target: None,
                span_ref: None,
            }),
            ..segmented_view(None)
        };
        let mut body = String::new();
        emit_hook_setup(&mut body, &view);
        emit_return_field(&mut body, view.search.as_ref());

        assert!(body.contains("const [searchRaw, setSearchRaw] = useState(\"\");"));
        assert!(body.contains(
            "search: { raw: searchRaw, setRaw: setRawSearch, clear: () => setRawSearch(\"\") },"
        ));
        assert!(!body.contains("parseSegments"));
        assert!(!body.contains("derivedFromFilters"));
    }

    #[test]
    fn segmented_mode_emits_keywords_and_multi_always_array() {
        let view = segmented_view(None);
        let mut body = String::new();
        emit_hook_setup(&mut body, &view);

        assert!(body.contains("const SEARCH_KEYWORDS = [\"slug\", \"type\", \"tag\"] as const;"));
        assert!(body.contains("const SEARCH_ALWAYS_ARRAY = [\"tag\"] as const;"));
    }

    #[test]
    fn segmented_mode_sets_single_and_typed_filters() {
        let view = segmented_view(None);
        let mut body = String::new();
        emit_hook_setup(&mut body, &view);

        assert!(body.contains("if (parsed.slug) filters.slug.set(parsed.slug as string);"));
        assert!(body.contains("if (parsed.type) filters.type.set(parsed.type as ItemType);"));
    }

    #[test]
    fn segmented_mode_flattens_multi_value_parser_output() {
        let view = segmented_view(None);
        let mut body = String::new();
        emit_hook_setup(&mut body, &view);

        assert!(body.contains(
            "filters.tags.set((Array.isArray(parsed.tag) ? parsed.tag : [parsed.tag]).map((value) => value as string));"
        ));
    }

    #[test]
    fn free_text_target_some_reads_source_input_for_canonicalize() {
        let view = segmented_view(Some(BindingRef::SourceInput {
            name: "q".to_owned(),
        }));
        let mut body = String::new();
        emit_hook_setup(&mut body, &view);

        assert!(body.contains("source.q.set(parsed.text);"));
        assert!(body.contains("}, source.q.value), [filters.slug.value, filters.type.value, filters.tags.value, source.q.value]);"));
    }

    #[test]
    fn free_text_target_none_uses_empty_free_text() {
        let view = segmented_view(None);
        let mut body = String::new();
        emit_hook_setup(&mut body, &view);

        assert!(
            body.contains(
                "}, \"\"), [filters.slug.value, filters.type.value, filters.tags.value]);"
            )
        );
        assert!(!body.contains("source.q"));
    }

    #[test]
    fn segmented_return_field_uses_runtime_parse_segments_signature() {
        let view = segmented_view(None);
        let mut body = String::new();
        emit_return_field(&mut body, view.search.as_ref());

        assert!(
            body.contains(
                "segments: parseSegments(searchRaw, SEARCH_KEYWORDS, SEARCH_ALWAYS_ARRAY),"
            )
        );
        assert!(body.contains("derivedFromFilters,"));
    }
}
