//! `surface.audience` + nested `view` — concrete per-audience shapes.
//!
//! Inside a `surface <experience> <platform>` block, each `audience`
//! declares the visible views for that role on that platform:
//!
//! ```text
//! surface checkout web
//!   audience anonymous
//!     policy @policy.anonymous
//!     view payment list
//!       columns name, price
//!       actions buy
//!   audience member premium
//!     view orders list
//!       columns ...
//! ```
//!
//! `audience <name> [qualifiers...]` opens the block; an optional
//! `policy` line carries the access guard. Inside, `view <name>
//! <type>` declarations are *platform views* — concrete projections
//! that bind columns/fields/sections to the experience-level view of
//! the same name.
//!
//! Platform views accept a closed alphabet of children:
//! `columns | fields | sections | search | filter | cells | actions |
//! submit | block | policy`. Partial overrides (`+=`, `-=`) are
//! rejected at parse time; the grammar demands full redeclaration.

use crate::ast::{AudienceUxAst, FilterDeclAst, LzxAudience, LzxPlatformView, Span, ViewUxAst};

use super::super::super::common::{SourceLine, is_trivia, line_error, split_lzx_list};
use super::super::super::error::ParseError;
use super::super::app::parse_lzx_view_guard;
use super::super::surface::{
    parse_board_block, parse_filters_block_into, parse_inline_table_line,
    parse_repeatable_group_line, parse_tab_group_block, parse_tabs_block, parse_view_mode_block,
    parse_wizard_block, parse_wizard_steps_line,
};

pub(super) fn parse_lzx_audience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxAudience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(line_error(header, "audience blocks use `audience <name>`"));
    }

    let mut views = Vec::new();
    let mut guard = None;
    let mut ux = AudienceUxAst::default();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations or `policy <policy>`",
            ));
        }

        if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "audience declares `policy` at most once"));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 4)?;
            guard = Some(parsed);
            index = next;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_platform_view(lines, index)?;
            views.push(view);
            index = next;
        } else if trimmed == "tabs" {
            // §7a — static tab container (audience sibling to `view`).
            let (tabs, next) = parse_tabs_block(lines, index, 4)?;
            ux.tabs.push(tabs);
            index = next;
        } else if let Some(rest) = trimmed.strip_prefix("wizard ") {
            // §7a — multi-step wizard container (audience sibling to `view`).
            let (wizard, next) = parse_wizard_block(lines, index, 4, rest.trim())?;
            ux.wizards.push(wizard);
            index = next;
        } else {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations, `policy <policy>`, `tabs`, or `wizard <name> steps`",
            ));
        }
    }

    Ok((
        LzxAudience {
            name: parts[1].to_owned(),
            qualifiers: parts[2..].iter().map(|part| (*part).to_owned()).collect(),
            views,
            guard,
            ux,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_platform_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxPlatformView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "platform views use `view <name> <type>`",
        ));
    }

    let mut columns = Vec::new();
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut search = Vec::new();
    let mut filter = Vec::new();
    let mut filters: Vec<FilterDeclAst> = Vec::new();
    let mut has_filters_block = false;
    let mut cells = Vec::new();
    let mut actions = Vec::new();
    let mut submit = None;
    let mut blocks = Vec::new();
    let mut guard = None;
    let mut ux = ViewUxAst::default();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 4 {
            break;
        }

        if line.indent != 6 {
            return Err(line_error(
                line,
                "platform view children use six-space indentation",
            ));
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("columns ") {
            columns = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("fields ") {
            fields = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            sections = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            search = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("filter ") {
            filter = split_lzx_list(rest);
        } else if trimmed == "filters" {
            // G-A1 — typed `filters` block, reusing the surface-dialect
            // parser (`parse_filters_block_into`). Children sit one
            // indentation level deeper (8 spaces); the single-line
            // `filter <list>` form above stays separate.
            let (next, _end) =
                parse_filters_block_into(lines, index, 6, &mut filters, &mut has_filters_block)?;
            index = next;
            continue;
        } else if trimmed.starts_with("filters ") {
            return Err(line_error(
                line,
                "`filters` is a block keyword and does not accept inline content",
            ));
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            cells = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            actions = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(
                    line,
                    "platform view declares `policy` at most once",
                ));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 6)?;
            guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("wizard_steps ") {
            // §7a — view-level UX primitives. The abstract experience
            // dialect has no list/detail/create kind, so every primitive
            // is accepted uniformly; lowering shares the surface-dialect
            // `ViewUxAst`.
            parse_wizard_steps_line(line, rest.trim(), &mut ux)?;
        } else if trimmed == "tab_group" || trimmed.starts_with("tab_group ") {
            let rest = trimmed.strip_prefix("tab_group").unwrap_or("").trim();
            let (next, _end) = parse_tab_group_block(lines, index, 6, rest, &mut ux)?;
            index = next;
            continue;
        } else if trimmed == "view_mode" {
            let (next, _end) = parse_view_mode_block(lines, index, 6, &mut ux)?;
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("view.inline_table ") {
            parse_inline_table_line(line, rest.trim(), &mut ux)?;
        } else if trimmed == "view.board" || trimmed.starts_with("view.board ") {
            let rest = trimmed.strip_prefix("view.board").unwrap_or("").trim();
            let (next, _end) = parse_board_block(lines, index, 6, rest, &mut ux)?;
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("repeatable input ") {
            parse_repeatable_group_line(line, rest.trim(), &mut ux)?;
        } else {
            return Err(line_error(
                line,
                "platform view children are `columns`, `fields`, `sections`, `search`, `filter`, `filters`, `cells`, `actions`, `submit`, `block`, `policy`, `wizard_steps`, `tab_group`, `view_mode`, `view.inline_table`, `view.board`, or `repeatable input`",
            ));
        }

        index += 1;
    }

    Ok((
        LzxPlatformView {
            name: parts[1].to_owned(),
            view_type: parts[2].to_owned(),
            columns,
            fields,
            sections,
            search,
            filter,
            filters,
            cells,
            actions,
            submit,
            blocks,
            guard,
            ux,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}
