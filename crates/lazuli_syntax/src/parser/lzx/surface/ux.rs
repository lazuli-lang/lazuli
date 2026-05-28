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
//! view.inline_table on_change @command.update_row
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
use super::ViewBodyState;
use crate::ast::{
    InlineTableAst, Span, TabEntryAst, TabGroupAst, TabGroupCaseAst, TabsAst, WizardAst,
    WizardStepAst, WizardStepsAst,
};

// ===========================================================================
// View-level primitives (mutate ViewBodyState)
// ===========================================================================

/// `wizard_steps <total> current <field>` — single line. GAP-UX-01.
pub(super) fn parse_wizard_steps_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.ux.wizard_steps.is_some() {
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
    state.ux.wizard_steps = Some(WizardStepsAst {
        total,
        current_field,
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

/// `view.inline_table on_change @command.<name>` — single line. GAP-UX-04.
pub(super) fn parse_inline_table_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.ux.inline_table.is_some() {
        return Err(line_error(
            line,
            "view declares `view.inline_table` at most once",
        ));
    }
    let command = rest.strip_prefix("on_change ").ok_or_else(|| {
        line_error(
            line,
            "`view.inline_table` must be `view.inline_table on_change @command.<name>`",
        )
    })?;
    let command = command.trim();
    if !command.starts_with("@command.") {
        return Err(line_error(
            line,
            "`view.inline_table on_change` must reference `@command.<name>`",
        ));
    }
    let name = &command["@command.".len()..];
    if !is_kebab_or_snake_ident(name) {
        return Err(line_error(
            line,
            "`view.inline_table on_change @command.<name>` requires a command identifier",
        ));
    }
    state.ux.inline_table = Some(InlineTableAst {
        on_change: command.to_owned(),
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

/// `view_mode` block — child lines are bare render-mode keywords. GAP-UX-04.
/// Returns the index of the first unconsumed line + the block's end offset.
pub(super) fn parse_view_mode_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    state: &mut ViewBodyState,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if !state.ux.view_modes.is_empty() {
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
    state.ux.view_modes = modes;
    Ok((i, last_end))
}

/// `tab_group derived_from <field>` block — child lines are `case` arms.
/// GAP-UX-02. Returns the first unconsumed index + the block end offset.
pub(super) fn parse_tab_group_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    header_rest: &str,
    state: &mut ViewBodyState,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if state.ux.tab_group.is_some() {
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
    state.ux.tab_group = Some(TabGroupAst {
        derived_from: derived_from.to_owned(),
        cases,
        span: Span::new(header.start, last_end),
    });
    Ok((i, last_end))
}

/// Parse one `case <V1, V2> -> tab "<label>"` arm.
fn parse_tab_group_case(line: &SourceLine<'_>, value: &str) -> Result<TabGroupCaseAst, ParseError> {
    let rest = value.strip_prefix("case ").ok_or_else(|| {
        line_error(
            line,
            "`tab_group` arms are `case <VARIANTS> -> tab \"<label>\"`",
        )
    })?;
    let (variants_raw, tab_raw) = split_lzx_arrow(rest)
        .ok_or_else(|| line_error(line, "`tab_group` arm requires `-> tab \"<label>\"`"))?;
    let variants = split_lzx_list(variants_raw);
    if variants.is_empty() {
        return Err(line_error(
            line,
            "`tab_group case` requires at least one variant",
        ));
    }
    let tab_clause = tab_raw
        .trim()
        .strip_prefix("tab ")
        .ok_or_else(|| line_error(line, "`tab_group` arm right side must be `tab \"<label>\"`"))?;
    let label = unquote_lzx_value(tab_clause.trim()).to_owned();
    if label.is_empty() {
        return Err(line_error(
            line,
            "`tab_group` tab label must be a quoted string",
        ));
    }
    Ok(TabGroupCaseAst {
        variants,
        label,
        span: Span::new(line.start, line.end),
    })
}

// ===========================================================================
// Audience-level containers (tabs / wizard)
// ===========================================================================

/// `tabs` block — child lines are `tab "<label>" -> view <name> [audience <a>]`.
/// GAP-UX-03. Returns the parsed AST + the first unconsumed line index.
pub(super) fn parse_tabs_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(TabsAst, usize), ParseError> {
    let header = &lines[start];
    let body_indent = parent_indent + 2;
    let mut entries: Vec<TabEntryAst> = Vec::new();
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
                "`tabs` entries use one indentation level deeper than the `tabs` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim();
        entries.push(parse_tab_entry(line, trimmed)?);
        last_end = line.end;
        i += 1;
    }

    if entries.is_empty() {
        return Err(line_error(
            header,
            "`tabs` requires at least one `tab` entry",
        ));
    }
    Ok((
        TabsAst {
            entries,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse one `tab "<label>" -> view <name> [audience <a>]` row.
fn parse_tab_entry(line: &SourceLine<'_>, value: &str) -> Result<TabEntryAst, ParseError> {
    let rest = value
        .strip_prefix("tab ")
        .ok_or_else(|| line_error(line, "`tabs` rows are `tab \"<label>\" -> view <name>`"))?;
    let (label_raw, target_raw) = split_lzx_arrow(rest)
        .ok_or_else(|| line_error(line, "`tab` row requires `-> view <name>`"))?;
    let label = unquote_lzx_value(label_raw.trim()).to_owned();
    if label.is_empty() {
        return Err(line_error(line, "`tab` label must be a quoted string"));
    }
    let target = target_raw.trim();
    let view_clause = target.strip_prefix("view ").ok_or_else(|| {
        line_error(
            line,
            "`tab` right side must be `view <name> [audience <a>]`",
        )
    })?;
    let mut parts = view_clause.split_whitespace();
    let view = parts
        .next()
        .ok_or_else(|| line_error(line, "`tab -> view` requires a view name"))?
        .to_owned();
    if !is_kebab_or_snake_ident(&view) {
        return Err(line_error_owned(
            line,
            format!("`tab` view `{}` must be a kebab/snake identifier", view),
        ));
    }
    let mut audience = None;
    if let Some(keyword) = parts.next() {
        if keyword != "audience" {
            return Err(line_error(
                line,
                "`tab -> view <name>` trailing clause must be `audience <a>`",
            ));
        }
        let aud = parts
            .next()
            .ok_or_else(|| line_error(line, "`tab ... audience` requires an audience name"))?
            .to_owned();
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "`tab` row has trailing tokens after `audience <a>`",
            ));
        }
        if !is_kebab_or_snake_ident(&aud) {
            return Err(line_error_owned(
                line,
                format!("`tab` audience `{}` must be a kebab/snake identifier", aud),
            ));
        }
        audience = Some(aud);
    }
    Ok(TabEntryAst {
        label,
        view,
        audience,
        span: Span::new(line.start, line.end),
    })
}

/// `wizard <name> steps` block — child lines are `step <N>: <ref>`.
/// GAP-UX-03. Returns the parsed AST + the first unconsumed line index.
pub(super) fn parse_wizard_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
    header_rest: &str,
) -> Result<(WizardAst, usize), ParseError> {
    let header = &lines[start];
    let name = header_rest
        .strip_suffix(" steps")
        .map(str::trim)
        .ok_or_else(|| line_error(header, "`wizard` header is `wizard <name> steps`"))?;
    if !is_kebab_or_snake_ident(name) {
        return Err(line_error_owned(
            header,
            format!("`wizard` name `{}` must be a kebab/snake identifier", name),
        ));
    }
    let body_indent = parent_indent + 2;
    let mut steps: Vec<WizardStepAst> = Vec::new();
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
                "`wizard` steps use one indentation level deeper than the `wizard` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim();
        steps.push(parse_wizard_step(line, trimmed)?);
        last_end = line.end;
        i += 1;
    }

    if steps.is_empty() {
        return Err(line_error(header, "`wizard` requires at least one `step`"));
    }
    Ok((
        WizardAst {
            name: name.to_owned(),
            steps,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse one `step <N>: <ref>` row.
fn parse_wizard_step(line: &SourceLine<'_>, value: &str) -> Result<WizardStepAst, ParseError> {
    let rest = value
        .strip_prefix("step ")
        .ok_or_else(|| line_error(line, "`wizard` rows are `step <N>: <ref>`"))?;
    let (index_raw, ref_raw) = rest
        .split_once(':')
        .ok_or_else(|| line_error(line, "`wizard` step must be `step <N>: <ref>`"))?;
    let index: u32 = index_raw
        .trim()
        .parse()
        .map_err(|_| line_error(line, "`wizard` step index must be a positive integer"))?;
    if index == 0 {
        return Err(line_error(
            line,
            "`wizard` step index must be a positive integer",
        ));
    }
    let ref_name = ref_raw.trim().to_owned();
    if !is_kebab_or_snake_ident(&ref_name) {
        return Err(line_error_owned(
            line,
            format!(
                "`wizard` step ref `{}` must be a kebab/snake identifier",
                ref_name
            ),
        ));
    }
    Ok(WizardStepAst {
        index,
        ref_name,
        span: Span::new(line.start, line.end),
    })
}
