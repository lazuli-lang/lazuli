//! `audit <subjects>[, before][, after][, retain <duration>]` parsing
//! and its grandchild block (`emit_to`, `data_subject`, `before`,
//! `after`, `retain`).
//!
//! `parse_command_audit` is `pub(in crate::parser::lzi)` because
//! `report.rs` reuses the same audit envelope grammar for its report
//! blocks.

use super::super::super::common::{SourceLine, is_kebab_or_snake_ident, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};

use crate::ast::{CommandAudit, Span};

pub(in crate::parser::lzi) fn parse_command_audit(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(CommandAudit, usize), ParseError> {
    let header = &lines[start];
    let mut subjects: Vec<String> = Vec::new();
    let mut record_before = false;
    let mut record_after = false;
    let mut retain_for: Option<String> = None;
    for part in rest.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if part == "before" {
            record_before = true;
        } else if part == "after" {
            record_after = true;
        } else if let Some(duration) = part.strip_prefix("retain ") {
            let duration = duration.trim();
            if duration.is_empty() {
                return Err(line_error(header, "`audit retain` requires a duration"));
            }
            retain_for = Some(duration.to_owned());
        } else {
            subjects.push(part.to_owned());
        }
    }
    if subjects.is_empty() && !record_before && !record_after && retain_for.is_none() {
        return Err(line_error(
            header,
            "`audit` requires at least one subject (e.g. `audit actor, target.id`)",
        ));
    }
    let mut emit_to: Option<String> = None;
    let mut data_subject: Option<String> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`audit` children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("emit_to ") {
            if emit_to.is_some() {
                return Err(line_error(
                    line,
                    "`audit emit_to` may be declared at most once",
                ));
            }
            emit_to = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("data_subject ") {
            let subject_field = rest.trim();
            if subject_field.is_empty() || !is_kebab_or_snake_ident(subject_field) {
                return Err(line_error(
                    line,
                    "`audit data_subject` requires a field identifier",
                ));
            }
            if data_subject.is_some() {
                return Err(line_error(
                    line,
                    "`audit data_subject` may be declared at most once",
                ));
            }
            data_subject = Some(subject_field.to_owned());
            i += 1;
        } else if trimmed == "before" {
            record_before = true;
            i += 1;
        } else if trimmed == "after" {
            record_after = true;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retain ") {
            if retain_for.is_some() {
                return Err(line_error(
                    line,
                    "`audit retain` may be declared at most once",
                ));
            }
            let duration = rest.trim();
            if duration.is_empty() {
                return Err(line_error(line, "`audit retain` requires a duration"));
            }
            retain_for = Some(duration.to_owned());
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`audit` children are `emit_to <event_group>`, `data_subject <field>`, `before`, `after`, or `retain <duration>` only",
            ));
        }
    }
    Ok((
        CommandAudit {
            subjects,
            emit_to,
            data_subject,
            record_before,
            record_after,
            retain_for,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}
