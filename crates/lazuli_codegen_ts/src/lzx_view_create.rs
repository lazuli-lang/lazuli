//! `view create` emitter — emits a form-shaped view hook with RHF +
//! `zodResolver` pre-bound to the command's input schema.
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §6.3. Returns a
//! `{ form, submit, handleSubmit, meta }` shape so consumers can wire
//! to any RHF-aware UI library.

use std::fmt::Write;

use crate::lzx::{
    Audience, Surface, ViewCreate, audience_view_pascal, banner, command_ident,
    format_cells_literal, format_string_array, pascal_case, view_hook_name, view_spec_const,
};

/// Emit `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`
/// for a `view create` view.
pub fn emit_view_create(surface: &Surface, audience: &Audience, view: &ViewCreate) -> String {
    let mut s = String::new();
    s.push_str(banner());

    write_imports(&mut s, surface, view);
    write_spec_const(&mut s, audience, view);
    write_slot_interface(&mut s, audience, view);
    write_hook(&mut s, audience, view, surface);

    s
}

fn write_imports(s: &mut String, surface: &Surface, view: &ViewCreate) {
    let feature_pascal = pascal_case(&surface.feature);
    let submit_ident = command_ident(&view.submit);
    let input_iface = command_input_iface(&view.submit, &feature_pascal);
    let schema_ident = command_schema_ident(&view.submit, &feature_pascal);

    // 1. Runtime hooks.
    writeln!(s, "import {{").ok();
    writeln!(s, "  useLazuliCommand,").ok();
    writeln!(s, "}} from \"@lazuli/runtime/react\";").ok();

    // 2. React Hook Form + zodResolver.
    writeln!(s, "import {{ useForm }} from \"react-hook-form\";").ok();
    writeln!(
        s,
        "import {{ zodResolver }} from \"@hookform/resolvers/zod\";"
    )
    .ok();

    // 3. Feature SDK — submit command + its input type.
    writeln!(s, "import {{").ok();
    writeln!(s, "  {},", submit_ident).ok();
    writeln!(s, "  type {},", input_iface).ok();
    writeln!(s, "}} from \"../../{}.gen.js\";", surface.feature).ok();

    // 4. Local Zod schema.
    writeln!(
        s,
        "import {{ {} }} from \"../../{}.zod.js\";",
        schema_ident, surface.feature
    )
    .ok();

    // 5. Cell slot prop types.
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

fn write_spec_const(s: &mut String, audience: &Audience, view: &ViewCreate) {
    let const_name = view_spec_const(&audience.name, &view.name);
    let submit_ident = command_ident(&view.submit);
    // Reuse feature pascal from the command's feature (matches surface feature).
    let feature_pascal = pascal_case(&view.submit.feature);
    let schema_ident = command_schema_ident(&view.submit, &feature_pascal);

    writeln!(s, "// Compile-time view spec const.").ok();
    writeln!(s, "export const {} = {{", const_name).ok();
    writeln!(s, "  submit: {},", submit_ident).ok();
    writeln!(s, "  schema: {},", schema_ident).ok();
    writeln!(
        s,
        "  fields: {} as const,",
        format_string_array(&view.fields)
    )
    .ok();
    writeln!(s, "  cells: {},", format_cells_literal(&view.cells)).ok();
    if let Some(route) = &view.route {
        writeln!(s, "  route: \"{}\",", route).ok();
    }
    writeln!(s, "}} as const;").ok();
    s.push('\n');
}

fn write_slot_interface(s: &mut String, audience: &Audience, view: &ViewCreate) {
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

fn write_hook(s: &mut String, audience: &Audience, view: &ViewCreate, _surface: &Surface) {
    let const_name = view_spec_const(&audience.name, &view.name);
    let hook_name = view_hook_name(&audience.name, &view.name);
    let feature_pascal = pascal_case(&view.submit.feature);
    let input_iface = command_input_iface(&view.submit, &feature_pascal);

    writeln!(s, "export function {}() {{", hook_name).ok();
    writeln!(s, "  const submit = useLazuliCommand({}.submit);", const_name).ok();
    writeln!(s, "  const form = useForm<{}>({{", input_iface).ok();
    writeln!(s, "    resolver: zodResolver({}.schema),", const_name).ok();
    writeln!(s, "  }});").ok();
    writeln!(s).ok();
    writeln!(
        s,
        "  const handleSubmit = form.handleSubmit(async (values) =>"
    )
    .ok();
    writeln!(s, "    submit.mutateAsync(values),").ok();
    writeln!(s, "  );").ok();
    writeln!(s).ok();
    writeln!(s, "  return {{ form, submit, handleSubmit, meta: {} }};", const_name).ok();
    writeln!(s, "}}").ok();
}

// ---------------------------------------------------------------------------
// Naming helpers specific to create views (the input interface and Zod
// schema identifiers follow the existing runtime emitter convention).
// ---------------------------------------------------------------------------

fn command_input_iface(cmd: &crate::lzx::CommandRef, feature_pascal: &str) -> String {
    let mut parts = cmd.name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();
    let mut out = pascal_case(verb);
    out.push_str(feature_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out.push_str("Input");
    out
}

fn command_schema_ident(cmd: &crate::lzx::CommandRef, feature_pascal: &str) -> String {
    let mut out = command_input_iface(cmd, feature_pascal);
    // CreateSlugInput → createSlugInputSchema.
    let mut chars = out.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return out,
    };
    let mut lower_first = String::with_capacity(out.len());
    for c in first.to_lowercase() {
        lower_first.push(c);
    }
    lower_first.push_str(chars.as_str());
    out = lower_first;
    out.push_str("Schema");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::ir::*;
    use crate::lzx::test_fixtures;

    fn minimal_view_create() -> ViewCreate {
        ViewCreate {
            name: "thing_create".to_owned(),
            route: None,
            submit: CommandRef {
                feature: "thing".to_owned(),
                name: "create".to_owned(),
            },
            fields: vec!["id".to_owned()],
            cells: vec![],
        }
    }

    fn minimal_audience(view: ViewCreate) -> Audience {
        Audience {
            name: "viewer".to_owned(),
            requires: vec![],
            views: vec![View::Create(view)],
        }
    }

    fn minimal_surface(audience: Audience) -> Surface {
        Surface {
            feature: "thing".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![audience],
        }
    }

    #[test]
    fn emits_minimal_fixture() {
        let view = minimal_view_create();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_create(&surface, &audience, &view);
        assert!(out.starts_with("// Code generated by lazuli; DO NOT EDIT.\n"));
        assert!(out.contains("export const viewerThingCreateView"));
        assert!(out.contains("export function useViewerThingCreateView"));
        // RHF + zodResolver always wired.
        assert!(out.contains("react-hook-form"));
        assert!(out.contains("@hookform/resolvers/zod"));
        assert!(out.contains("zodResolver(viewerThingCreateView.schema)"));
        // No cells → no slot interface.
        assert!(!out.contains("Slots"));
        // No route → spec const omits it.
        assert!(!out.contains("route:"));
    }

    #[test]
    fn emits_full_l0_3_section_13_1_fixture() {
        let surface = test_fixtures::slug_web_surface();
        let audience = &surface.audiences[0]; // admin
        let view = match &audience.views[2] {
            View::Create(v) => v,
            _ => panic!("expected create view at index 2"),
        };

        let out = emit_view_create(&surface, audience, view);

        // Spec const + hook.
        assert!(out.contains("export const adminSlugCreateView"));
        assert!(out.contains("export function useAdminSlugCreateView"));
        // Imports.
        assert!(out.contains("import { useForm } from \"react-hook-form\";"));
        assert!(
            out.contains("import { zodResolver } from \"@hookform/resolvers/zod\";")
        );
        // Command + input + schema imports.
        assert!(out.contains("createSlug,"));
        assert!(out.contains("type CreateSlugInput,"));
        assert!(out.contains("from \"../../slug.gen.js\""));
        assert!(out.contains("import { createSlugInputSchema } from \"../../slug.zod.js\";"));
        // Spec const fields.
        assert!(out.contains("submit: createSlug"));
        assert!(out.contains("schema: createSlugInputSchema"));
        assert!(out.contains(
            "fields: [\"key\", \"title\", \"description\", \"tags\"] as const"
        ));
        assert!(out.contains("cells: { tags: \"@client.type_badge\" as const }"));
        assert!(out.contains("route: \"/slugs/new\""));
        // Slot interface.
        assert!(out.contains("export interface AdminSlugCreateSlots"));
        // Hook body — RHF wiring.
        assert!(out.contains("const submit = useLazuliCommand(adminSlugCreateView.submit);"));
        assert!(out.contains("const form = useForm<CreateSlugInput>({"));
        assert!(out.contains("resolver: zodResolver(adminSlugCreateView.schema)"));
        assert!(out.contains(
            "const handleSubmit = form.handleSubmit(async (values) =>"
        ));
        assert!(out.contains("submit.mutateAsync(values)"));
        assert!(out.contains(
            "return { form, submit, handleSubmit, meta: adminSlugCreateView };"
        ));
    }

    #[test]
    fn generates_correct_hook_name_with_kebab_audience() {
        let mut view = minimal_view_create();
        view.submit.feature = "thing".to_owned();
        let audience = Audience {
            name: "workspace-admin".to_owned(),
            requires: vec![],
            views: vec![View::Create(view.clone())],
        };
        let surface = minimal_surface(audience.clone());

        let out = emit_view_create(&surface, &audience, &view);
        assert!(out.contains("export function useWorkspaceAdminThingCreateView"));
        assert!(out.contains("export const workspaceAdminThingCreateView"));
    }

    #[test]
    fn slot_bindings_produce_parallel_import_lines() {
        let mut view = minimal_view_create();
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

        let out = emit_view_create(&surface, &audience, &view);
        assert!(out.contains(
            "import type { TypeBadgeProps } from \"../../cells/type_badge.gen.js\";"
        ));
        assert!(out.contains(
            "import type { UserAvatarProps } from \"../../cells/user_avatar.gen.js\";"
        ));
        assert!(out.contains("TypeBadge: React.ComponentType<TypeBadgeProps>"));
        assert!(out.contains("UserAvatar: React.ComponentType<UserAvatarProps>"));
    }

    #[test]
    fn pre_binds_rhf_and_zod_resolver() {
        // Every create emit must always include the RHF + zodResolver
        // wiring — there's no opt-out at the emitter layer (schemas
        // can be disabled at the manifest layer, handled by a future
        // cell).
        let view = minimal_view_create();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_create(&surface, &audience, &view);
        assert!(out.contains("useForm"));
        assert!(out.contains("zodResolver"));
        assert!(out.contains("resolver: zodResolver"));
        assert!(out.contains("handleSubmit = form.handleSubmit"));
    }

    #[test]
    fn returns_form_submit_handle_meta_quad() {
        let view = minimal_view_create();
        let audience = minimal_audience(view.clone());
        let surface = minimal_surface(audience.clone());

        let out = emit_view_create(&surface, &audience, &view);
        // The proposal §6.3 says the hook returns `{ form, submit,
        // handleSubmit, meta }` — assert all four keys present in
        // the return statement.
        assert!(out.contains("return { form, submit, handleSubmit, meta:"));
    }
}
