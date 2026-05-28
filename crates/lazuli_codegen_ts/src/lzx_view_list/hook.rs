//! `use<Audience><View>View` hook — bundles the source query, action
//! mutations, filter state, search state, drawer sub-view, and any
//! `useMultiSelection` plumbing into one return shape.
//!
//! The hook is where most of the dynamic decisions live: should we
//! call `useMultiSelection`? Was a drawer declared? Are filters
//! URL-synced? Each decision drives both an early helper call near
//! the top of the function body and a corresponding return field.
//!
//! The `cellClick` callback is the trickiest pivot: with a drawer +
//! multi-selection, it has to triage shift-click range selection,
//! cmd/ctrl-click toggle, "additive while a selection exists" toggle,
//! and finally the "open the drawer" fallthrough. With single
//! selection it's a one-line forward to `drawer.open(id)`. Without a
//! drawer at all, `cellClick` isn't emitted.

use std::fmt::Write;

use crate::lzx::{
    Audience, CommandRef, SearchMode, SelectionMode, Surface, ViewList, command_action_key,
    command_ident, lzx_aux, lzx_filters, lzx_search, pascal_case, query_ident, view_hook_name,
    view_spec_const,
};

pub(super) fn write_hook(s: &mut String, audience: &Audience, view: &ViewList, surface: &Surface) {
    let const_name = view_spec_const(&audience.name, &view.name);
    let hook_name = view_hook_name(&audience.name, &view.name);
    let feature_pascal = pascal_case(&surface.feature);
    let has_filters = !view.filter.is_empty();
    let has_url_synced_filters = lzx_filters::has_url_synced_filters(&view.filter);
    let has_search = view.search.is_some();
    let is_columns_search = matches!(
        view.search.as_ref().map(|d| &d.mode),
        Some(SearchMode::Columns { .. })
    );

    writeln!(s, "export function {}(", hook_name).ok();
    writeln!(
        s,
        "  options: UseLazuliQueryOptions<{{}}, {}[]> = {{}},",
        feature_pascal
    )
    .ok();
    writeln!(s, ") {{").ok();

    if has_url_synced_filters {
        // TODO: useUrlParams() ships with @lazuli/runtime/react alongside
        // this emitter — see follow-up issue.
        writeln!(s, "  const [params, setParams] = useUrlParams();").ok();
    }
    if has_filters {
        s.push_str(&lzx_filters::emit_filters_const(&view.filter, surface));
    }
    if has_search {
        lzx_search::emit_hook_setup(s, view);
    }

    writeln!(
        s,
        "  const query = useLazuliQuery({}.source, {}, options);",
        const_name,
        format_query_input(view, has_filters, is_columns_search)
    )
    .ok();
    lzx_aux::write_hook_state(s, surface, view);
    if is_multi_selection(view) && view.selection.is_none() {
        // Currently unreachable (is_multi_selection already implies
        // view.selection.is_some()), but kept in sync with the primary
        // emission in lzx_aux::write_selection_state so a future
        // refactor doesn't reintroduce the `<string>` hardcode.
        writeln!(
            s,
            "  const selection = useMultiSelection<{}[\"id\"]>(query.data ?? []);",
            pascal_case(&surface.feature)
        )
        .ok();
    }
    if view.drawer.is_some() {
        writeln!(s, "  const routerState = useRouterState();").ok();
        writeln!(
            s,
            "  const [drawerId, setDrawerId] = useState<string | null>(null);"
        )
        .ok();
        if is_multi_selection(view) {
            writeln!(
                s,
                "  const [lastSelectedId, setLastSelectedId] = useState<string | null>(null);"
            )
            .ok();
        }
    }

    for cmd in &view.actions {
        let key = command_action_key(cmd);
        // `delete` is reserved → suffix `_`. Match the proposal §6.1 shape.
        let bind = if key == "delete" {
            "delete_".to_owned()
        } else {
            key.clone()
        };
        writeln!(
            s,
            "  const {} = useLazuliCommand({}.actions.{});",
            bind, const_name, key
        )
        .ok();
    }
    if let Some(delete_cmd) = drawer_delete_action(view)
        && !view
            .actions
            .iter()
            .any(|cmd| command_action_key(cmd) == "delete")
    {
        writeln!(
            s,
            "  const drawerDelete = useLazuliCommand({});",
            command_ident(delete_cmd)
        )
        .ok();
    }
    write_drawer_state(s, view);
    // Wave-W6 view-level UX primitives (wizard_steps / tab_group /
    // view_mode / view.inline_table).
    if !view.ux.is_empty() {
        s.push_str(&crate::lzx::lzx_ux::emit_ux_const(&view.ux));
    }

    writeln!(s).ok();
    writeln!(s, "  return {{").ok();
    writeln!(s, "    query,").ok();
    if has_filters {
        writeln!(s, "    filters,").ok();
    }
    lzx_search::emit_return_field(s, view.search.as_ref());
    if !view.actions.is_empty() {
        let parts: Vec<String> = view
            .actions
            .iter()
            .map(|c| {
                let key = command_action_key(c);
                if key == "delete" {
                    "delete: delete_".to_owned()
                } else {
                    key
                }
            })
            .collect();
        writeln!(s, "    actions: {{ {} }},", parts.join(", ")).ok();
    }
    lzx_aux::write_return_fields(s, view);
    crate::lzx::lzx_ux::emit_ux_return_fields(s, &view.ux);
    if view.drawer.is_some() {
        writeln!(s, "    drawer,").ok();
        writeln!(s, "    cellClick,").ok();
    }
    writeln!(s, "    meta: {},", const_name).ok();
    writeln!(s, "  }} as const;").ok();
    writeln!(s, "}}").ok();
}

fn format_query_input(view: &ViewList, has_filters: bool, is_columns_search: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if has_filters {
        for filter in &view.filter {
            parts.push(format!("{}: filters.{}.value", filter.name, filter.name));
        }
    }
    if is_columns_search {
        parts.push("q: searchRaw".to_owned());
    }
    if parts.is_empty() {
        "{}".to_owned()
    } else {
        format!("{{ {} }}", parts.join(", "))
    }
}

pub(super) fn is_multi_selection(view: &ViewList) -> bool {
    matches!(
        view.selection.as_ref().map(|selection| selection.mode),
        Some(SelectionMode::Multi)
    )
}

pub(super) fn drawer_delete_action(view: &ViewList) -> Option<&CommandRef> {
    view.drawer.as_ref().and_then(|drawer| {
        drawer
            .actions
            .iter()
            .find(|cmd| command_action_key(cmd) == "delete")
    })
}

fn delete_success_expr(view: &ViewList) -> &'static str {
    if drawer_delete_action(view).is_none() {
        "null"
    } else if view
        .actions
        .iter()
        .any(|cmd| command_action_key(cmd) == "delete")
    {
        "delete_.isSuccess ? delete_.submittedAt : null"
    } else {
        "drawerDelete.isSuccess ? drawerDelete.submittedAt : null"
    }
}

fn write_drawer_state(s: &mut String, view: &ViewList) {
    let Some(drawer) = &view.drawer else {
        return;
    };

    let input = drawer
        .route_binding
        .as_ref()
        .map(|binding| format!("{{ {}: drawerId ?? \"\" }}", binding.target))
        .unwrap_or_else(|| "{ id: drawerId ?? \"\" }".to_owned());
    let drawer_source = query_ident(&drawer.source);
    let selection_contains = if is_multi_selection(view) {
        "drawerId !== null ? selection.has(drawerId) : false"
    } else {
        "undefined"
    };

    writeln!(
        s,
        "  const drawerSubQuery = useLazuliQuery({}, {}, {{ enabled: drawerId !== null }});",
        drawer_source, input
    )
    .ok();
    writeln!(s, "  const drawerState = useDrawerSubView({{").ok();
    writeln!(s, "    item: drawerSubQuery.data,").ok();
    writeln!(
        s,
        "    itemMissing: !drawerSubQuery.isLoading && drawerSubQuery.data === null,"
    )
    .ok();
    writeln!(s, "    pathname: routerState.location.pathname,").ok();
    writeln!(s, "    lastDeleteSuccess: {},", delete_success_expr(view)).ok();
    writeln!(s, "    selectionContainsOpenId: {},", selection_contains).ok();
    writeln!(s, "  }});").ok();
    writeln!(s, "  const drawer = {{").ok();
    writeln!(s, "    ...drawerState,").ok();
    writeln!(s, "    id: drawerId,").ok();
    writeln!(s, "    isOpen: drawerState.isOpen,").ok();
    writeln!(
        s,
        "    item: drawerId !== null ? (drawerSubQuery.data ?? null) : null,"
    )
    .ok();
    writeln!(
        s,
        "    open: (id: string) => {{ setDrawerId(id); drawerState.open(id); }},"
    )
    .ok();
    writeln!(
        s,
        "    close: () => {{ setDrawerId(null); drawerState.close(); }},"
    )
    .ok();
    writeln!(s, "  }};").ok();
    if is_multi_selection(view) {
        writeln!(
            s,
            "  const cellClick = useCallback((id: string, event: React.MouseEvent) => {{"
        )
        .ok();
        writeln!(
            s,
            "    if (event.shiftKey && lastSelectedId !== null) {{ selection.selectRange(lastSelectedId, id); setLastSelectedId(id); return; }}"
        )
        .ok();
        writeln!(
            s,
            "    if (event.metaKey || event.ctrlKey) {{ selection.toggle(id); setLastSelectedId(id); return; }}"
        )
        .ok();
        writeln!(
            s,
            "    if (selection.ids.size > 0) {{ selection.toggle(id); setLastSelectedId(id); return; }}"
        )
        .ok();
        writeln!(s, "    drawer.open(id); setLastSelectedId(id);").ok();
        writeln!(s, "  }}, [selection, drawer, lastSelectedId]);").ok();
    } else {
        writeln!(
            s,
            "  const cellClick = useCallback((id: string, _event: React.MouseEvent) => {{"
        )
        .ok();
        writeln!(s, "    drawer.open(id);").ok();
        writeln!(s, "  }}, [drawer]);").ok();
    }
}
