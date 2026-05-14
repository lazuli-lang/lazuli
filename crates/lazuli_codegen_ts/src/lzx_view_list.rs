//! `view list` emitter — emits a typed `<audience><View>View` spec const
//! + `use<Audience><View>View` hook bundling the source query and all
//! action mutations, plus a slot interface for any `cells <field>
//! @client.<slot>` bindings.
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §6.1.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::lzx::{
    Audience, CommandRef, Surface, ViewList, audience_view_pascal, banner, command_action_key,
    command_ident, format_cells_literal, format_string_array, lower_camel, pascal_case, query_ident,
    view_hook_name, view_spec_const,
};
use crate::lzx::lzx_aux;
use crate::lzx::lzx_filters;
use crate::lzx::lzx_search;
use crate::lzx::{ListRender, SearchDecl, SearchMode, SelectionMode};

/// Emit `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`.
pub fn emit_view_list(
    surface: &Surface,
    audience: &Audience,
    view: &ViewList,
    app_name: &str,
) -> String {
    let mut s = String::new();
    s.push_str(banner());

    write_imports(&mut s, surface, view);
    lzx_aux::write_setting_keys(&mut s, surface, view, app_name);
    write_spec_const(&mut s, audience, view);
    write_column_assert(&mut s, audience, view, surface);
    write_slot_interface(&mut s, audience, view, surface);
    write_hook(&mut s, audience, view, surface);

    s
}

fn write_imports(s: &mut String, surface: &Surface, view: &ViewList) {
    let feature_pascal = pascal_case(&surface.feature);
    let has_drawer = view.drawer.is_some();
    let has_drawer_delete = drawer_delete_action(view).is_some();
    let has_multi_selection = is_multi_selection(view);
    let has_filters = !view.filter.is_empty();
    let has_search = view.search.is_some();
    let has_segmented_search = lzx_search::needs_segmented_imports(view.search.as_ref());
    let has_url_synced_filters = lzx_filters::has_url_synced_filters(&view.filter);

    // 1. Runtime hooks.
    writeln!(s, "import {{").ok();
    writeln!(s, "  useLazuliQuery,").ok();
    if !view.actions.is_empty() || has_drawer_delete || lzx_aux::needs_bulk_commands(view) {
        writeln!(s, "  useLazuliCommand,").ok();
    }
    if has_drawer {
        writeln!(s, "  useDrawerSubView,").ok();
    }
    if has_multi_selection || lzx_aux::needs_multi_selection(view) {
        writeln!(s, "  useMultiSelection,").ok();
    }
    if lzx_aux::needs_local_setting(view) {
        writeln!(s, "  useLocalSetting,").ok();
    }
    if has_filters {
        writeln!(s, "  useFilterState,").ok();
    }
    if has_url_synced_filters {
        // TODO: introduce useUrlParams() helper in @lazuli/runtime/react.
        writeln!(s, "  useUrlParams,").ok();
    }
    if has_segmented_search {
        writeln!(s, "  parseSegments,").ok();
        writeln!(s, "  canonicalizeSearch,").ok();
    }
    writeln!(s, "  type UseLazuliQueryOptions,").ok();
    writeln!(s, "}} from \"@lazuli/runtime/react\";").ok();

    // React hooks — combine search-driven needs with the existing
    // drawer/aux state needs so we never emit two `import ... from "react"`
    // lines.
    let needs_use_state = has_drawer || lzx_aux::needs_use_state(view) || has_search;
    let needs_use_callback = has_drawer || has_search;
    let needs_use_memo = has_segmented_search;
    let needs_react_type = has_drawer || !view.cells.is_empty();
    if needs_use_state || needs_use_callback || needs_use_memo {
        let mut parts: Vec<&str> = Vec::new();
        if needs_use_callback {
            parts.push("useCallback");
        }
        if needs_use_memo {
            parts.push("useMemo");
        }
        if needs_use_state {
            parts.push("useState");
        }
        writeln!(s, "import {{ {} }} from \"react\";", parts.join(", ")).ok();
    }
    if needs_react_type {
        writeln!(s, "import type * as React from \"react\";").ok();
    }
    if has_drawer {
        writeln!(
            s,
            "import {{ useRouterState }} from \"@tanstack/react-router\";"
        )
        .ok();
    }
    if has_url_synced_filters {
        writeln!(
            s,
            "import {{ useSearch as useRouterSearch, useNavigate }} from \"@tanstack/react-router\";"
        )
        .ok();
    }
    if has_segmented_search {
        writeln!(
            s,
            "import {{ parse as parseSearchQuery, type SearchParserResult, type SearchParserOptions }} from \"search-query-parser\";"
        )
        .ok();
    }

    // 2. Feature SDK — resource type + source query + each action command.
    let mut sdk_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    sdk_imports
        .entry(view.source.feature.clone())
        .or_default()
        .insert(query_ident(&view.source));
    for cmd in &view.actions {
        sdk_imports
            .entry(cmd.feature.clone())
            .or_default()
            .insert(command_ident(cmd));
    }
    if let Some(drawer) = &view.drawer {
        sdk_imports
            .entry(drawer.source.feature.clone())
            .or_default()
            .insert(query_ident(&drawer.source));
        for cmd in &drawer.actions {
            sdk_imports
                .entry(cmd.feature.clone())
                .or_default()
                .insert(command_ident(cmd));
        }
    }
    for ident in lzx_aux::unique_bulk_command_imports(view) {
        sdk_imports
            .entry(surface.feature.clone())
            .or_default()
            .insert(ident);
    }
    for ident in lzx_filters::enum_value_imports(&view.filter) {
        sdk_imports
            .entry(surface.feature.clone())
            .or_default()
            .insert(ident);
    }
    sdk_imports
        .entry(surface.feature.clone())
        .or_default()
        .insert(format!("type {}", feature_pascal));

    for (feature, imports) in sdk_imports {
        writeln!(s, "import {{").ok();
        for import in imports {
            writeln!(s, "  {},", import).ok();
        }
        writeln!(s, "}} from \"../../{}.gen.js\";", feature).ok();
    }

    // 3. Cell slot prop types — one import per binding.
    for cell in &view.cells {
        let pascal_slot = pascal_case(&cell.slot);
        writeln!(
            s,
            "import type {{ {}Props }} from \"../../cells/{}.gen.js\";",
            pascal_slot, cell.slot
        )
        .ok();
    }
    s.push('\n');
}


fn write_spec_const(s: &mut String, audience: &Audience, view: &ViewList) {
    let const_name = view_spec_const(&audience.name, &view.name);
    let search_columns = view_search_columns(view);
    let filter_names = view_filter_names(view);

    writeln!(
        s,
        "// Compile-time view spec const. Frozen, type-checked against .lzx."
    )
    .ok();
    writeln!(s, "export const {} = {{", const_name).ok();
    writeln!(s, "  source: {},", query_ident(&view.source)).ok();
    match &view.render {
        ListRender::Table { columns } => {
            writeln!(
                s,
                "  columns: {} as const,",
                format_string_array(columns)
            )
            .ok();
        }
        ListRender::Cells { slot } => {
            writeln!(s, "  cells: \"@client.{}\" as const,", slot).ok();
        }
    }
    if !search_columns.is_empty() {
        writeln!(
            s,
            "  search: {} as const,",
            format_string_array(search_columns)
        )
        .ok();
    }
    if !filter_names.is_empty() {
        writeln!(
            s,
            "  filter: {} as const,",
            format_string_array(&filter_names)
        )
        .ok();
    }
    if matches!(&view.render, ListRender::Table { .. }) {
        writeln!(s, "  cells: {},", format_cells_literal(&view.cells)).ok();
    }
    if !view.actions.is_empty() {
        writeln!(s, "  actions: {},", format_actions_object(&view.actions)).ok();
    }
    if let Some(route) = &view.route {
        writeln!(s, "  route: \"{}\",", route).ok();
    }
    writeln!(s, "}} as const;").ok();
    s.push('\n');
}

fn view_search_columns(view: &ViewList) -> &[String] {
    match &view.search {
        Some(SearchDecl {
            mode: SearchMode::Columns { columns },
            ..
        }) => columns.as_slice(),
        _ => &[],
    }
}

fn view_filter_names(view: &ViewList) -> Vec<String> {
    view.filter
        .iter()
        .map(|filter| filter.name.clone())
        .collect()
}

fn format_actions_object(actions: &[CommandRef]) -> String {
    let parts: Vec<String> = actions
        .iter()
        .map(|c| format!("{}: {}", command_action_key(c), command_ident(c)))
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

fn write_column_assert(s: &mut String, audience: &Audience, view: &ViewList, surface: &Surface) {
    if !matches!(&view.render, ListRender::Table { .. }) {
        return;
    }

    let const_name = view_spec_const(&audience.name, &view.name);
    let feature_pascal = pascal_case(&surface.feature);

    writeln!(
        s,
        "// Compile-time guarantee: every column must be a {} field.",
        feature_pascal
    )
    .ok();
    writeln!(
        s,
        "type _AssertColumns = (typeof {}.columns)[number] extends keyof {}",
        const_name, feature_pascal
    )
    .ok();
    writeln!(s, "  ? true").ok();
    writeln!(s, "  : never;").ok();
    s.push('\n');
}

fn write_slot_interface(s: &mut String, audience: &Audience, view: &ViewList, surface: &Surface) {
    match &view.render {
        ListRender::Table { .. } => {
            if view.cells.is_empty() {
                return;
            }
            let iface = format!("{}Slots", audience_view_pascal(&audience.name, &view.name));
            writeln!(s, "// Slot binding contract.").ok();
            writeln!(s, "export interface {} {{", iface).ok();
            for cell in &view.cells {
                let pascal_slot = pascal_case(&cell.slot);
                writeln!(
                    s,
                    "  {}: React.ComponentType<{}Props>;",
                    pascal_slot, pascal_slot
                )
                .ok();
            }
        }
        ListRender::Cells { slot } => {
            let iface = format!("{}Cells", audience_view_pascal(&audience.name, &view.name));
            let pascal_slot = pascal_case(slot);
            let row_prop = lower_camel(&surface.feature);
            let row_type = pascal_case(&surface.feature);
            writeln!(s, "// Grid cell slot contract.").ok();
            writeln!(s, "export interface {} {{", iface).ok();
            writeln!(
                s,
                "  {}: React.ComponentType<{{ {}: {} }}>;",
                pascal_slot, row_prop, row_type
            )
            .ok();
        }
    }
    writeln!(s, "}}").ok();
    s.push('\n');
}

fn write_hook(s: &mut String, audience: &Audience, view: &ViewList, surface: &Surface) {
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

fn is_multi_selection(view: &ViewList) -> bool {
    matches!(
        view.selection.as_ref().map(|selection| selection.mode),
        Some(SelectionMode::Multi)
    )
}

fn drawer_delete_action(view: &ViewList) -> Option<&CommandRef> {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::ir::*;
    use crate::lzx::test_fixtures;

    fn minimal_view_list() -> ViewList {
        ViewList {
            name: "thing_list".to_owned(),
            route: None,
            source: QueryRef {
                feature: "thing".to_owned(),
                kind: QueryKind::List,
                name: "list".to_owned(),
            },
            render: ListRender::Table {
                columns: vec!["id".to_owned()],
            },
            search: None,
            filter: vec![],
            cells: vec![],
            actions: vec![],
            drawer: None,
            sort: None,
            selection: None,
            settings: vec![],
            span_ref: None,
        }
    }

    fn minimal_audience(view: ViewList) -> Audience {
        Audience {
            name: "viewer".to_owned(),
            requires: vec![],
            views: vec![View::List(view)],
            span_ref: None,
        }
    }

    fn minimal_surface(audience: Audience) -> Surface {
        Surface {
            feature: "thing".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![audience],
            span_ref: None,
        }
    }

    fn drawer() -> DrawerSubView {
        DrawerSubView {
            name: "thing_detail".to_owned(),
            trigger: DrawerTrigger::Select,
            source: QueryRef {
                feature: "thing".to_owned(),
                kind: QueryKind::Lookup,
                name: "by_id".to_owned(),
            },
            route_binding: Some(DrawerRouteBinding {
                target: "id".to_owned(),
                source: DrawerBindingSource::Selection,
            }),
            sections: vec![],
            cells: vec![],
            actions: vec![],
            span_ref: None,
        }
    }

    fn delete_thing_ref() -> CommandRef {
        CommandRef {
            feature: "thing".to_owned(),
            name: "delete".to_owned(),
        }
    }

    #[test]
    fn emits_minimal_fixture() {
        let view = minimal_view_list();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view, "");
        assert!(out.starts_with("// Code generated by lazuli; DO NOT EDIT.\n"));
        assert!(out.contains("export const viewerThingListView"));
        assert!(out.contains("export function useViewerThingListView"));
        // No actions → no useLazuliCommand import.
        assert!(!out.contains("useLazuliCommand"));
        // No cells → no slot interface.
        assert!(!out.contains("Slots"));
        // No route → spec const has no `route:` field.
        assert!(!out.contains("route:"));
    }

    #[test]
    fn emits_full_l0_3_section_13_1_fixture() {
        let surface = test_fixtures::slug_web_surface();
        let audience = &surface.audiences[0]; // admin
        let view = match &audience.views[0] {
            View::List(v) => v,
            _ => panic!("expected list view"),
        };

        let out = emit_view_list(&surface, audience, view, "");

        // Spec const + hook present.
        assert!(out.contains("export const adminSlugListView"));
        assert!(out.contains("export function useAdminSlugListView"));

        // Columns array preserved verbatim.
        assert!(out.contains("columns: [\"key\", \"title\", \"tags\", \"created_at\"] as const"));
        // Search + filter arrays.
        assert!(out.contains("search: [\"key\", \"title\"] as const"));
        assert!(out.contains("filter: [\"tags\"] as const"));
        // Cells literal.
        assert!(out.contains("cells: { tags: \"@client.type_badge\" as const }"));
        // Actions object — every command resolves to <verb><PascalFeature>.
        assert!(out.contains("actions: {"));
        assert!(out.contains("create: createSlug"));
        assert!(out.contains("update: updateSlug"));
        assert!(out.contains("delete: deleteSlug"));
        // Route string.
        assert!(out.contains("route: \"/slugs\""));

        // Source query identifier.
        assert!(out.contains("listMineSlugs"));

        // Column assertion.
        assert!(out.contains("type _AssertColumns"));
        assert!(out.contains("extends keyof Slug"));

        // Slot interface.
        assert!(out.contains("export interface AdminSlugListSlots"));
        assert!(out.contains("TypeBadge: React.ComponentType<TypeBadgeProps>"));

        // Hook body — delete is bound as `delete_` and re-exported as `delete: delete_`.
        assert!(out.contains("const delete_ = useLazuliCommand(adminSlugListView.actions.delete)"));
        assert!(out.contains("delete: delete_"));
    }

    #[test]
    fn generates_correct_hook_name_with_kebab_audience() {
        let mut view = minimal_view_list();
        view.name = "slug_list".to_owned();
        view.source.feature = "slug".to_owned();
        view.render = ListRender::Table {
            columns: vec!["key".to_owned()],
        };

        let audience = Audience {
            name: "workspace-admin".to_owned(),
            requires: vec![],
            views: vec![View::List(view.clone())],
            span_ref: None,
        };
        let surface = Surface {
            feature: "slug".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![audience.clone()],
            span_ref: None,
        };

        let out = emit_view_list(&surface, &audience, &view, "");
        assert!(out.contains("export function useWorkspaceAdminSlugListView"));
        assert!(out.contains("export const workspaceAdminSlugListView"));
    }

    #[test]
    fn slot_bindings_produce_parallel_import_lines() {
        let mut view = minimal_view_list();
        view.cells = vec![
            CellBinding {
                field: "tags".to_owned(),
                slot: "type_badge".to_owned(),
            },
            CellBinding {
                field: "owner".to_owned(),
                slot: "user_avatar".to_owned(),
            },
        ];
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view, "");
        assert!(
            out.contains("import type { TypeBadgeProps } from \"../../cells/type_badge.gen.js\";")
        );
        assert!(
            out.contains(
                "import type { UserAvatarProps } from \"../../cells/user_avatar.gen.js\";"
            )
        );
        // Slot interface includes both.
        assert!(out.contains("TypeBadge: React.ComponentType<TypeBadgeProps>"));
        assert!(out.contains("UserAvatar: React.ComponentType<UserAvatarProps>"));
    }

    #[test]
    fn cells_render_emits_grid_slot_interface() {
        let mut view = minimal_view_list();
        view.render = ListRender::Cells {
            slot: "item_card".to_owned(),
        };
        let audience = minimal_audience(view.clone());
        let mut surface = minimal_surface(audience.clone());
        surface.feature = "item".to_owned();

        let out = emit_view_list(&surface, &audience, &view, "");

        assert!(out.contains("export interface ViewerThingListCells"));
        assert!(out.contains("ItemCard: React.ComponentType<{ item: Item }>;"));
        assert!(!out.contains("export interface ViewerThingListSlots"));
    }

    #[test]
    fn cells_render_spec_uses_client_slot_literal() {
        let mut view = minimal_view_list();
        view.render = ListRender::Cells {
            slot: "item_card".to_owned(),
        };
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view, "");

        assert!(out.contains("cells: \"@client.item_card\" as const"));
        assert!(!out.contains("columns:"));
        assert!(!out.contains("type _AssertColumns"));
    }

    #[test]
    fn table_render_keeps_columns_and_column_assertion() {
        let view = minimal_view_list();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view, "");

        assert!(out.contains("columns: [\"id\"] as const"));
        assert!(out.contains("cells: {}"));
        assert!(out.contains("type _AssertColumns"));
        assert!(!out.contains("export interface ViewerThingListCells"));
    }

    #[test]
    fn multiple_actions_emit_as_object_literal() {
        let mut view = minimal_view_list();
        view.source.feature = "slug".to_owned();
        view.actions = vec![
            CommandRef {
                feature: "slug".to_owned(),
                name: "create".to_owned(),
            },
            CommandRef {
                feature: "slug".to_owned(),
                name: "update".to_owned(),
            },
            CommandRef {
                feature: "slug".to_owned(),
                name: "archive".to_owned(),
            },
        ];
        let audience = minimal_audience(view.clone());
        let mut surface = minimal_surface(audience.clone());
        surface.feature = "slug".to_owned();

        let out = emit_view_list(&surface, &audience, &view, "");
        // Object literal style + all three resolved identifiers.
        assert!(
            out.contains(
                "actions: { create: createSlug, update: updateSlug, archive: archiveSlug }"
            )
        );
        // Hook returns the spread actions map.
        assert!(out.contains("actions: { create, update, archive }"));
    }

    #[test]
    fn omits_search_filter_and_actions_when_empty() {
        let view = minimal_view_list();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view, "");
        assert!(!out.contains("search:"));
        assert!(!out.contains("filter:"));
        assert!(!out.contains("actions:"));
    }

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
            out.contains(
                "const cellClick = useCallback((id: string, event: React.MouseEvent) => {"
            )
        );
        assert!(out.contains("if (event.shiftKey && lastSelectedId !== null) { selection.selectRange(lastSelectedId, id); setLastSelectedId(id); return; }"));
        assert!(out.contains("if (event.metaKey || event.ctrlKey) { selection.toggle(id); setLastSelectedId(id); return; }"));
        assert!(out.contains(
            "if (selection.ids.size > 0) { selection.toggle(id); setLastSelectedId(id); return; }"
        ));
        assert!(out.contains("drawer.open(id); setLastSelectedId(id);"));
        assert!(out.contains(
            "selectionContainsOpenId: drawerId !== null ? selection.has(drawerId) : false"
        ));
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
            out.contains(
                "const cellClick = useCallback((id: string, _event: React.MouseEvent) => {"
            )
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
        assert!(out.contains(
            "lastDeleteSuccess: drawerDelete.isSuccess ? drawerDelete.submittedAt : null"
        ));
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
        assert!(out.contains(
            "segments: parseSegments(searchRaw, SEARCH_KEYWORDS, SEARCH_ALWAYS_ARRAY),"
        ));
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
}
