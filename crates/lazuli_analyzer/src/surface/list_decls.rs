//! List-view sub-shape lowerings extracted from `surface/mod.rs`
//! (Rails-style R9 split): list render mode, filter declarations,
//! search declarations + binding refs, drawer sidecar, sort, selection,
//! and setting declarations.
//!
//! Every fn here is `pub(super)`; the parent `surface` module is the
//! only consumer.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::AnalyzeError;
use crate::helpers::span_of;

use super::parse_command_ref;
use super::validate_cells_slot_only;

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
    let source = super::parse_query_ref(&ast.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
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
