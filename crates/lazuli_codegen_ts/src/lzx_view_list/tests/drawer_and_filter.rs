//! Drawer-dispatch + filter/search emission tests, sub-grouped from
//! `lzx_view_list/tests/mod.rs` to keep each file under 500 LOC.

use super::super::*;
use super::*;
use crate::lzx::CommandRef;
use crate::lzx::ir::*;

#[test]
fn drawer_with_multi_selection_emits_dispatcher_branches() {
    let mut view = minimal_view_list();
    view.drawer = Some(drawer());
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![],
        span_ref: None,
    });
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    assert!(out.contains("useDrawerSubView"));
    assert!(out.contains("useMultiSelection"));
    // Resource ID type is threaded via indexed-access on the
    // surface feature interface (`Thing["id"]`).
    assert!(out.contains("const selection = useMultiSelection<Thing[\"id\"]>(query.data ?? [])"));
    assert!(
        out.contains("const cellClick = useCallback((id: string, event: React.MouseEvent) => {")
    );
    assert!(out.contains("if (event.shiftKey && lastSelectedId !== null) { selection.selectRange(lastSelectedId, id); setLastSelectedId(id); return; }"));
    assert!(out.contains("if (event.metaKey || event.ctrlKey) { selection.toggle(id); setLastSelectedId(id); return; }"));
    assert!(out.contains(
        "if (selection.ids.size > 0) { selection.toggle(id); setLastSelectedId(id); return; }"
    ));
    assert!(out.contains("drawer.open(id); setLastSelectedId(id);"));
    assert!(
        out.contains(
            "selectionContainsOpenId: drawerId !== null ? selection.has(drawerId) : false"
        )
    );
    assert!(out.contains("drawer,"));
    assert!(out.contains("cellClick,"));
}

#[test]
fn drawer_with_single_selection_emits_simple_dispatcher() {
    let mut view = minimal_view_list();
    view.drawer = Some(drawer());
    view.selection = Some(SelectionDecl {
        mode: SelectionMode::Single,
        bulk_actions: vec![],
        span_ref: None,
    });
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    assert!(
        out.contains("const cellClick = useCallback((id: string, _event: React.MouseEvent) => {")
    );
    assert!(out.contains("    drawer.open(id);"));
    assert!(!out.contains("event.shiftKey"));
    assert!(!out.contains("event.metaKey || event.ctrlKey"));
    assert!(!out.contains("selection.ids.size > 0"));
    assert!(!out.contains("useMultiSelection"));
}

#[test]
fn drawer_none_emits_no_drawer_or_cell_click_fields() {
    let view = minimal_view_list();
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    assert!(!out.contains("useDrawerSubView"));
    assert!(!out.contains("drawerSubQuery"));
    assert!(!out.contains("const drawer ="));
    assert!(!out.contains("cellClick"));
    assert!(!out.contains("useRouterState"));
}

#[test]
fn drawer_delete_action_threads_last_delete_success() {
    let mut drawer = drawer();
    drawer.actions = vec![delete_thing_ref()];
    let mut view = minimal_view_list();
    view.drawer = Some(drawer);
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    assert!(out.contains("deleteThing"));
    assert!(out.contains("const drawerDelete = useLazuliCommand(deleteThing);"));
    assert!(
        out.contains("lastDeleteSuccess: drawerDelete.isSuccess ? drawerDelete.submittedAt : null")
    );
}

#[test]
fn drawer_route_binding_fills_subquery_input_target() {
    let mut drawer = drawer();
    drawer.route_binding = Some(DrawerRouteBinding {
        target: "key".to_owned(),
        source: DrawerBindingSource::Selection,
    });
    let mut view = minimal_view_list();
    view.drawer = Some(drawer);
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    assert!(out.contains("lookupThingByID"));
    assert!(out.contains("const drawerSubQuery = useLazuliQuery(lookupThingByID, { key: drawerId ?? \"\" }, { enabled: drawerId !== null });"));
}

#[test]
fn filter_decl_emits_use_filter_state_call() {
    let mut view = minimal_view_list();
    view.filter = vec![FilterDecl {
        name: "tags".to_owned(),
        type_ref: "Text".to_owned(),
        cardinality: FilterCardinality::Single,
        url_sync: false,
        span_ref: None,
    }];
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    // useFilterState imported from runtime.
    assert!(out.contains("useFilterState,"));
    // Hook declares `const filters = useFilterState(...)`.
    assert!(out.contains(
        "const filters = useFilterState({ tags: { mode: \"single\", urlKey: undefined } });"
    ));
    // Return shape includes `filters,`.
    assert!(out.contains("    filters,\n"));
}

#[test]
fn segmented_search_emits_parse_segments_and_canonicalize() {
    let mut view = minimal_view_list();
    view.filter = vec![
        FilterDecl {
            name: "slug".to_owned(),
            type_ref: "Text".to_owned(),
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
    ];
    view.search = Some(SearchDecl {
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
                key: "tag".to_owned(),
                binds_to: BindingRef::Filter {
                    name: "tags".to_owned(),
                },
                span_ref: None,
            },
        ],
        free_text_target: None,
        span_ref: None,
    });
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    // search-query-parser pulled in.
    assert!(out.contains(
        "import { parse as parseSearchQuery, type SearchParserResult, type SearchParserOptions } from \"search-query-parser\";"
    ));
    // Runtime imports include parseSegments + canonicalizeSearch.
    assert!(out.contains("  parseSegments,"));
    assert!(out.contains("  canonicalizeSearch,"));
    // Hook setup invokes parseSearchQuery, canonicalizeSearch.
    assert!(out.contains("parseSearchQuery(input,"));
    assert!(out.contains("canonicalizeSearch({"));
    // Return field surfaces segments via parseSegments.
    assert!(
        out.contains("segments: parseSegments(searchRaw, SEARCH_KEYWORDS, SEARCH_ALWAYS_ARRAY),")
    );
}

#[test]
fn enum_filter_imports_values_const() {
    let mut view = minimal_view_list();
    view.filter = vec![FilterDecl {
        name: "type".to_owned(),
        type_ref: "ItemType".to_owned(),
        cardinality: FilterCardinality::Single,
        url_sync: false,
        span_ref: None,
    }];
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    // Feature SDK import block includes the enum VALUES constant.
    assert!(out.contains("  ITEM_TYPE_VALUES,"));
    // Filter config references it.
    assert!(out.contains("values: [...ITEM_TYPE_VALUES] as const"));
}

#[test]
fn use_lazuli_query_threads_filter_values() {
    let mut view = minimal_view_list();
    view.filter = vec![
        FilterDecl {
            name: "tags".to_owned(),
            type_ref: "Text".to_owned(),
            cardinality: FilterCardinality::Multi,
            url_sync: false,
            span_ref: None,
        },
        FilterDecl {
            name: "kind".to_owned(),
            type_ref: "Text".to_owned(),
            cardinality: FilterCardinality::Single,
            url_sync: false,
            span_ref: None,
        },
    ];
    let audience = minimal_audience(view.clone());
    let surface = minimal_surface(audience.clone());

    let out = emit_view_list(&surface, &audience, &view, "");

    // useLazuliQuery receives the filter values in declaration order.
    assert!(out.contains(
        "const query = useLazuliQuery(viewerThingListView.source, { tags: filters.tags.value, kind: filters.kind.value }, options);"
    ));
}
