//! Compile-time spec const + slot interface for a generated view.
//!
//! Each `view list` lowers to one frozen TS object
//! (`export const <audience><View>View = { … } as const;`) that the
//! companion hook reads through. The const carries the source query,
//! the render shape (table columns or `cells: "@client.<slot>"`),
//! search/filter declarations, action command bindings, and the route
//! string when authored — so the hook is purely about wiring
//! React-side state.
//!
//! Column-assertion + slot-interface emission live here too because
//! they're driven from the same `ListRender` discriminator: tables get
//! the `_AssertColumns` type guard that pins every column to a
//! `<Feature>` field, grids get a `<Audience><View>Cells` slot
//! interface keyed by the row noun.

use std::fmt::Write;

use crate::lzx::{
    Audience, CommandRef, ListRender, SearchDecl, SearchMode, Surface, ViewList,
    audience_view_pascal, command_action_key, command_ident, format_cells_literal,
    format_string_array, lower_camel, pascal_case, query_ident, view_spec_const,
};

pub(super) fn write_spec_const(s: &mut String, audience: &Audience, view: &ViewList) {
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
            writeln!(s, "  columns: {} as const,", format_string_array(columns)).ok();
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

pub(super) fn write_column_assert(
    s: &mut String,
    audience: &Audience,
    view: &ViewList,
    surface: &Surface,
) {
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

pub(super) fn write_slot_interface(
    s: &mut String,
    audience: &Audience,
    view: &ViewList,
    surface: &Surface,
) {
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
