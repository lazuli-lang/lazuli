//! `experience` resume router — declarative restart logic.
//!
//! A `resume <name>` block describes how an in-flight workflow resumes
//! after a session boundary. It reads a single source query and then
//! pattern-matches lifecycle states to target views:
//!
//! ```text
//! resume onboarding
//!   source query.lookup onboarding.state
//!   none -> view start
//!   in_progress substep "kyc" -> view kyc_form
//!   in_progress -> view review
//!   completed -> view done
//!   * -> view fallback
//! ```
//!
//! Each arm is `<state> -> view <name>` (or the Unicode `→`). `<state>`
//! is a lifecycle state ident, `none` (no state recorded yet), or `*`
//! (catch-all). An optional `substep "<...>"` tail narrows a state
//! arm; it is **not** valid on `none` / `*`.
//!
//! `source query.lookup <query>` is mandatory and may appear at most
//! once; `<query>` is either `<feature>.<query>` or a bare
//! `<query>` (resolved against the enclosing experience's imports).

use crate::ast::{LzxResumeArm, LzxResumeArmKind, LzxResumeRouter, Span};

use super::super::super::common::{
    SourceLine, is_lzx_bare_ident, is_lzx_resume_ref, is_trivia, line_error, split_lzx_arrow,
};
use super::super::super::error::ParseError;
use super::super::app::parse_lzx_optional_substep_tail;

pub(super) fn parse_lzx_resume_router(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxResumeRouter, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "resume blocks use `resume <name>`"));
    }
    if !is_lzx_bare_ident(parts[1]) {
        return Err(line_error(
            header,
            "`resume` name must be a bare identifier",
        ));
    }

    let mut source_query = None;
    let mut arms = Vec::new();
    let mut index = start + 1;
    let mut last_end = header.end;

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
                "resume children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source query.lookup ") {
            if source_query.is_some() {
                return Err(line_error(
                    line,
                    "resume declares `source query.lookup` at most once",
                ));
            }
            let query = rest.trim();
            if !is_lzx_resume_ref(query) {
                return Err(line_error(
                    line,
                    "`source query.lookup` requires `<query>` or `<feature>.<query>`",
                ));
            }
            source_query = Some(query.to_owned());
        } else {
            arms.push(parse_lzx_resume_arm(line, trimmed)?);
        }

        last_end = line.end;
        index += 1;
    }

    let Some(source_query) = source_query else {
        return Err(line_error(
            header,
            "resume blocks require `source query.lookup <query>`",
        ));
    };
    if arms.is_empty() {
        return Err(line_error(header, "resume blocks require at least one arm"));
    }

    Ok((
        LzxResumeRouter {
            name: parts[1].to_owned(),
            source_query,
            arms,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

fn parse_lzx_resume_arm(line: &SourceLine<'_>, trimmed: &str) -> Result<LzxResumeArm, ParseError> {
    let Some((left, right)) = split_lzx_arrow(trimmed) else {
        return Err(line_error(
            line,
            "resume arms use `<state> -> view <name>` or `<state> → view <name>`",
        ));
    };
    let arm = left.trim();
    let (arm, substep) = parse_lzx_optional_substep_tail(line, arm, "resume arm")?;
    let kind = match arm {
        "none" => LzxResumeArmKind::None,
        "*" => LzxResumeArmKind::Wildcard,
        state if is_lzx_bare_ident(state) => LzxResumeArmKind::State(state.to_owned()),
        _ => {
            return Err(line_error(
                line,
                "resume arm state must be a lifecycle state, `none`, or `*`",
            ));
        }
    };
    if substep.is_some() && matches!(kind, LzxResumeArmKind::None | LzxResumeArmKind::Wildcard) {
        return Err(line_error(
            line,
            "resume arm `substep` is only valid on lifecycle state arms",
        ));
    }

    let Some(target_view) = right.trim().strip_prefix("view ") else {
        return Err(line_error(
            line,
            "resume arms target views with `view <name>`",
        ));
    };
    let target_view = target_view.trim();
    if !is_lzx_bare_ident(target_view) {
        return Err(line_error(
            line,
            "resume arm target view must be a bare identifier",
        ));
    }

    Ok(LzxResumeArm {
        kind,
        substep,
        target_view: target_view.to_owned(),
        span: Span::new(line.start, line.end),
    })
}
