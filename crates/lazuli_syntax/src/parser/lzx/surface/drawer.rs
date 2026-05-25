//! `view list` drawer sub-view — a side panel triggered by row selection.
//!
//! Drawer is the only `view`-shaped child a `view list` may declare; it
//! reuses the same body-line alphabet as the parent view but with a
//! reduced surface:
//!
//! ```text
//! drawer customer-details on select
//!   source customers.query.detail
//!   route id from selection
//!   sections summary, address
//!   cells avatar @client.avatar
//!   actions archive, send_email
//! ```
//!
//! `on <select|open>` is the trigger. `route <key> from selection`
//! binds a single key in the inner view's path params from the parent
//! list's current selection — drawers cannot be nested.
//!
//! Internally `parse_drawer_block` runs its own `ViewBodyState`
//! and dispatches `source` / `sections` / `cells` / `actions` lines
//! to the same parsers the parent view uses, so a drawer is a
//! sub-view contract, not a separate dialect.

use crate::ast::{
    DrawerBindingSourceAst, DrawerRouteBindingAst, DrawerSubViewAst, DrawerTriggerAst, Span,
};

use super::super::super::common::{
    SourceLine, is_kebab_or_snake_ident, is_trivia, line_error, line_error_owned,
    strip_inline_comment,
};
use super::super::super::error::ParseError;
use super::{
    ViewBodyState, parse_view_actions_line, parse_view_cells_line, parse_view_sections_line,
    parse_view_source_line,
};

pub(crate) fn parse_drawer_block(
    lines: &[SourceLine<'_>],
    start: usize,
    drawer_indent: usize,
    header_rest: &str,
) -> Result<(DrawerSubViewAst, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header_rest.split_whitespace().collect();
    if parts.len() != 3 || parts[1] != "on" {
        return Err(line_error(
            header,
            "drawer blocks use `drawer <name> on select|open`",
        ));
    }
    let name = parts[0].to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            header,
            format!("drawer name `{}` must be kebab/snake identifier", name),
        ));
    }
    let trigger = match parts[2] {
        "select" => DrawerTriggerAst::Select,
        "open" => DrawerTriggerAst::ManualOpen,
        _ => {
            return Err(line_error(
                header,
                "drawer trigger must be `select` or `open`",
            ));
        }
    };

    let child_indent = drawer_indent + 2;
    let mut state = ViewBodyState::default();
    let mut route_binding = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= drawer_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "drawer body lines use one indentation level deeper than the `drawer` header",
            ));
        }
        if raw.contains("+=") || raw.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole drawer",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed.starts_with("drawer ") {
            return Err(line_error(line, "drawer cannot be nested"));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            parse_view_source_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            if route_binding.is_some() {
                return Err(line_error(line, "drawer declares `route` at most once"));
            }
            route_binding = Some(parse_drawer_route_binding(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            parse_view_sections_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            parse_drawer_cells_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            parse_view_actions_line(line, rest.trim(), &mut state)?;
        } else {
            return Err(line_error_owned(
                line,
                format!(
                    "drawer body lines are `source`, `route <key> from selection`, `sections`, `cells <field> @client.<slot>`, or `actions` declarations (got `{}`)",
                    trimmed
                ),
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        DrawerSubViewAst {
            name,
            trigger,
            source: state.source.ok_or_else(|| {
                line_error(
                    header,
                    "drawer requires a `source <feature>.query.<name>` line",
                )
            })?,
            route_binding,
            sections: state.sections,
            cells: state.cells,
            actions: state.actions,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_drawer_cells_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if rest.split_whitespace().count() != 2 {
        return Err(line_error(
            line,
            "drawer cells use `cells <field> @client.<slot>`",
        ));
    }
    parse_view_cells_line(line, rest, state)
}

fn parse_drawer_route_binding(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<DrawerRouteBindingAst, ParseError> {
    let (target, source) = value.rsplit_once(" from ").ok_or_else(|| {
        line_error(
            line,
            "drawer route binding must be `route <key> from selection`",
        )
    })?;
    let target = target.trim();
    if target.is_empty() {
        return Err(line_error(
            line,
            "drawer route binding requires a target key",
        ));
    }
    if !is_kebab_or_snake_ident(target) {
        return Err(line_error_owned(
            line,
            format!(
                "drawer route target `{}` must be kebab/snake identifier",
                target
            ),
        ));
    }
    if source.trim() != "selection" {
        return Err(line_error(
            line,
            "drawer route binding source must be `from selection`",
        ));
    }
    Ok(DrawerRouteBindingAst {
        target: target.to_owned(),
        source: DrawerBindingSourceAst::Selection,
    })
}
