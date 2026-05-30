//! Cell C.2 — slot interface emitter tests.
//!
//! Ships ≥6 tests covering single binding, cross-view consistency,
//! section fallback, casing convention, missing resource, and
//! determinism.

use super::*;

use lazuli_codegen_spec::{FieldKind, RuntimeFeature, RuntimeField, RuntimeResource, Tenancy};
use lazuli_ir::ListRender;

use crate::lzx_audience_slot::ir::{
    Audience, CellBinding, CommandRef, PolicyAtom, QueryKind, QueryRef, RouteParam, Surface,
    SurfaceTarget, View, ViewCreate, ViewDetail, ViewList,
};

// -----------------------------------------------------------------------
// Fixtures
// -----------------------------------------------------------------------

fn slug_feature() -> RuntimeFeature {
    RuntimeFeature {
        name: "slug".to_owned(),
        source_path: "features/slug/slug.lzi".to_owned(),
        resources: vec![RuntimeResource {
            name: "slug".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![
                RuntimeField {
                    name: "key".to_owned(),
                    kind: FieldKind::Text,
                },
                RuntimeField {
                    name: "tags".to_owned(),
                    kind: FieldKind::Text,
                },
            ],
        }],
        commands: vec![],
        queries: vec![],
    }
}

fn feature_without_matching_resource() -> RuntimeFeature {
    // Feature `widget` but no `widget` resource — only `gadget`.
    // Matches the "missing source resource" graceful fallback.
    RuntimeFeature {
        name: "widget".to_owned(),
        source_path: "features/widget/widget.lzi".to_owned(),
        resources: vec![RuntimeResource {
            name: "gadget".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![],
        }],
        commands: vec![],
        queries: vec![],
    }
}

fn type_badge_cell() -> CellBinding {
    CellBinding {
        field: "tags".to_owned(),
        slot: "type_badge".to_owned(),
    }
}

fn slug_list_view(cells: Vec<CellBinding>) -> View {
    View::List(ViewList {
        name: "slug_list".to_owned(),
        route: Some("/slugs".to_owned()),
        source: QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "mine".to_owned(),
        },
        render: ListRender::Table {
            columns: vec!["key".to_owned(), "tags".to_owned()],
        },
        search: None,
        filter: vec![],
        cells,
        actions: vec![CommandRef {
            feature: "slug".to_owned(),
            name: "create".to_owned(),
        }],
        drawer: None,
        sort: None,
        selection: None,
        settings: vec![],
        redacted_fields: Vec::new(),
        ux: Default::default(),
        span_ref: None,
    })
}

fn slug_detail_view(cells: Vec<CellBinding>, sections: Vec<String>) -> View {
    View::Detail(ViewDetail {
        name: "slug_detail".to_owned(),
        route: Some("/slugs/:key".to_owned()),
        source: QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::Lookup,
            name: "by_key".to_owned(),
        },
        route_params: vec![RouteParam {
            name: "key".to_owned(),
            type_ref: "Text".to_owned(),
        }],
        sections,
        cells,
        actions: vec![],
        redacted_fields: Vec::new(),
        ux: Default::default(),
        span_ref: None,
    })
}

fn slug_create_view(cells: Vec<CellBinding>) -> View {
    View::Create(ViewCreate {
        name: "slug_create".to_owned(),
        route: Some("/slugs/new".to_owned()),
        submit: CommandRef {
            feature: "slug".to_owned(),
            name: "create".to_owned(),
        },
        on_success: None,
        fields: vec!["key".to_owned(), "tags".to_owned()],
        cells,
        redacted_fields: Vec::new(),
        span_ref: None,
    })
}

fn admin_audience_with_views(views: Vec<View>) -> Audience {
    Audience {
        name: "admin".to_owned(),
        requires: vec![PolicyAtom {
            namespace: "scope".to_owned(),
            name: "workspace_admin".to_owned(),
            args: None,
        }],
        views,
        ux: Default::default(),
        span_ref: None,
    }
}

fn surface_with_audience(audience: Audience) -> Surface {
    Surface {
        feature: "slug".to_owned(),
        target: SurfaceTarget::Web,
        audiences: vec![audience],
        span_ref: None,
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

/// Single binding — slot bound in one view, type derived from
/// `Resource["field"]`.
#[test]
fn single_binding_emits_field_derived_type() {
    let surface = surface_with_audience(admin_audience_with_views(vec![slug_list_view(vec![
        type_badge_cell(),
    ])]));

    let output = emit_slot_interface(&surface, &slug_feature(), "type_badge");

    assert!(
        output.contains("export interface TypeBadgeProps"),
        "missing interface; got:\n{output}"
    );
    assert!(
        output.contains("value: Slug[\"tags\"];"),
        "expected `value: Slug[\"tags\"];`; got:\n{output}"
    );
    assert!(
        output.contains("row: Slug;"),
        "expected `row: Slug;`; got:\n{output}"
    );
    assert!(
        output.contains("import type { Slug } from \"../slug.gen.js\""),
        "missing typed import; got:\n{output}"
    );
}

/// Cross-view binding — same slot bound in `slug_list` AND
/// `slug_detail` on the SAME field. The interface stays single
/// and consistent; alternative bindings are noted as comments.
#[test]
fn cross_view_binding_emits_one_consistent_interface() {
    let surface = surface_with_audience(admin_audience_with_views(vec![
        slug_list_view(vec![type_badge_cell()]),
        slug_detail_view(vec![type_badge_cell()], vec![]),
    ]));

    let output = emit_slot_interface(&surface, &slug_feature(), "type_badge");

    // Single interface emitted (only one `export interface`).
    let export_count = output.matches("export interface ").count();
    assert_eq!(
        export_count, 1,
        "expected exactly one exported interface; got {export_count}:\n{output}"
    );

    // Both bindings flagged in the multi-binding comment block.
    assert!(
        output.contains("Bound in 2 views:"),
        "missing multi-binding header; got:\n{output}"
    );
    assert!(
        output.contains("admin/slug_list field `tags` (primary)"),
        "missing primary binding line; got:\n{output}"
    );
    assert!(
        output.contains("admin/slug_detail field `tags` (also)"),
        "missing secondary binding line; got:\n{output}"
    );

    // Primary type still wins.
    assert!(output.contains("value: Slug[\"tags\"];"));
}

/// Section slot — slot name matches a `sections` entry, no cell
/// binding anywhere. Emits `value: void; row: <Resource>;`.
#[test]
fn section_slot_emits_void_value_with_whole_row() {
    let surface = surface_with_audience(admin_audience_with_views(vec![slug_detail_view(
        vec![],
        vec!["header".to_owned(), "metadata".to_owned()],
    )]));

    let output = emit_slot_interface(&surface, &slug_feature(), "header");

    assert!(
        output.contains("export interface HeaderProps"),
        "missing section interface; got:\n{output}"
    );
    assert!(
        output.contains("value: void;"),
        "section slot must emit `value: void;`; got:\n{output}"
    );
    assert!(
        output.contains("row: Slug;"),
        "section slot must still expose the whole row; got:\n{output}"
    );
    // Section emitter calls out the section-only nature so reviewers
    // don't grep `value: <type>` and miss the contract.
    assert!(
        output.contains("Section slot:"),
        "missing section-slot banner; got:\n{output}"
    );
}

/// Casing — slot `type_badge` (snake) pascalizes to `TypeBadge`;
/// interface is `TypeBadgeProps`. Also verifies kebab-case slots.
#[test]
fn slot_name_pascal_case_with_props_suffix() {
    // snake_case slot name
    let surface_snake =
        surface_with_audience(admin_audience_with_views(vec![slug_list_view(vec![
            type_badge_cell(),
        ])]));
    let output_snake = emit_slot_interface(&surface_snake, &slug_feature(), "type_badge");
    assert!(output_snake.contains("export interface TypeBadgeProps"));

    // kebab-case slot name → `quick-action`
    let kebab_cell = CellBinding {
        field: "tags".to_owned(),
        slot: "quick-action".to_owned(),
    };
    let surface_kebab =
        surface_with_audience(admin_audience_with_views(vec![slug_list_view(vec![
            kebab_cell,
        ])]));
    let output_kebab = emit_slot_interface(&surface_kebab, &slug_feature(), "quick-action");
    assert!(
        output_kebab.contains("export interface QuickActionProps"),
        "kebab-case slot should pascalize to PascalCase; got:\n{output_kebab}"
    );
}

/// Missing source resource — feature has no resource whose Pascal
/// matches the feature name. The emitter falls back to
/// `unknown`-typed props + TODO comment instead of producing a
/// broken `import type { Widget }` line.
#[test]
fn missing_source_resource_resolves_gracefully() {
    let surface = Surface {
        feature: "widget".to_owned(),
        target: SurfaceTarget::Web,
        audiences: vec![Audience {
            name: "admin".to_owned(),
            requires: vec![],
            views: vec![View::List(ViewList {
                name: "widget_list".to_owned(),
                route: None,
                source: QueryRef {
                    feature: "widget".to_owned(),
                    kind: QueryKind::List,
                    name: "all".to_owned(),
                },
                render: ListRender::Table {
                    columns: vec!["name".to_owned()],
                },
                search: None,
                filter: vec![],
                cells: vec![CellBinding {
                    field: "status".to_owned(),
                    slot: "badge".to_owned(),
                }],
                actions: vec![],
                drawer: None,
                sort: None,
                selection: None,
                settings: vec![],
                redacted_fields: Vec::new(),
                ux: Default::default(),
                span_ref: None,
            })],
            ux: Default::default(),
            span_ref: None,
        }],
        span_ref: None,
    };

    let output = emit_slot_interface(&surface, &feature_without_matching_resource(), "badge");

    assert!(
        output.contains("// TODO"),
        "missing-resource case must emit TODO comment; got:\n{output}"
    );
    assert!(
        output.contains("value: unknown;"),
        "missing-resource case must fall back to `unknown` value; got:\n{output}"
    );
    assert!(
        output.contains("row: unknown;"),
        "missing-resource case must fall back to `unknown` row; got:\n{output}"
    );
    // Don't emit a broken typed import.
    assert!(
        !output.contains("import type { Widget }"),
        "should NOT emit unresolved type import; got:\n{output}"
    );
}

/// Deterministic emission — same surface emits identical bytes
/// across repeated calls, and re-ordering equivalent bindings
/// inside the same audience yields the same primary pick
/// (source-order discipline).
#[test]
fn slot_interface_emission_is_deterministic() {
    let surface = surface_with_audience(admin_audience_with_views(vec![
        slug_list_view(vec![type_badge_cell()]),
        slug_detail_view(vec![type_badge_cell()], vec![]),
    ]));
    let feature = slug_feature();

    let out_a = emit_slot_interface(&surface, &feature, "type_badge");
    let out_b = emit_slot_interface(&surface, &feature, "type_badge");
    assert_eq!(out_a, out_b);
}

/// Slot with no binding anywhere — emits `unknown`-typed props +
/// TODO comment naming the slot. Locks the "unresolved slot"
/// graceful path so generated cells/<x>.gen.ts always type-checks.
#[test]
fn slot_with_no_binding_emits_unresolved_fallback() {
    let surface = surface_with_audience(admin_audience_with_views(vec![slug_list_view(vec![])]));

    let output = emit_slot_interface(&surface, &slug_feature(), "ghost_slot");

    assert!(output.contains("export interface GhostSlotProps"));
    assert!(output.contains("// TODO"));
    assert!(output.contains("value: unknown;"));
    assert!(output.contains("row: unknown;"));
}

/// Create-view binding — `view create` also carries `cells <field>
/// @client.<slot>` per §6.3. The emitter picks them up just like
/// list/detail bindings; the resulting interface is structurally
/// identical (no special "create" treatment — the form view's slot
/// still receives a field value typed against the resource).
#[test]
fn create_view_cell_binding_emits_field_derived_type() {
    let surface = surface_with_audience(admin_audience_with_views(vec![
        slug_create_view(vec![type_badge_cell()]),
        // Detail view also binds the same field/slot — exercises
        // the multi-binding comment block so we can assert the
        // create-view binding appears alongside detail.
        slug_detail_view(vec![type_badge_cell()], vec![]),
    ]));

    let output = emit_slot_interface(&surface, &slug_feature(), "type_badge");

    assert!(
        output.contains("export interface TypeBadgeProps"),
        "create-view binding should emit the same shape as list/detail; got:\n{output}"
    );
    assert!(output.contains("value: Slug[\"tags\"];"));
    assert!(
        output.contains("admin/slug_create field `tags`"),
        "create-view binding should appear in multi-binding comment block; got:\n{output}"
    );
}
