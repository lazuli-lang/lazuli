//! Command-lifecycle child grammars: `approval`, `triggers transition`,
//! `write_window`, `deprecated`, `tests`. These are the modifier-shape
//! children that gate / temporalise / supersede a command without
//! changing what the command *does*.
//!
//! `parse_command_deprecated` and `parse_deprecated_block` are
//! `pub(in crate::parser::lzi)` because `api.rs` reuses them — `api`
//! blocks accept the same deprecation envelope.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned, unquote_lzx_value};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};

use crate::ast::{
    ApprovalThenDecl, CommandApproval, CommandDeprecatedDecl, CommandWriteWindow, Span,
};

pub(super) fn parse_command_triggers(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<String>, ParseError> {
    let rest = rest.trim();
    let names = if rest == "transition" {
        ""
    } else if let Some(names) = rest.strip_prefix("transition ") {
        names.trim()
    } else {
        // Legacy pilot files used `triggers <transition>` before the surface
        // grew the explicit `transition` discriminator. Keep accepting it,
        // but normalize to the same `CommandDecl.triggers` vector.
        rest
    };
    if names.is_empty() {
        return Err(line_error(
            line,
            "`triggers transition` requires at least one transition name",
        ));
    }

    parse_command_trigger_names(line, names)
}

fn parse_command_trigger_names(
    line: &SourceLine<'_>,
    names: &str,
) -> Result<Vec<String>, ParseError> {
    let mut triggers = Vec::new();
    for name in names.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`triggers transition` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        if name.chars().any(char::is_whitespace) {
            return Err(line_error(
                line,
                "transition names in `triggers transition` cannot contain whitespace; separate with commas",
            ));
        }
        triggers.push(name.to_owned());
    }
    Ok(triggers)
}

pub(super) fn parse_command_triggers_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let mut triggers = Vec::new();
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
                "`triggers` children use six-space indentation",
            ));
        }

        let Some(rest) = trimmed.strip_prefix("transition ") else {
            return Err(line_error(
                line,
                "`triggers` children use `transition <name>[, <name>]`",
            ));
        };

        let parsed = parse_command_trigger_names(line, rest.trim())?;
        triggers.extend(parsed);
        i += 1;
    }

    if triggers.is_empty() {
        return Err(line_error(
            &lines[start],
            "`triggers` requires at least one `transition <name>` child",
        ));
    }

    Ok((triggers, i))
}

pub(super) fn parse_command_write_window(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandWriteWindow, ParseError> {
    let Some(rest) = rest.trim().strip_prefix("by ") else {
        return Err(line_error(
            line,
            "`write_window` must be `write_window by <path> within <duration_or_ref>`",
        ));
    };
    let Some((by, within)) = rest.trim().split_once(" within ") else {
        return Err(line_error(
            line,
            "`write_window by <path>` requires `within <duration_or_ref>`",
        ));
    };
    let by = by.trim();
    let within = within.trim();
    if by.is_empty() {
        return Err(line_error(line, "`write_window by` requires a path"));
    }
    if within.is_empty() {
        return Err(line_error(
            line,
            "`write_window within` requires a duration or reference",
        ));
    }
    Ok(CommandWriteWindow {
        by: by.to_owned(),
        within: within.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

pub(super) fn parse_command_approval(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandApproval, usize), ParseError> {
    let header = &lines[start];
    let mut required_when: Option<String> = None;
    let mut by: Option<String> = None;
    // W4 GAP-06 — `chain [@role.a, @role.b] [sequential]` child form.
    let mut chain: Vec<String> = Vec::new();
    let mut sequential = false;
    let mut timeout: Option<String> = None;
    let mut then: Option<ApprovalThenDecl> = None;
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
                "`approval` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("required_when ") {
            required_when = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("chain ") {
            // W4 GAP-06 — `chain [@role.a, @role.b] [sequential]`. The
            // approvers are a bracketed comma list; an optional trailing
            // `sequential` token enforces chain order.
            if !chain.is_empty() {
                return Err(line_error(
                    line,
                    "`approval` may declare at most one `chain`",
                ));
            }
            let (parsed_chain, seq) = parse_approval_chain(line, rest.trim())?;
            chain = parsed_chain;
            sequential = sequential || seq;
            i += 1;
        } else if trimmed == "sequential" {
            sequential = true;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("by ") {
            by = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("then ") {
            then = Some(match rest.trim() {
                "deny" => ApprovalThenDecl::Deny,
                "allow" => ApprovalThenDecl::Allow,
                "escalate" => ApprovalThenDecl::Escalate,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!(
                            "`approval then` requires `deny`, `allow`, or `escalate` (got `{other}`)"
                        ),
                    ));
                }
            });
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`approval` children are `required_when`, `by`, `chain`, `sequential`, `timeout`, or `then`",
            ));
        }
    }
    // W4 GAP-06 — reconcile the single-approver `by` and the `chain` forms.
    // Exactly one of them supplies the approvers. `by` lifts to a 1-element
    // chain; `chain` sets `by = chain[0]` for back-compat.
    let (by, chain) = match (by, chain.is_empty()) {
        (Some(b), true) => (b.clone(), vec![b]),
        (None, false) => (chain[0].clone(), chain),
        (Some(_), false) => {
            return Err(line_error(
                header,
                "`approval` accepts either `by <role>` or `chain [...]`, not both",
            ));
        }
        (None, true) => {
            return Err(line_error(
                header,
                "`approval` requires a `by @role.<name>` declaration or a `chain [@role.a, ...]`",
            ));
        }
    };
    let then = then.ok_or_else(|| {
        line_error(
            header,
            "`approval` requires a `then deny | allow | escalate` declaration",
        )
    })?;
    Ok((
        CommandApproval {
            required_when,
            by,
            chain,
            sequential,
            timeout,
            then,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// W4 GAP-06 — parse the body of an `approval chain` child:
/// `[@role.a, @role.b] [sequential]`. Returns the ordered approver atoms +
/// the `sequential` flag. The list must be bracketed and non-empty.
fn parse_approval_chain(
    line: &SourceLine<'_>,
    body: &str,
) -> Result<(Vec<String>, bool), ParseError> {
    let body = body.trim();
    let Some(close) = body.find(']') else {
        return Err(line_error(
            line,
            "`approval chain` requires a bracketed list `[@role.a, @role.b]`",
        ));
    };
    let list = body[..close]
        .strip_prefix('[')
        .ok_or_else(|| line_error(line, "`approval chain` list must start with `[`"))?;
    let approvers: Vec<String> = list
        .split(',')
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect();
    if approvers.is_empty() {
        return Err(line_error(
            line,
            "`approval chain [...]` requires at least one approver role",
        ));
    }
    let trailing = body[close + 1..].trim();
    let sequential = match trailing {
        "" => false,
        "sequential" => true,
        other => {
            return Err(line_error_owned(
                line,
                format!("`approval chain [...]` only accepts a trailing `sequential` (got `{other}`)"),
            ));
        }
    };
    Ok((approvers, sequential))
}

/// Parse `deprecated [since "<X>"] [replacement <ref>] [sunset "<Y>"]` —
/// inline single-line shape. Keys may appear in any order; each at most
/// once.
pub(in crate::parser::lzi) fn parse_command_deprecated(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandDeprecatedDecl, ParseError> {
    let mut since: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut sunset: Option<String> = None;
    let mut cursor = rest.trim();
    while !cursor.is_empty() {
        if let Some(after) = cursor.strip_prefix("since ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated since` requires a value"))?;
            since = Some(val);
            cursor = next.trim_start();
        } else if let Some(after) = cursor.strip_prefix("replacement ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated replacement` requires a value"))?;
            replacement = Some(val);
            cursor = next.trim_start();
        } else if let Some(after) = cursor.strip_prefix("sunset ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated sunset` requires a value"))?;
            sunset = Some(val);
            cursor = next.trim_start();
        } else {
            return Err(line_error(
                line,
                "`deprecated` children are `since`, `replacement`, `sunset`",
            ));
        }
    }
    Ok(CommandDeprecatedDecl {
        since,
        replacement,
        sunset,
        span: Span::new(line.start, line.end),
    })
}

pub(in crate::parser::lzi) fn parse_deprecated_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandDeprecatedDecl, usize), ParseError> {
    let header = &lines[start];
    let mut since: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut sunset: Option<String> = None;
    let mut last_end = header.end;
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
                "`deprecated` block children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("since ") {
            since = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("replacement ") {
            let (value, _) = take_quoted_or_word(rest)
                .ok_or_else(|| line_error(line, "`deprecated replacement` requires a value"))?;
            replacement = Some(value);
        } else if let Some(rest) = trimmed.strip_prefix("sunset ") {
            sunset = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`deprecated` children are `since`, `replacement`, `sunset`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        CommandDeprecatedDecl {
            since,
            replacement,
            sunset,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Take a quoted string or a single bare word (dotted refs allowed),
/// returning the unquoted value and the remainder of the input.
fn take_quoted_or_word(s: &str) -> Option<(String, &str)> {
    let trimmed = s.trim_start();
    if let Some(after_quote) = trimmed.strip_prefix('"') {
        let end = after_quote.find('"')?;
        let value = after_quote[..end].to_owned();
        let next = &after_quote[end + 1..];
        Some((value, next))
    } else {
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let value = trimmed[..end].to_owned();
        if value.is_empty() {
            return None;
        }
        let next = &trimmed[end..];
        Some((value, next))
    }
}

pub(super) fn parse_command_tests_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let mut out: Vec<String> = Vec::new();
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
        out.push(trimmed.to_owned());
        i += 1;
    }
    Ok((out, i))
}
