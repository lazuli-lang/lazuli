//! Per-view body state + the dispatch table of view-body line handlers.
//!
//! Every view kind (list/detail/create) shares the same body grammar
//! (`source`, `submit`, `columns`, `fields`, `filter`, `sections`,
//! `selection`, `bulk_actions`, `actions`, `cells`, `route`). The shared
//! state machine lives here so each view kind reads the same accumulator
//! and dispatches through the same handler table.
//!
//! Each handler is a `fn(&SourceLine, &str, &mut ViewBodyState)` —
//! adding a new keyword is a one-line entry in [`view_body_handlers`].
//! Handlers MUST be idempotent against missing siblings: structural
//! validity (required `source` etc.) is the caller's job.

use crate::ast::{
    CellBindingAst, DrawerSubViewAst, FilterDeclAst, OnSuccessSpecAst, RouteParamAst,
    SearchDeclAst, SelectionDeclAst, SelectionModeAst, SettingDeclAst, SortDeclAst, Span,
};

use super::super::super::common::{
    SourceLine, is_kebab_or_snake_ident, line_error, line_error_owned, split_lzx_list,
};
use super::super::super::error::ParseError;

#[derive(Default)]
pub(in crate::parser::lzx) struct ViewBodyState {
    pub(in crate::parser::lzx) source: Option<String>,
    pub(in crate::parser::lzx) submit: Option<String>,
    pub(in crate::parser::lzx) columns: Vec<String>,
    pub(in crate::parser::lzx) search: Option<SearchDeclAst>,
    pub(in crate::parser::lzx) filter: Vec<String>,
    pub(in crate::parser::lzx) filters: Vec<FilterDeclAst>,
    pub(in crate::parser::lzx) has_filters_block: bool,
    pub(in crate::parser::lzx) fields: Vec<String>,
    pub(in crate::parser::lzx) sections: Vec<String>,
    pub(in crate::parser::lzx) cells_slot: Option<String>,
    pub(in crate::parser::lzx) cells: Vec<CellBindingAst>,
    pub(in crate::parser::lzx) actions: Vec<String>,
    pub(in crate::parser::lzx) route_params: Vec<RouteParamAst>,
    pub(in crate::parser::lzx) drawer: Option<DrawerSubViewAst>,
    pub(in crate::parser::lzx) on_success: Option<OnSuccessSpecAst>,
    pub(in crate::parser::lzx) sort: Option<SortDeclAst>,
    pub(in crate::parser::lzx) selection: Option<SelectionDeclAst>,
    pub(in crate::parser::lzx) bulk_actions: Vec<String>,
    pub(in crate::parser::lzx) bulk_actions_seen: bool,
    pub(in crate::parser::lzx) settings: Vec<SettingDeclAst>,
    pub(in crate::parser::lzx) redacted_fields: Vec<String>,
}

type ViewBodyLineHandler =
    for<'a> fn(&SourceLine<'a>, &str, &mut ViewBodyState) -> Result<(), ParseError>;

pub(in crate::parser::lzx) fn view_body_handlers() -> &'static [(&'static str, ViewBodyLineHandler)]
{
    &[
        ("source ", parse_view_source_line),
        ("submit ", parse_view_submit_line),
        ("columns ", parse_view_columns_line),
        ("fields ", parse_view_fields_line),
        ("filter ", parse_view_filter_line),
        ("sections ", parse_view_sections_line),
        ("selection ", parse_view_selection_line),
        ("bulk_actions ", parse_view_bulk_actions_line),
        ("actions ", parse_view_actions_line),
        ("cells ", parse_view_cells_line),
        ("route ", parse_view_route_line),
    ]
}

pub(in crate::parser::lzx) fn parse_view_source_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.source.is_some() {
        return Err(line_error(line, "view declares `source` at most once"));
    }
    state.source = Some(rest.to_owned());
    Ok(())
}

fn parse_view_submit_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.submit.is_some() {
        return Err(line_error(line, "view declares `submit` at most once"));
    }
    state.submit = Some(rest.to_owned());
    Ok(())
}

fn parse_view_columns_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.columns.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_fields_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if let Some(fields) = rest.strip_suffix(" redacted") {
        let fields = split_lzx_list(fields);
        state.redacted_fields.extend(fields.iter().cloned());
        state.fields.extend(fields);
    } else {
        state.fields.extend(split_lzx_list(rest));
    }
    Ok(())
}

fn parse_view_filter_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.filter.extend(split_lzx_list(rest));
    Ok(())
}

pub(in crate::parser::lzx) fn parse_view_sections_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.sections.extend(split_lzx_list(rest));
    Ok(())
}

pub(in crate::parser::lzx) fn parse_view_actions_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.actions.extend(split_lzx_list(rest));
    Ok(())
}

pub(in crate::parser::lzx) fn parse_view_cells_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    if let Some(slot_rest) = rest.strip_prefix("@client.") {
        let slot = slot_rest.trim();
        if slot.is_empty() {
            return Err(line_error(
                line,
                "`cells @client.<slot>` requires a slot identifier after `@client.`",
            ));
        }
        if slot.split_whitespace().count() > 1 {
            return Err(line_error_owned(
                line,
                format!(
                    "`cells @client.<slot>` accepts only one slot identifier (got `{}`); per-column form is `cells <field> @client.<slot>` and binds a single field",
                    slot
                ),
            ));
        }
        if state.cells_slot.is_some() {
            return Err(line_error(
                line,
                "view declares `cells @client.<slot>` (grid form) at most once",
            ));
        }
        if !is_kebab_or_snake_ident(slot) {
            return Err(line_error_owned(
                line,
                format!("cell slot `{}` must be a kebab/snake identifier", slot),
            ));
        }
        state.cells_slot = Some(slot.to_owned());
        Ok(())
    } else {
        let binding = parse_cell_binding(line, rest)?;
        state.cells.push(binding);
        Ok(())
    }
}

fn parse_view_route_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    let param = parse_route_param(line, rest)?;
    state.route_params.push(param);
    Ok(())
}

fn parse_view_selection_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.selection.is_some() {
        return Err(line_error(line, "view declares `selection` at most once"));
    }
    let mode = match rest {
        "single" => SelectionModeAst::Single,
        "multi" => SelectionModeAst::Multi,
        "none" => {
            return Err(line_error(
                line,
                "`selection none` is not valid; omit the line for no selection",
            ));
        }
        _ => {
            return Err(line_error(
                line,
                "`selection` must be `selection single` or `selection multi`",
            ));
        }
    };
    state.selection = Some(SelectionDeclAst {
        mode,
        bulk_actions: Vec::new(),
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

fn parse_view_bulk_actions_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.bulk_actions_seen {
        return Err(line_error(
            line,
            "view declares `bulk_actions` at most once",
        ));
    }
    let actions = split_lzx_list(rest);
    if actions.is_empty() {
        return Err(line_error(
            line,
            "`bulk_actions` requires at least one command name",
        ));
    }
    state.bulk_actions = actions;
    state.bulk_actions_seen = true;
    Ok(())
}

/// Parse `cells <field> @client.<slot>` — `value` is the text after the
/// `cells ` prefix.
fn parse_cell_binding(line: &SourceLine<'_>, value: &str) -> Result<CellBindingAst, ParseError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(
            line,
            "cell bindings use `cells <field> @client.<slot>`",
        ));
    }
    let field = parts[0].to_owned();
    let slot = parts[1]
        .strip_prefix("@client.")
        .ok_or_else(|| line_error(line, "cell slot must be `@client.<slot>`"))?
        .to_owned();
    if !is_kebab_or_snake_ident(&field) {
        return Err(line_error_owned(
            line,
            format!("cell field `{}` must be a kebab/snake identifier", field),
        ));
    }
    if !is_kebab_or_snake_ident(&slot) {
        return Err(line_error_owned(
            line,
            format!("cell slot `{}` must be a kebab/snake identifier", slot),
        ));
    }
    Ok(CellBindingAst {
        field,
        slot,
        span: Span::new(line.start, line.end),
    })
}

/// Parse `route <name>: <Type> from path` — the path-source clause is
/// mandatory; the lzx grammar reserves `route ... from path` for typed
/// path parameters.
fn parse_route_param(line: &SourceLine<'_>, value: &str) -> Result<RouteParamAst, ParseError> {
    // Pattern: `<name>: <Type> from path`. Split on `from` first so
    // any `:` inside `<Type>` is preserved.
    let (head, source) = value
        .rsplit_once(" from ")
        .ok_or_else(|| line_error(line, "route param must be `route <name>: <Type> from path`"))?;
    if source.trim() != "path" {
        return Err(line_error(line, "route param source must be `from path`"));
    }
    let (name_raw, type_raw) = head
        .split_once(':')
        .ok_or_else(|| line_error(line, "route param must be `route <name>: <Type> from path`"))?;
    let name = name_raw.trim().to_owned();
    let type_ref = type_raw.trim().to_owned();
    if name.is_empty() || type_ref.is_empty() {
        return Err(line_error(
            line,
            "route param requires both a name and a type",
        ));
    }
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            line,
            format!("route param name `{}` must be kebab/snake case", name),
        ));
    }
    Ok(RouteParamAst {
        name,
        type_ref,
        span: Span::new(line.start, line.end),
    })
}
