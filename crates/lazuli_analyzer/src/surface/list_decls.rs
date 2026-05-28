//! List-view sub-shape lowerings extracted from `surface/mod.rs`
//! (Rails-style R9 split): list render mode, filter declarations,
//! search declarations + binding refs, drawer sidecar, sort, selection,
//! and setting declarations.
//!
//! Every fn here is `pub(super)`; the parent `surface` module is the
//! only consumer.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use super::{parse_command_ref, validate_cells_slot_only};
use crate::AnalyzeError;
use crate::helpers::span_of;

pub(super) fn lower_list_render(ast: &syntax::ViewListAst) -> ir::ListRender {
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

pub(super) fn lower_filter_decls(ast: &syntax::ViewListAst) -> Vec<ir::FilterDecl> {
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
            syntax::FilterCardinalityAst::DateRange => ir::FilterCardinality::DateRange,
        },
        url_sync: ast.url_sync,
        span_ref: Some(span_of(ast.span)),
    }
}

pub(super) fn lower_search_decl(ast: &syntax::SearchDeclAst) -> ir::SearchDecl {
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

pub(super) fn lower_drawer(
    ast: &syntax::DrawerSubViewAst,
    owning_feature: &str,
) -> Result<ir::DrawerSubView, AnalyzeError> {
    let source =
        super::parse_query_ref(&ast.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
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

pub(super) fn lower_sort_decl(ast: &syntax::SortDeclAst) -> ir::SortDecl {
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

pub(super) fn lower_selection_decl(
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

/// Lower the Wave-W6 view-level UX aggregate (`wizard_steps`, `tab_group`,
/// `view_mode`, `view.inline_table`). Unknown render-mode keywords are
/// dropped here (doctor `LZX-VIEW-MODE-001` flags them); the `inline_table`
/// command ref is normalized via `parse_command_ref`.
pub(super) fn lower_view_ux(
    ast: &syntax::ViewUxAst,
    owning_feature: &str,
    view_name: &str,
) -> Result<ir::ViewUx, AnalyzeError> {
    let mut view_modes = Vec::with_capacity(ast.view_modes.len());
    for mode in &ast.view_modes {
        let parsed =
            ir::RenderMode::parse(mode).ok_or_else(|| AnalyzeError::LzxUnknownRenderMode {
                view: view_name.to_owned(),
                mode: mode.clone(),
            })?;
        view_modes.push(parsed);
    }
    let inline_table = match &ast.inline_table {
        Some(inline) => {
            let bare = inline
                .on_change
                .trim()
                .strip_prefix("@command.")
                .unwrap_or(inline.on_change.trim());
            Some(ir::InlineTable {
                on_change: parse_command_ref(bare, owning_feature)?,
                span_ref: Some(span_of(inline.span)),
            })
        }
        None => None,
    };
    Ok(ir::ViewUx {
        wizard_steps: ast.wizard_steps.as_ref().map(|w| ir::WizardSteps {
            total: w.total,
            current_field: w.current_field.clone(),
            span_ref: Some(span_of(w.span)),
        }),
        tab_group: ast.tab_group.as_ref().map(|g| ir::TabGroup {
            derived_from: g.derived_from.clone(),
            cases: g
                .cases
                .iter()
                .map(|c| ir::TabGroupCase {
                    variants: c.variants.clone(),
                    label: c.label.clone(),
                    span_ref: Some(span_of(c.span)),
                })
                .collect(),
            span_ref: Some(span_of(g.span)),
        }),
        view_modes,
        inline_table,
        board: ast.board.as_ref().map(|b| ir::Board {
            name: b.name.clone(),
            lanes_source: b.lanes_source.clone(),
            span_ref: Some(span_of(b.span)),
        }),
        repeatable_groups: ast
            .repeatable_groups
            .iter()
            .map(|g| ir::RepeatableGroup {
                name: g.name.clone(),
                fields: g
                    .fields
                    .iter()
                    .map(|f| ir::RepeatableField {
                        name: f.name.clone(),
                        type_name: f.type_name.clone(),
                    })
                    .collect(),
                sum_field: g.sum_field.clone(),
                sum_target: g.sum_target.clone(),
                span_ref: Some(span_of(g.span)),
            })
            .collect(),
    })
}

/// Lower the Wave-W6 audience-level UX aggregate (`tabs`, `wizard`).
pub(super) fn lower_audience_ux(ast: &syntax::AudienceUxAst) -> ir::AudienceUx {
    ir::AudienceUx {
        tabs: ast
            .tabs
            .iter()
            .map(|t| ir::Tabs {
                entries: t
                    .entries
                    .iter()
                    .map(|e| ir::TabEntry {
                        label: e.label.clone(),
                        view: e.view.clone(),
                        audience: e.audience.clone(),
                        span_ref: Some(span_of(e.span)),
                    })
                    .collect(),
                span_ref: Some(span_of(t.span)),
            })
            .collect(),
        wizards: ast
            .wizards
            .iter()
            .map(|w| ir::Wizard {
                name: w.name.clone(),
                steps: w
                    .steps
                    .iter()
                    .map(|s| ir::WizardStep {
                        index: s.index,
                        ref_name: s.ref_name.clone(),
                        span_ref: Some(span_of(s.span)),
                    })
                    .collect(),
                span_ref: Some(span_of(w.span)),
            })
            .collect(),
    }
}

pub(super) fn lower_setting_decl(ast: &syntax::SettingDeclAst) -> ir::SettingDecl {
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
