//! `view list | detail | create` block parser — the central dispatcher.
//!
//! A `view` is the unit of surface presentation an audience renders.
//! Three kinds share the same header shape (`view <kind> <name> [at
//! "<path>"]`) but admit different body alphabets:
//!
//! ```text
//! view list customers at "/customers"
//!   source customers.query.list
//!   columns name, email, status
//!   search title
//!   filter status, owner_id
//!   filters
//!     status: list of UserStatus from query
//!   sort
//!     by created_at, name
//!     default created_at desc
//!   selection multi
//!   bulk_actions archive, delete
//!   settings
//!     density: Enum [comfortable, compact] default comfortable
//!   drawer customer-details on select
//!     ...
//!   actions create
//!
//! view detail customers at "/customers/:id"
//!   source customers.query.detail
//!   route id: CustomerId from path
//!   sections summary, address
//!   cells avatar @client.avatar
//!
//! view create customers at "/customers/new"
//!   submit customers.command.create
//!   on_success
//!     back
//!     flash success @translation.customer.created
//!   fields name, email redacted
//! ```
//!
//! The dispatcher inside `parse_view_block` walks each body line,
//! rejects keywords that don't belong in the active kind, and feeds
//! a shared `ViewBodyState` (defined in `super`). At the bottom of
//! the block it crystallises that state into the matching variant of
//! `ViewAst`.
//!
//! Selection helpers (`assemble_selection_decl`,
//! `reject_list_only_view_body`) and the header tail splitter
//! (`parse_view_header_tail`) live here too — they're internal to
//! the view dispatcher.

use super::super::super::common::{
    SourceLine, find_top_level_token, is_kebab_or_snake_ident, is_trivia, line_error,
    line_error_owned, strip_inline_comment, unquote_lzx_value,
};
use super::super::super::error::ParseError;
use super::{
    ViewBodyState, parse_board_block, parse_drawer_block, parse_filters_block,
    parse_inline_table_line, parse_on_success_block, parse_repeatable_group_line,
    parse_tab_group_block, parse_view_mode_block, parse_view_search_decl,
    parse_view_settings_block, parse_view_sort_block, parse_wizard_steps_line, view_body_handlers,
};
use crate::ast::{
    SelectionDeclAst, SelectionModeAst, Span, ViewAst, ViewCreateAst, ViewDetailAst, ViewListAst,
};

/// Parse one of `view list`, `view detail`, `view create` blocks.
pub(super) fn parse_view_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(ViewAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let (kind, after_kind) = if let Some(rest) = header_text.strip_prefix("view list ") {
        ("list", rest)
    } else if let Some(rest) = header_text.strip_prefix("view detail ") {
        ("detail", rest)
    } else if let Some(rest) = header_text.strip_prefix("view create ") {
        ("create", rest)
    } else {
        return Err(line_error(
            header,
            "view header is `view list|detail|create <name> [at \"<path>\"]`",
        ));
    };

    let (name, route) = parse_view_header_tail(header, after_kind)?;
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            header,
            format!("view name `{}` must be kebab-case or snake_case", name),
        ));
    }
    let body_indent = parent_indent + 2;

    // Collect raw children; dispatch into the kind-specific builder.
    let mut state = ViewBodyState::default();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "view body lines use one indentation level deeper than the `view` header",
            ));
        }
        if raw.contains("+=") || raw.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();

        if let Some(rest) = trimmed.strip_prefix("drawer ") {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`drawer` is only valid in `view list` bodies",
                ));
            }
            if state.drawer.is_some() {
                return Err(line_error(
                    line,
                    "view list declares at most one `drawer` block",
                ));
            }
            let (drawer, next) = parse_drawer_block(lines, i, body_indent, rest.trim())?;
            last_end = drawer.span.end;
            state.drawer = Some(drawer);
            i = next;
            continue;
        }

        if trimmed == "filters" {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`filters` block is only valid in `view list`",
                ));
            }
            let (next, block_end) = parse_filters_block(lines, i, body_indent, &mut state)?;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed.starts_with("filters ") {
            return Err(line_error(
                line,
                "`filters` is a block keyword and does not accept inline content",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("search ") {
            if state.search.is_some() {
                return Err(line_error(line, "view declares `search` at most once"));
            }
            let (search, next) = parse_view_search_decl(lines, i, rest.trim(), body_indent)?;
            state.search = Some(search);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "sort" {
            if state.sort.is_some() {
                return Err(line_error(line, "view declares `sort` at most once"));
            }
            let (sort, next, block_end) = parse_view_sort_block(lines, i, body_indent)?;
            state.sort = Some(sort);
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed == "settings" {
            if !state.settings.is_empty() {
                return Err(line_error(line, "view declares `settings` at most once"));
            }
            let (settings, next, block_end) = parse_view_settings_block(lines, i, body_indent)?;
            state.settings = settings;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed.starts_with("persist ") {
            return Err(line_error(
                line,
                "`persist` is valid only as a child of a `settings` declaration",
            ));
        }

        if trimmed == "on_success" {
            if state.on_success.is_some() {
                return Err(line_error(line, "view declares `on_success` at most once"));
            }
            let (on_success, next) = parse_on_success_block(lines, i, body_indent)?;
            last_end = on_success.span.end;
            state.on_success = Some(on_success);
            i = next;
            continue;
        }
        if trimmed.starts_with("on_success ") {
            return Err(line_error(
                line,
                "`on_success` is a block keyword and does not accept inline content",
            ));
        }

        // Wave-W6 view-level UX primitives. `wizard_steps` / `tab_group`
        // attach to list+detail; `view_mode` / `view.inline_table` are
        // list-only (they shape a tabular surface).
        if let Some(rest) = trimmed.strip_prefix("wizard_steps ") {
            parse_wizard_steps_line(line, rest.trim(), &mut state.ux)?;
            last_end = line.end;
            i += 1;
            continue;
        }
        if trimmed.starts_with("tab_group ") || trimmed == "tab_group" {
            let rest = trimmed.strip_prefix("tab_group").unwrap_or("").trim();
            let (next, block_end) =
                parse_tab_group_block(lines, i, body_indent, rest, &mut state.ux)?;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed == "view_mode" {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`view_mode` is only valid in `view list` bodies",
                ));
            }
            let (next, block_end) = parse_view_mode_block(lines, i, body_indent, &mut state.ux)?;
            last_end = block_end;
            i = next;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("view.inline_table ") {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`view.inline_table` is only valid in `view list` bodies",
                ));
            }
            parse_inline_table_line(line, rest.trim(), &mut state.ux)?;
            last_end = line.end;
            i += 1;
            continue;
        }

        // GAP-UX-05 — kanban board (list-only) and repeatable input group
        // (valid in any view kind that hosts inputs/lists).
        if trimmed.starts_with("view.board ") || trimmed == "view.board" {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`view.board` is only valid in `view list` bodies",
                ));
            }
            let rest = trimmed.strip_prefix("view.board").unwrap_or("").trim();
            let (next, block_end) = parse_board_block(lines, i, body_indent, rest, &mut state.ux)?;
            last_end = block_end;
            i = next;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("repeatable input ") {
            parse_repeatable_group_line(line, rest.trim(), &mut state.ux)?;
            last_end = line.end;
            i += 1;
            continue;
        }

        let mut matched = false;
        for (prefix, handler) in view_body_handlers() {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                handler(line, rest.trim(), &mut state)?;
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(line_error_owned(
                line,
                format!(
                    "view body lines are `source`, `submit`, `on_success`, `columns`, `fields`, `search`, `filter`, `sections`, `cells`, `route`, or `actions` declarations (got `{}`)",
                    trimmed
                ),
            ));
        }
        last_end = line.end;
        i += 1;
    }

    let span = Span::new(header.start, last_end);
    let view = match kind {
        "list" => {
            if state.on_success.is_some() {
                return Err(line_error(
                    header,
                    "`on_success` is valid only in submit-backed `view create` bodies",
                ));
            }
            let selection = assemble_selection_decl(&state, span);
            ViewAst::List(ViewListAst {
                name,
                route,
                source: state.source.ok_or_else(|| {
                    line_error(
                        header,
                        "view list requires a `source <feature>.query.<name>` line",
                    )
                })?,
                columns: state.columns,
                search: state.search,
                filter: state.filter,
                filters: state.filters,
                cells_slot: state.cells_slot,
                cells: state.cells,
                drawer: state.drawer,
                sort: state.sort,
                selection,
                settings: state.settings,
                actions: state.actions,
                redacted_fields: state.redacted_fields,
                ux: state.ux,
                span,
            })
        }
        "detail" => {
            reject_list_only_view_body(header, &state, "view detail")?;
            if state.on_success.is_some() {
                return Err(line_error(
                    header,
                    "`on_success` is valid only in submit-backed `view create` bodies",
                ));
            }
            ViewAst::Detail(ViewDetailAst {
                name,
                route,
                source: state.source.ok_or_else(|| {
                    line_error(
                        header,
                        "view detail requires a `source <feature>.query.<name>` line",
                    )
                })?,
                route_params: state.route_params,
                sections: state.sections,
                cells: state.cells,
                actions: state.actions,
                redacted_fields: state.redacted_fields,
                ux: state.ux,
                span,
            })
        }
        "create" => {
            reject_list_only_view_body(header, &state, "view create")?;
            if !state.ux.is_empty() {
                return Err(line_error(
                    header,
                    "`wizard_steps` / `tab_group` / `view_mode` / `view.inline_table` / `view.board` / `repeatable input` are not valid in `view create`",
                ));
            }
            ViewAst::Create(ViewCreateAst {
                name,
                route,
                submit: state.submit.ok_or_else(|| {
                    line_error(
                        header,
                        "view create requires a `submit <feature>.command.<name>` line",
                    )
                })?,
                on_success: state.on_success,
                fields: state.fields,
                cells: state.cells,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        // `kind` was set to `"list" | "detail" | "create"` at line 73-78
        // and is unchanged since; this arm is dead. Treat it defensively as
        // a malformed view header rather than panicking.
        _ => {
            return Err(line_error(
                header,
                "view header is `view list|detail|create <name> [at \"<path>\"]`",
            ));
        }
    };
    Ok((view, i))
}

fn assemble_selection_decl(state: &ViewBodyState, view_span: Span) -> Option<SelectionDeclAst> {
    if let Some(mut selection) = state.selection.clone() {
        selection.bulk_actions = state.bulk_actions.clone();
        Some(selection)
    } else if state.bulk_actions_seen {
        Some(SelectionDeclAst {
            mode: SelectionModeAst::None,
            bulk_actions: state.bulk_actions.clone(),
            span: view_span,
        })
    } else {
        None
    }
}

fn reject_list_only_view_body(
    header: &SourceLine<'_>,
    state: &ViewBodyState,
    kind: &str,
) -> Result<(), ParseError> {
    if state.sort.is_some()
        || state.selection.is_some()
        || state.bulk_actions_seen
        || !state.settings.is_empty()
    {
        return Err(line_error_owned(
            header,
            format!(
                "`sort`, `selection`, `bulk_actions`, and `settings` are valid only in `view list`, not `{}`",
                kind
            ),
        ));
    }
    Ok(())
}

/// Split the `<name> [at "<path>"]` tail of a view header. The optional
/// `at "<...>"` clause carries a quoted route path.
fn parse_view_header_tail(
    header: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<String>), ParseError> {
    let rest = rest.trim();
    if let Some(at_idx) = find_top_level_token(rest, " at ") {
        let name = rest[..at_idx].trim().to_owned();
        if name.is_empty() {
            return Err(line_error(header, "view header requires a name"));
        }
        let after = rest[at_idx + " at ".len()..].trim();
        if !after.starts_with('"') {
            return Err(line_error(
                header,
                "`at` route must be a quoted string (e.g. `at \"/slugs\"`)",
            ));
        }
        let route = unquote_lzx_value(after).to_owned();
        if !route.starts_with('/') {
            return Err(line_error(header, "`at` route path must begin with `/`"));
        }
        Ok((name, Some(route)))
    } else {
        let name = rest.trim().to_owned();
        if name.is_empty() {
            return Err(line_error(header, "view header requires a name"));
        }
        Ok((name, None))
    }
}
