//! `view detail` emitter — emits a typed `<audience><View>View` spec const
//! plus a `use<Audience><View>View` hook that consumes router params via
//! a target-specific `useParams` import.
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §6.2. Also emits a
//! `<Audience><View>Sections` slot interface for any `sections` declared
//! on the detail view per §8.1.

use std::fmt::Write;

use crate::lzx::lzx_router_adapter::{RouterTarget, router_useparams_import, translate_route_path};
use crate::lzx::{
    Audience, CommandRef, RouteParam, Surface, ViewDetail, audience_view_pascal, banner,
    command_action_key, command_ident, format_cells_literal, lower_camel, pascal_case, query_ident,
    view_hook_name, view_spec_const,
};

/// Emit `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`
/// for a `view detail` view.
///
/// Detail views resolve their record from the route's path params, so
/// the target's `useParams` hook is imported via
/// `crate::lzx_router_adapter::router_useparams_import`. The hook
/// itself wires the typed `Params` interface against the bound query.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx_view_detail::emit_view_detail;
/// use lazuli_codegen_ts::lzx::lzx_router_adapter::RouterTarget;
/// use lazuli_ir::{Audience, Surface, ViewDetail};
///
/// let surface: Surface = /* … */ unimplemented!();
/// let audience: Audience = /* … */ unimplemented!();
/// let view: ViewDetail = /* … */ unimplemented!();
/// let _src = emit_view_detail(&surface, &audience, &view, RouterTarget::ViteReact);
/// ```
pub fn emit_view_detail(
    surface: &Surface,
    audience: &Audience,
    view: &ViewDetail,
    target: RouterTarget,
) -> String {
    let mut s = String::new();
    s.push_str(banner());

    write_imports(&mut s, surface, view, target);
    write_spec_const(&mut s, audience, view, target);
    write_sections_interface(&mut s, audience, view, surface);
    write_slot_interface(&mut s, audience, view);
    write_hook(&mut s, audience, view, target);

    s
}

fn write_imports(s: &mut String, surface: &Surface, view: &ViewDetail, target: RouterTarget) {
    let feature_pascal = pascal_case(&surface.feature);

    // 1. Runtime hooks.
    writeln!(s, "import {{").ok();
    writeln!(s, "  useLazuliQuery,").ok();
    if !view.actions.is_empty() {
        writeln!(s, "  useLazuliCommand,").ok();
    }
    // Wave-W6 view-level UX primitive hooks — each emitted call site in
    // `lzx_ux::emit_ux_const` must be imported or the file won't typecheck.
    for hook in crate::lzx::lzx_ux::ux_runtime_hook_imports(&view.ux) {
        writeln!(s, "  {},", hook).ok();
    }
    writeln!(s, "}} from \"@lazuli/runtime/react\";").ok();

    // 2. Router-specific useParams import.
    s.push_str(router_useparams_import(target));

    // 3. Feature SDK — resource type + source query + each action command.
    writeln!(s, "import {{").ok();
    writeln!(s, "  {},", query_ident(&view.source)).ok();
    for cmd in &view.actions {
        writeln!(s, "  {},", command_ident(cmd)).ok();
    }
    writeln!(s, "  type {},", feature_pascal).ok();
    writeln!(s, "}} from \"../../{}.gen.js\";", surface.feature).ok();

    // 4. Cell slot prop types.
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

fn write_spec_const(s: &mut String, audience: &Audience, view: &ViewDetail, target: RouterTarget) {
    let const_name = view_spec_const(&audience.name, &view.name);

    writeln!(s, "// Compile-time view spec const.").ok();
    writeln!(s, "export const {} = {{", const_name).ok();
    writeln!(s, "  source: {},", query_ident(&view.source)).ok();
    if let Some(route) = &view.route {
        writeln!(s, "  route: \"{}\",", translate_route_path(target, route)).ok();
    }
    if !view.sections.is_empty() {
        let sections: Vec<String> = view.sections.iter().map(|s| format!("\"{}\"", s)).collect();
        writeln!(s, "  sections: [{}] as const,", sections.join(", ")).ok();
    }
    writeln!(s, "  cells: {},", format_cells_literal(&view.cells)).ok();
    if !view.actions.is_empty() {
        writeln!(s, "  actions: {},", format_actions_object(&view.actions)).ok();
    }
    writeln!(s, "}} as const;").ok();
    s.push('\n');
}

fn format_actions_object(actions: &[CommandRef]) -> String {
    let parts: Vec<String> = actions
        .iter()
        .map(|c| format!("{}: {}", command_action_key(c), command_ident(c)))
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

fn write_sections_interface(
    s: &mut String,
    audience: &Audience,
    view: &ViewDetail,
    surface: &Surface,
) {
    if view.sections.is_empty() {
        return;
    }
    let iface = format!(
        "{}Sections",
        audience_view_pascal(&audience.name, &view.name)
    );
    let feature_pascal = pascal_case(&surface.feature);
    let row_param = lower_camel(&surface.feature);

    writeln!(s, "// Section slot contract — V implements each Component.").ok();
    writeln!(s, "export interface {} {{", iface).ok();
    for section in &view.sections {
        let pascal = pascal_case(section);
        writeln!(
            s,
            "  {}: React.ComponentType<{{ {}: {} }}>;",
            pascal, row_param, feature_pascal
        )
        .ok();
    }
    writeln!(s, "}}").ok();
    s.push('\n');
}

fn write_slot_interface(s: &mut String, audience: &Audience, view: &ViewDetail) {
    if view.cells.is_empty() {
        return;
    }
    let iface = format!("{}Slots", audience_view_pascal(&audience.name, &view.name));
    writeln!(s, "// Cell slot contract.").ok();
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
    writeln!(s, "}}").ok();
    s.push('\n');
}

fn write_hook(s: &mut String, audience: &Audience, view: &ViewDetail, target: RouterTarget) {
    let const_name = view_spec_const(&audience.name, &view.name);
    let hook_name = view_hook_name(&audience.name, &view.name);
    let route = view.route.as_deref().unwrap_or("/");
    let translated = translate_route_path(target, route);

    writeln!(s, "export function {}() {{", hook_name).ok();

    // Destructure all route params off useParams.
    if !view.route_params.is_empty() {
        let names: Vec<&str> = view.route_params.iter().map(|p| p.name.as_str()).collect();
        writeln!(
            s,
            "  const {{ {} }} = useParams({{ from: \"{}\" }});",
            names.join(", "),
            translated
        )
        .ok();
        let args = format_route_args(&view.route_params);
        writeln!(
            s,
            "  const query = useLazuliQuery({}.source, {});",
            const_name, args
        )
        .ok();
    } else {
        writeln!(
            s,
            "  const query = useLazuliQuery({}.source, {{}});",
            const_name
        )
        .ok();
    }

    for cmd in &view.actions {
        let key = command_action_key(cmd);
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
    // Wave-W6 detail-view UX primitives (wizard_steps / tab_group).
    if !view.ux.is_empty() {
        s.push_str(&crate::lzx::lzx_ux::emit_ux_const(&view.ux));
    }

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
    crate::lzx::lzx_ux::emit_ux_return_fields(s, &view.ux);
    writeln!(s, "    meta: {},", const_name).ok();
    writeln!(s, "  }} as const;").ok();
    writeln!(s, "}}").ok();
}

fn format_route_args(params: &[RouteParam]) -> String {
    let parts: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    format!("{{ {} }}", parts.join(", "))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::ir::*;
    use crate::lzx::lzx_router_adapter::RouterTarget;
    use crate::lzx::test_fixtures;

    fn minimal_view_detail() -> ViewDetail {
        ViewDetail {
            name: "thing_detail".to_owned(),
            route: None,
            source: QueryRef {
                feature: "thing".to_owned(),
                kind: QueryKind::Lookup,
                name: "by_id".to_owned(),
            },
            route_params: vec![],
            sections: vec![],
            cells: vec![],
            actions: vec![],
            redacted_fields: Vec::new(),
            ux: Default::default(),
            span_ref: None,
        }
    }

    fn minimal_audience(view: ViewDetail) -> Audience {
        Audience {
            name: "viewer".to_owned(),
            requires: vec![],
            views: vec![View::Detail(view)],
            ux: Default::default(),
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
        let view = minimal_view_detail();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_detail(&surface, &audience, &view, RouterTarget::ViteReact);

        assert!(out.starts_with("// Code generated by lazuli; DO NOT EDIT.\n"));
        assert!(out.contains("export const viewerThingDetailView"));
        assert!(out.contains("export function useViewerThingDetailView"));
        // No actions → no useLazuliCommand.
        assert!(!out.contains("useLazuliCommand"));
        // No sections → no sections interface.
        assert!(!out.contains("Sections"));
        // No cells → no Slots.
        assert!(!out.contains("Slots"));
        // No route params → empty args object.
        assert!(out.contains("useLazuliQuery(viewerThingDetailView.source, {})"));
    }

    #[test]
    fn emits_full_l0_3_section_13_1_fixture() {
        let surface = test_fixtures::slug_web_surface();
        let audience = &surface.audiences[0]; // admin
        let view = match &audience.views[1] {
            View::Detail(v) => v,
            _ => panic!("expected detail view at index 1"),
        };

        let out = emit_view_detail(&surface, audience, view, RouterTarget::ViteReact);

        // Spec const + hook.
        assert!(out.contains("export const adminSlugDetailView"));
        assert!(out.contains("export function useAdminSlugDetailView"));
        // Source query identifier (lookup → lookup<Resource>By<Short>).
        assert!(out.contains("lookupSlugByKey"));
        // Route — translated to TanStack Router syntax for vite-react.
        assert!(out.contains("route: \"/slugs/$key\""));
        // Sections array in the spec.
        assert!(out.contains("sections: [\"header\", \"metadata\", \"related_items\"] as const"));
        // Sections interface emitted with each section pascal-cased.
        assert!(out.contains("export interface AdminSlugDetailSections"));
        assert!(out.contains("Header: React.ComponentType<{ slug: Slug }>"));
        assert!(out.contains("Metadata: React.ComponentType<{ slug: Slug }>"));
        assert!(out.contains("RelatedItems: React.ComponentType<{ slug: Slug }>"));
        // Slot interface for `cells tags`.
        assert!(out.contains("export interface AdminSlugDetailSlots"));
        assert!(out.contains("TypeBadge: React.ComponentType<TypeBadgeProps>"));
        // useParams import for the default vite-react target.
        assert!(out.contains("@tanstack/react-router"));
        // Hook body destructures key from useParams and passes it to source.
        assert!(out.contains("const { key } = useParams({ from: \"/slugs/$key\" })"));
        assert!(out.contains("useLazuliQuery(adminSlugDetailView.source, { key })"));
        // Actions: update + delete.
        assert!(out.contains("actions: { update: updateSlug, delete: deleteSlug }"));
        assert!(
            out.contains("const delete_ = useLazuliCommand(adminSlugDetailView.actions.delete)")
        );
    }

    #[test]
    fn generates_correct_hook_name_with_compound_view() {
        let mut view = minimal_view_detail();
        view.name = "user-profile".to_owned();
        let audience = Audience {
            name: "admin".to_owned(),
            requires: vec![],
            views: vec![View::Detail(view.clone())],
            ux: Default::default(),
            span_ref: None,
        };
        let surface = minimal_surface(audience.clone());

        let out = emit_view_detail(&surface, &audience, &view, RouterTarget::ViteReact);
        assert!(out.contains("export function useAdminUserProfileView"));
        assert!(out.contains("export const adminUserProfileView"));
    }

    #[test]
    fn slot_bindings_produce_parallel_import_lines() {
        let mut view = minimal_view_detail();
        view.cells = vec![
            CellBinding {
                field: "tags".to_owned(),
                slot: "type_badge".to_owned(),
            },
            CellBinding {
                field: "avatar".to_owned(),
                slot: "user_avatar".to_owned(),
            },
        ];
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_detail(&surface, &audience, &view, RouterTarget::ViteReact);
        assert!(
            out.contains("import type { TypeBadgeProps } from \"../../cells/type_badge.gen.js\";")
        );
        assert!(
            out.contains(
                "import type { UserAvatarProps } from \"../../cells/user_avatar.gen.js\";"
            )
        );
        assert!(out.contains("TypeBadge: React.ComponentType<TypeBadgeProps>"));
        assert!(out.contains("UserAvatar: React.ComponentType<UserAvatarProps>"));
    }

    #[test]
    fn router_target_switches_useparams_import() {
        let surface = test_fixtures::slug_web_surface();
        let audience = &surface.audiences[0];
        let view = match &audience.views[1] {
            View::Detail(v) => v,
            _ => panic!("expected detail view"),
        };

        let vite = emit_view_detail(&surface, audience, view, RouterTarget::ViteReact);
        let nextjs = emit_view_detail(&surface, audience, view, RouterTarget::NextJs);
        let expo = emit_view_detail(&surface, audience, view, RouterTarget::Expo);
        let tauri = emit_view_detail(&surface, audience, view, RouterTarget::Tauri);

        assert!(vite.contains("from \"@tanstack/react-router\""));
        assert!(!vite.contains("next/navigation"));
        assert!(!vite.contains("expo-router"));

        assert!(nextjs.contains("from \"next/navigation\""));
        assert!(!nextjs.contains("@tanstack/react-router"));

        assert!(expo.contains("useLocalSearchParams as useParams"));
        assert!(expo.contains("from \"expo-router\""));

        // Tauri matches vite.
        assert!(tauri.contains("from \"@tanstack/react-router\""));
    }

    #[test]
    fn multiple_actions_emit_as_object_literal() {
        let mut view = minimal_view_detail();
        view.source.feature = "slug".to_owned();
        view.actions = vec![
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

        let out = emit_view_detail(&surface, &audience, &view, RouterTarget::ViteReact);
        assert!(out.contains("actions: { update: updateSlug, archive: archiveSlug }"));
        assert!(out.contains("actions: { update, archive }"));
    }
}
