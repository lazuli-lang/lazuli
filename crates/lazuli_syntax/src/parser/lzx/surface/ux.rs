//! Wave-W6 surface UX primitives — view-level (`wizard_steps`, `tab_group`,
//! `view_mode`, `view.inline_table`) and audience-level (`tabs`, `wizard`).
//!
//! These close pauta UX gaps GAP-UX-01..04. The view-level helpers mutate the
//! shared [`ViewBodyState`]; the audience-level helpers return their own AST
//! and are dispatched from `mod.rs`.
//!
//! ```text
//! # view-level (inside a `view list|detail` body)
//! wizard_steps 3 current registration_step
//! tab_group derived_from vehicle_type
//!   case TV, RADIO -> tab "Broadcast"
//!   case PRINT -> tab "Print"
//! view_mode
//!   table
//!   kanban
//! view.inline_table on_change update_row
//!
//! # audience-level (sibling to `view`)
//! tabs
//!   tab "Details" -> view detail
//!   tab "History" -> view history audience admin
//! wizard job_create steps
//!   step 1: job_basics
//!   step 2: job_targeting
//! ```

use super::super::super::common::{
    SourceLine, is_kebab_or_snake_ident, is_trivia, line_error, line_error_owned, split_lzx_arrow,
    split_lzx_list, strip_inline_comment, unquote_lzx_value,
};
use super::super::super::error::ParseError;
use crate::ast::{
    BoardAst, InlineTableAst, RepeatableFieldAst, RepeatableGroupAst, Span, TabEntryAst,
    TabGroupAst, TabGroupCaseAst, TabsAst, ViewUxAst, WizardAst, WizardStepAst, WizardStepsAst,
};

// ===========================================================================
// View-level primitives (mutate ViewBodyState)
// ===========================================================================

/// `wizard_steps <total> current <field>` — single line. GAP-UX-01.
pub(in crate::parser::lzx) fn parse_wizard_steps_line(
    line: &SourceLine<'_>,
    rest: &str,
    ux: &mut ViewUxAst,
) -> Result<(), ParseError> {
    if ux.wizard_steps.is_some() {
        return Err(line_error(
            line,
            "view declares `wizard_steps` at most once",
        ));
    }
    let (total_raw, current_raw) = rest.split_once(" current ").ok_or_else(|| {
        line_error(
            line,
            "`wizard_steps` must be `wizard_steps <total> current <field>`",
        )
    })?;
    let total: u32 = total_raw.trim().parse().map_err(|_| {
        line_error(
            line,
            "`wizard_steps` total must be a positive integer literal",
        )
    })?;
    if total == 0 {
        return Err(line_error(
            line,
            "`wizard_steps` total must be a positive integer literal",
        ));
    }
    let current_field = current_raw.trim().to_owned();
    if !is_kebab_or_snake_ident(&current_field) {
        return Err(line_error_owned(
            line,
            format!(
                "`wizard_steps current` field `{}` must be a kebab/snake identifier",
                current_field
            ),
        ));
    }
    ux.wizard_steps = Some(WizardStepsAst {
        total,
        current_field,
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

/// `view.inline_table on_change <name>` — single line. GAP-UX-04.
pub(in crate::parser::lzx) fn parse_inline_table_line(
    line: &SourceLine<'_>,
    rest: &str,
    ux: &mut ViewUxAst,
) -> Result<(), ParseError> {
    if ux.inline_table.is_some() {
        return Err(line_error(
            line,
            "view declares `view.inline_table` at most once",
        ));
    }
    let command = rest.strip_prefix("on_change ").ok_or_else(|| {
        line_error(
            line,
            "`view.inline_table` must be `view.inline_table on_change <command>`",
        )
    })?;
    let command = command.trim();
    // SPEC-02 — commands are referenced BARE everywhere in the language; the
    // `@command.<name>` sigil (the only `@command` usage) is retired here.
    if command.starts_with("@command.") {
        return Err(line_error(
            line,
            "E-AT-COMMAND-RETIRED: the `@command.<name>` sigil was retired (SPEC-02); \
             reference the command bare — `view.inline_table on_change <name>`",
        ));
    }
    if !is_kebab_or_snake_ident(command) {
        return Err(line_error(
            line,
            "`view.inline_table on_change <name>` requires a command identifier",
        ));
    }
    ux.inline_table = Some(InlineTableAst {
        on_change: command.to_owned(),
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

/// `view_mode` block — child lines are bare render-mode keywords. GAP-UX-04.
/// Returns the index of the first unconsumed line + the block's end offset.
pub(in crate::parser::lzx) fn parse_view_mode_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    ux: &mut ViewUxAst,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if !ux.view_modes.is_empty() {
        return Err(line_error(header, "view declares `view_mode` at most once"));
    }
    let child_indent = body_indent + 2;
    let mut modes: Vec<String> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`view_mode` entries use one indentation level deeper than the header",
            ));
        }
        let mode = strip_inline_comment(raw).trim().to_owned();
        if !is_kebab_or_snake_ident(&mode) {
            return Err(line_error_owned(
                line,
                format!(
                    "`view_mode` entry `{}` must be a bare render-mode keyword",
                    mode
                ),
            ));
        }
        if modes.contains(&mode) {
            return Err(line_error_owned(
                line,
                format!("duplicate render mode `{}` in `view_mode`", mode),
            ));
        }
        modes.push(mode);
        last_end = line.end;
        i += 1;
    }

    if modes.is_empty() {
        return Err(line_error(
            header,
            "`view_mode` requires at least one render mode",
        ));
    }
    ux.view_modes = modes;
    Ok((i, last_end))
}

/// `tab_group derived_from <field>` block — child lines are `case` arms.
/// GAP-UX-02. Returns the first unconsumed index + the block end offset.
pub(in crate::parser::lzx) fn parse_tab_group_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    header_rest: &str,
    ux: &mut ViewUxAst,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if ux.tab_group.is_some() {
        return Err(line_error(header, "view declares `tab_group` at most once"));
    }
    let derived_from = header_rest
        .strip_prefix("derived_from ")
        .map(str::trim)
        .ok_or_else(|| {
            line_error(
                header,
                "`tab_group` header is `tab_group derived_from <field>`",
            )
        })?;
    if !is_kebab_or_snake_ident(derived_from) {
        return Err(line_error_owned(
            header,
            format!(
                "`tab_group derived_from` field `{}` must be a kebab/snake identifier",
                derived_from
            ),
        ));
    }

    let child_indent = body_indent + 2;
    let mut cases: Vec<TabGroupCaseAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`tab_group` cases use one indentation level deeper than the header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim();
        cases.push(parse_tab_group_case(line, trimmed)?);
        last_end = line.end;
        i += 1;
    }

    if cases.is_empty() {
        return Err(line_error(
            header,
            "`tab_group` requires at least one `case ... -> tab \"...\"`",
        ));
    }
    ux.tab_group = Some(TabGroupAst {
        derived_from: derived_from.to_owned(),
        cases,
        span: Span::new(header.start, last_end),
    });
    Ok((i, last_end))
}

// Parse one `case <V1, V2> -> tab "<label>"` arm (defined in ux_p1.rs).
include!("ux_p1.rs");
include!("ux_p2.rs");
