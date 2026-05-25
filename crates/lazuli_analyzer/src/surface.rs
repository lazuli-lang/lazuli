//! `.lzx` ViewModel surface lowering — audiences, views, cells, drawers.
//!
//! ## Why this slot exists
//!
//! `.lzx` files carry two distinct surfaces: the **app layer** (routes,
//! experiences, platform surfaces — owned by [`crate::lzx`]) and the
//! **feature ViewModel layer** (audiences + views projected from a
//! single feature's query / command catalog — owned here). Splitting
//! the ViewModel surface from `lib.rs` keeps the surface-specific
//! validations (`lzx-route-param-*`, `lzx-cell-slot-orphan`,
//! `lzx-bad-query-ref`, `lzx-bad-command-ref`) localized to one file
//! instead of bleeding the top-level lowering shape into half the
//! analyzer.
//!
//! The entry point is `lower_surface`, which takes a parsed
//! `SurfaceAst` (from `lazuli_syntax::parse_surface_document`) and
//! yields an `ir::Surface`. Per-view validations performed at
//! lowering time:
//!
//! * source / submit references are well-formed
//!   (`<feature>.query.<kind>.<name>` and
//!   `<feature>.command.<name>` respectively, OR the short
//!   bare-name form for actions inside the same surface).
//! * cell slot + field identifiers are valid kebab/snake idents
//!   (parser already enforces; we re-check to defend against direct
//!   AST construction).
//! * When both `at "<path>"` and `route <name>: Type from path` are
//!   declared, every `:<name>` placeholder in the path has a matching
//!   `route_params` entry and vice versa (`lzx-route-param-*`).
//!
//! Deeper validations (`source` resource exists, `actions` reaches
//! the audience's scope, etc.) defer to the doctor cells per
//! `docs/proposals/lzx-integration-codegen.md` §5.
//!
//! ## What lives here
//!
//! * `lower_surface` — top-level entry point, dispatches per audience.
//! * `lower_audience_ast` / `lower_view_ast` — audience + view family
//!   discriminator (list / detail / create).
//! * `lower_on_success_spec` — `on_success { back | redirect | flash
//!   | invalidates }` lowering for `view create`.
//! * `lower_list_render`, `lower_filter_decl(s)`, `lower_search_decl`,
//!   `lower_search_field`, `lower_binding_ref` — list view sub-shape
//!   builders.
//! * `lower_drawer`, `lower_drawer_route_binding`, `lower_sort_decl`,
//!   `lower_selection_decl`, `lower_setting_decl` — sidecar shapes
//!   on list views.
//! * `validate_cells`, `validate_cells_slot_only`,
//!   `validate_cell_slot_shape` — cell binding validation helpers.
//! * `path_placeholders` — `:name` extractor for route param
//!   cross-check.
//! * `parse_query_ref`, `parse_command_ref` — closed-grammar
//!   reference shape parsers.
//!
//! ## What does NOT live here
//!
//! Anything that touches the *app* layer of `.lzx` (routes,
//! experiences, platform surfaces) — those live in `crate::lzx`.
//! The boundary is: this module lowers `SurfaceAst` (per-feature
//! ViewModel surface), `lzx.rs` lowers `LzxDocument` (per-app
//! manifest).
//!
//! Source AST shapes: `lazuli_syntax::{SurfaceAst, AudienceAst,
//! ViewAst, ViewListAst, ViewDetailAst, ViewCreateAst,
//! OnSuccessSpecAst, DrawerSubViewAst, ...}`. Destination IR shapes:
//! `lazuli_ir::{Surface, Audience, View, ViewList, ViewDetail,
//! ViewCreate, OnSuccessSpec, FlashSpec, DrawerSubView, SortDecl,
//! SelectionDecl, SettingDecl, QueryRef, CommandRef, ...}`.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::command::{lower_invalidates_query_ref, lower_named_arg};
use crate::expr::lower_translation_key_ref;
use crate::helpers::span_of;
use crate::AnalyzeError;

/// Lower a `SurfaceAst` (parser output) into the canonical `ir::Surface`
/// per `docs/proposals/lzx-integration-codegen.md` §5 + §5.2.
pub fn lower_surface(ast: &syntax::SurfaceAst) -> Result<ir::Surface, AnalyzeError> {
    let target = match ast.target {
        syntax::SurfaceTargetAst::Web => ir::SurfaceTarget::Web,
        syntax::SurfaceTargetAst::Mobile => ir::SurfaceTarget::Mobile,
    };
    let owning_feature = ast
        .uses_feature
        .clone()
        .unwrap_or_else(|| ast.feature.clone());

    let mut audiences = Vec::with_capacity(ast.audiences.len());
    for audience in &ast.audiences {
        audiences.push(lower_audience_ast(audience, &owning_feature)?);
    }

    Ok(ir::Surface {
        feature: owning_feature,
        target,
        audiences,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_audience_ast(
    ast: &syntax::AudienceAst,
    owning_feature: &str,
) -> Result<ir::Audience, AnalyzeError> {
    let requires = ast
        .requires
        .iter()
        .map(|atom| ir::PolicyAtom {
            namespace: atom.namespace.clone(),
            name: atom.name.clone(),
            args: atom.args.clone(),
        })
        .collect();

    let mut views = Vec::with_capacity(ast.views.len());
    for view in &ast.views {
        views.push(lower_view_ast(view, owning_feature)?);
    }
    Ok(ir::Audience {
        name: ast.name.clone(),
        requires,
        views,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_view_ast(ast: &syntax::ViewAst, owning_feature: &str) -> Result<ir::View, AnalyzeError> {
    match ast {
        syntax::ViewAst::List(v) => {
            let source =
                parse_query_ref(&v.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
                    view: v.name.clone(),
                    value: v.source.clone(),
                })?;
            let render = lower_list_render(v);
            let render_columns = match &render {
                ir::ListRender::Table { columns } => columns.as_slice(),
                ir::ListRender::Cells { .. } => &[],
            };
            validate_cells(&v.cells, render_columns, &v.name)?;
            let actions = v
                .actions
                .iter()
                .map(|s| parse_command_ref(s, owning_feature))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ir::View::List(ir::ViewList {
                name: v.name.clone(),
                route: v.route.clone(),
                source,
                render,
                search: v.search.as_ref().map(lower_search_decl),
                filter: lower_filter_decls(v),
                cells: v
                    .cells
                    .iter()
                    .map(|c| ir::CellBinding {
                        field: c.field.clone(),
                        slot: c.slot.clone(),
                    })
                    .collect(),
                actions,
                drawer: v
                    .drawer
                    .as_ref()
                    .map(|drawer| lower_drawer(drawer, owning_feature))
                    .transpose()?,
                sort: v.sort.as_ref().map(lower_sort_decl),
                selection: v
                    .selection
                    .as_ref()
                    .map(|selection| lower_selection_decl(selection, owning_feature))
                    .transpose()?,
                settings: v.settings.iter().map(lower_setting_decl).collect(),
                redacted_fields: v.redacted_fields.clone(),
                span_ref: Some(span_of(v.span)),
            }))
        }
        syntax::ViewAst::Detail(v) => {
            let source =
                parse_query_ref(&v.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
                    view: v.name.clone(),
                    value: v.source.clone(),
                })?;
            // Detail views bind cells against fields on the source resource,
            // not against the `sections` enumeration. The source-resource
            // cross-check happens at doctor time (`lzx-source-resource-mismatch`).
            // We only validate cell slot identifier shape here.
            validate_cells_slot_only(&v.cells, &v.name)?;
            // Route param ↔ placeholder cross-check (`lzx-route-param-*`).
            if let Some(path) = v.route.as_ref() {
                let placeholders = path_placeholders(path);
                for placeholder in &placeholders {
                    if !v.route_params.iter().any(|p| &p.name == placeholder) {
                        return Err(AnalyzeError::LzxRouteParamMissingBinding {
                            view: v.name.clone(),
                            placeholder: placeholder.clone(),
                        });
                    }
                }
                for param in &v.route_params {
                    if !placeholders.iter().any(|p| p == &param.name) {
                        return Err(AnalyzeError::LzxRouteParamOrphan {
                            view: v.name.clone(),
                            param: param.name.clone(),
                        });
                    }
                }
            }
            let actions = v
                .actions
                .iter()
                .map(|s| parse_command_ref(s, owning_feature))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ir::View::Detail(ir::ViewDetail {
                name: v.name.clone(),
                route: v.route.clone(),
                source,
                route_params: v
                    .route_params
                    .iter()
                    .map(|p| ir::RouteParam {
                        name: p.name.clone(),
                        type_ref: p.type_ref.clone(),
                    })
                    .collect(),
                sections: v.sections.clone(),
                cells: v
                    .cells
                    .iter()
                    .map(|c| ir::CellBinding {
                        field: c.field.clone(),
                        slot: c.slot.clone(),
                    })
                    .collect(),
                actions,
                redacted_fields: v.redacted_fields.clone(),
                span_ref: Some(span_of(v.span)),
            }))
        }
        syntax::ViewAst::Create(v) => {
            let submit = parse_command_ref(&v.submit, owning_feature)?;
            validate_cells(&v.cells, &v.fields, &v.name)?;
            Ok(ir::View::Create(ir::ViewCreate {
                name: v.name.clone(),
                route: v.route.clone(),
                submit,
                on_success: v
                    .on_success
                    .as_ref()
                    .map(|spec| lower_on_success_spec(owning_feature, spec)),
                fields: v.fields.clone(),
                cells: v
                    .cells
                    .iter()
                    .map(|c| ir::CellBinding {
                        field: c.field.clone(),
                        slot: c.slot.clone(),
                    })
                    .collect(),
                redacted_fields: v.redacted_fields.clone(),
                span_ref: Some(span_of(v.span)),
            }))
        }
    }
}

fn lower_on_success_spec(feature: &str, spec: &syntax::OnSuccessSpecAst) -> ir::OnSuccessSpec {
    ir::OnSuccessSpec {
        back: spec.back,
        redirect: spec.redirect.clone(),
        flash: spec.flash.as_ref().map(|flash| ir::FlashSpec {
            kind: flash.kind.clone(),
            message_key: lower_translation_key_ref(&flash.message_key),
        }),
        invalidates: spec
            .invalidates
            .iter()
            .map(|inv| ir::InvalidatesSpec {
                query: lower_invalidates_query_ref(feature, &inv.query),
                args: inv.args.iter().map(lower_named_arg).collect(),
            })
            .collect(),
        replace: spec.replace,
    }
}

fn lower_list_render(ast: &syntax::ViewListAst) -> ir::ListRender {
    match (ast.columns.is_empty(), ast.cells_slot.as_ref()) {
        (false, None) => ir::ListRender::Table {
            columns: ast.columns.clone(),
        },
        (true, Some(slot)) => ir::ListRender::Cells { slot: slot.clone() },
        (false, Some(_)) | (true, None) => ir::ListRender::Table {
            columns: ast.columns.clone(),
        },
    }
}

fn lower_filter_decls(ast: &syntax::ViewListAst) -> Vec<ir::FilterDecl> {
    let mut filters: Vec<ir::FilterDecl> = ast.filters.iter().map(lower_filter_decl).collect();
    filters.extend(ast.filter.iter().map(|name| ir::FilterDecl {
        name: name.clone(),
        type_ref: String::new(),
        cardinality: ir::FilterCardinality::Single,
        url_sync: false,
        span_ref: None,
    }));
    filters
}

fn lower_filter_decl(ast: &syntax::FilterDeclAst) -> ir::FilterDecl {
    ir::FilterDecl {
        name: ast.name.clone(),
        type_ref: ast.type_ref.clone(),
        cardinality: match ast.cardinality {
            syntax::FilterCardinalityAst::Single => ir::FilterCardinality::Single,
            syntax::FilterCardinalityAst::Multi => ir::FilterCardinality::Multi,
        },
        url_sync: ast.url_sync,
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_search_decl(ast: &syntax::SearchDeclAst) -> ir::SearchDecl {
    ir::SearchDecl {
        mode: match &ast.mode {
            syntax::SearchModeAst::Columns(columns) => ir::SearchMode::Columns {
                columns: columns.clone(),
            },
            syntax::SearchModeAst::Segmented => ir::SearchMode::Segmented,
        },
        fields: ast.fields.iter().map(lower_search_field).collect(),
        free_text_target: ast.free_text_target.as_ref().map(lower_binding_ref),
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_search_field(ast: &syntax::SearchFieldAst) -> ir::SearchField {
    ir::SearchField {
        key: ast.key.clone(),
        binds_to: lower_binding_ref(&ast.binds_to),
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_binding_ref(ast: &syntax::BindingRefAst) -> ir::BindingRef {
    match ast {
        syntax::BindingRefAst::Filter { name } => ir::BindingRef::Filter { name: name.clone() },
        syntax::BindingRefAst::SourceInput { name } => {
            ir::BindingRef::SourceInput { name: name.clone() }
        }
        syntax::BindingRefAst::SelectionScalar => ir::BindingRef::SelectionScalar,
    }
}

fn lower_drawer(
    ast: &syntax::DrawerSubViewAst,
    owning_feature: &str,
) -> Result<ir::DrawerSubView, AnalyzeError> {
    let source = parse_query_ref(&ast.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
        view: ast.name.clone(),
        value: ast.source.clone(),
    })?;
    validate_cells_slot_only(&ast.cells, &ast.name)?;
    let actions = ast
        .actions
        .iter()
        .map(|s| parse_command_ref(s, owning_feature))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ir::DrawerSubView {
        name: ast.name.clone(),
        trigger: match ast.trigger {
            syntax::DrawerTriggerAst::Select => ir::DrawerTrigger::Select,
            syntax::DrawerTriggerAst::ManualOpen => ir::DrawerTrigger::ManualOpen,
        },
        source,
        route_binding: ast.route_binding.as_ref().map(lower_drawer_route_binding),
        sections: ast.sections.clone(),
        cells: ast
            .cells
            .iter()
            .map(|c| ir::CellBinding {
                field: c.field.clone(),
                slot: c.slot.clone(),
            })
            .collect(),
        actions,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_drawer_route_binding(ast: &syntax::DrawerRouteBindingAst) -> ir::DrawerRouteBinding {
    ir::DrawerRouteBinding {
        target: ast.target.clone(),
        source: match ast.source {
            syntax::DrawerBindingSourceAst::Selection => ir::DrawerBindingSource::Selection,
        },
    }
}

fn lower_sort_decl(ast: &syntax::SortDeclAst) -> ir::SortDecl {
    ir::SortDecl {
        allowed: ast.allowed.clone(),
        default_field: ast.default_field.clone(),
        default_dir: match ast.default_dir {
            syntax::SortDirAst::Asc => ir::SortDir::Asc,
            syntax::SortDirAst::Desc => ir::SortDir::Desc,
        },
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_selection_decl(
    ast: &syntax::SelectionDeclAst,
    owning_feature: &str,
) -> Result<ir::SelectionDecl, AnalyzeError> {
    let bulk_actions = ast
        .bulk_actions
        .iter()
        .map(|s| parse_command_ref(s, owning_feature))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ir::SelectionDecl {
        mode: match ast.mode {
            syntax::SelectionModeAst::None => ir::SelectionMode::None,
            syntax::SelectionModeAst::Single => ir::SelectionMode::Single,
            syntax::SelectionModeAst::Multi => ir::SelectionMode::Multi,
        },
        bulk_actions,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_setting_decl(ast: &syntax::SettingDeclAst) -> ir::SettingDecl {
    ir::SettingDecl {
        name: ast.name.clone(),
        value_space: match &ast.value_space {
            syntax::SettingValueSpaceAst::Enum(values) => ir::SettingValueSpace::Enum {
                values: values.clone(),
            },
            syntax::SettingValueSpaceAst::Bool => ir::SettingValueSpace::Bool,
            syntax::SettingValueSpaceAst::Int { min, max } => ir::SettingValueSpace::Int {
                min: min.unwrap_or(i64::MIN),
                max: max.unwrap_or(i64::MAX),
            },
        },
        default: ast.default.clone(),
        persistence: match ast.persistence {
            syntax::SettingPersistenceAst::None => ir::SettingPersistence::None,
            syntax::SettingPersistenceAst::Local => ir::SettingPersistence::Local,
            syntax::SettingPersistenceAst::Workspace => ir::SettingPersistence::Workspace,
        },
        span_ref: Some(span_of(ast.span)),
    }
}

/// Validate that every cell `field` shows up in the view's column /
/// fields list (proposal §5.2 + doctor rule `lzx-cell-slot-orphan`).
/// Slot names are restricted to kebab/snake identifiers (defensive —
/// parser already enforces).
fn validate_cells(
    cells: &[syntax::CellBindingAst],
    field_universe: &[String],
    view_name: &str,
) -> Result<(), AnalyzeError> {
    for cell in cells {
        validate_cell_slot_shape(&cell.slot, view_name)?;
        if !field_universe.is_empty() && !field_universe.contains(&cell.field) {
            return Err(AnalyzeError::LzxCellSlotOrphan {
                view: view_name.to_owned(),
                field: cell.field.clone(),
            });
        }
    }
    Ok(())
}

/// Cell-slot identifier validation only (skip the field-universe
/// orphan check). Used for `view detail` where cells bind against
/// fields on the source resource, not against the section enum.
fn validate_cells_slot_only(
    cells: &[syntax::CellBindingAst],
    view_name: &str,
) -> Result<(), AnalyzeError> {
    for cell in cells {
        validate_cell_slot_shape(&cell.slot, view_name)?;
    }
    Ok(())
}

fn validate_cell_slot_shape(slot: &str, view_name: &str) -> Result<(), AnalyzeError> {
    if slot.is_empty()
        || !slot
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(AnalyzeError::LzxCellSlotInvalid {
            view: view_name.to_owned(),
            slot: slot.to_owned(),
        });
    }
    Ok(())
}

/// Extract `:name` placeholders from a route path like `/slugs/:key`.
fn path_placeholders(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        if let Some(rest) = segment.strip_prefix(':') {
            // Trim any non-ident tail (the segment could theoretically
            // carry a suffix; v0 keeps it strict so anything after the
            // name is a parse-time error already).
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out
}

/// Parse `<feature>.query.<short>` into a `QueryRef`. The middle token
/// disambiguates list / lookup / sql when the source pre-qualifies it
/// (`feat.query.list.mine` / `feat.query.lookup.by_key` /
/// `feat.query.sql.raw` / `feat.query.view.home`).
/// The shorter form `<feat>.query.<short>` defaults to `List`.
fn parse_query_ref(text: &str) -> Option<ir::QueryRef> {
    let parts: Vec<&str> = text.split('.').collect();
    match parts.as_slice() {
        [feature, "query", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::List,
            name: (*name).to_owned(),
        }),
        [feature, "query", "list", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::List,
            name: (*name).to_owned(),
        }),
        [feature, "query", "lookup", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::Lookup,
            name: (*name).to_owned(),
        }),
        [feature, "query", "sql", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::Sql,
            name: (*name).to_owned(),
        }),
        [feature, "query", "view", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::View,
            name: (*name).to_owned(),
        }),
        _ => None,
    }
}

/// Parse a command reference from a `submit ` line or an `actions`
/// list entry. Accepts both the qualified form (`feat.command.name`)
/// and the bare local form (`name`) — the latter assumes the owning
/// feature.
fn parse_command_ref(text: &str, owning_feature: &str) -> Result<ir::CommandRef, AnalyzeError> {
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.as_slice() {
        [feature, "command", name] => Ok(ir::CommandRef {
            feature: (*feature).to_owned(),
            name: (*name).to_owned(),
        }),
        [name] if !name.is_empty() => Ok(ir::CommandRef {
            feature: owning_feature.to_owned(),
            name: (*name).to_owned(),
        }),
        _ => Err(AnalyzeError::LzxBadCommandRef {
            value: trimmed.to_owned(),
        }),
    }
}
