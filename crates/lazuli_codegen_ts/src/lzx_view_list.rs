//! `view list` emitter — emits a typed `<audience><View>View` spec const
//! + `use<Audience><View>View` hook bundling the source query and all
//! action mutations, plus a slot interface for any `cells <field>
//! @client.<slot>` bindings.
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §6.1.

use std::fmt::Write;

use crate::lzx::{
    Audience, CommandRef, Surface, ViewList, audience_view_pascal, banner, command_action_key,
    command_ident, format_cells_literal, format_string_array, lower_camel, pascal_case, query_ident,
    view_hook_name, view_spec_const,
};
use crate::lzx::{ListRender, SearchDecl, SearchMode};

/// Emit `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`.
pub fn emit_view_list(surface: &Surface, audience: &Audience, view: &ViewList) -> String {
    let mut s = String::new();
    s.push_str(banner());

    write_imports(&mut s, surface, view);
    write_spec_const(&mut s, audience, view);
    write_column_assert(&mut s, audience, view, surface);
    write_slot_interface(&mut s, audience, view, surface);
    write_hook(&mut s, audience, view, surface);

    s
}

fn write_imports(s: &mut String, surface: &Surface, view: &ViewList) {
    let feature_pascal = pascal_case(&surface.feature);

    // 1. Runtime hooks.
    writeln!(s, "import {{").ok();
    writeln!(s, "  useLazuliQuery,").ok();
    if !view.actions.is_empty() {
        writeln!(s, "  useLazuliCommand,").ok();
    }
    writeln!(s, "  type UseLazuliQueryOptions,").ok();
    writeln!(s, "}} from \"@lazuli/runtime/react\";").ok();

    // 2. Feature SDK — resource type + source query + each action command.
    writeln!(s, "import {{").ok();
    writeln!(s, "  {},", query_ident(&view.source)).ok();
    for cmd in &view.actions {
        writeln!(s, "  {},", command_ident(cmd)).ok();
    }
    writeln!(s, "  type {},", feature_pascal).ok();
    writeln!(s, "}} from \"../../{}.gen.js\";", surface.feature).ok();

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
    view.filter.iter().map(|filter| filter.name.clone()).collect()
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

    writeln!(s, "export function {}(", hook_name).ok();
    writeln!(
        s,
        "  options: UseLazuliQueryOptions<{{}}, {}[]> = {{}},",
        feature_pascal
    )
    .ok();
    writeln!(s, ") {{").ok();
    writeln!(
        s,
        "  const query = useLazuliQuery({}.source, {{}}, options);",
        const_name
    )
    .ok();

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

    writeln!(s).ok();
    writeln!(s, "  return {{").ok();
    writeln!(s, "    query,").ok();
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
    writeln!(s, "    meta: {},", const_name).ok();
    writeln!(s, "  }} as const;").ok();
    writeln!(s, "}}").ok();
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

    #[test]
    fn emits_minimal_fixture() {
        let view = minimal_view_list();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view);
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

        let out = emit_view_list(&surface, audience, view);

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

        let out = emit_view_list(&surface, &audience, &view);
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

        let out = emit_view_list(&surface, &audience, &view);
        assert!(out.contains(
            "import type { TypeBadgeProps } from \"../../cells/type_badge.gen.js\";"
        ));
        assert!(out.contains(
            "import type { UserAvatarProps } from \"../../cells/user_avatar.gen.js\";"
        ));
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

        let out = emit_view_list(&surface, &audience, &view);

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

        let out = emit_view_list(&surface, &audience, &view);

        assert!(out.contains("cells: \"@client.item_card\" as const"));
        assert!(!out.contains("columns:"));
        assert!(!out.contains("type _AssertColumns"));
    }

    #[test]
    fn table_render_keeps_columns_and_column_assertion() {
        let view = minimal_view_list();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view);

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

        let out = emit_view_list(&surface, &audience, &view);
        // Object literal style + all three resolved identifiers.
        assert!(out.contains("actions: { create: createSlug, update: updateSlug, archive: archiveSlug }"));
        // Hook returns the spread actions map.
        assert!(out.contains("actions: { create, update, archive }"));
    }

    #[test]
    fn omits_search_filter_and_actions_when_empty() {
        let view = minimal_view_list();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_list(&surface, &audience, &view);
        assert!(!out.contains("search:"));
        assert!(!out.contains("filter:"));
        assert!(!out.contains("actions:"));
    }
}
