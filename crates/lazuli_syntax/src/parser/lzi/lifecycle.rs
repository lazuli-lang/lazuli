//! Resource-scoped `lifecycle <field>` block parser.
//!
//! The lifecycle vocabulary (`docs/proposals/lifecycle-vocab.md` v0.3)
//! retired the feature-level `workflow` keyword in favour of a typed
//! block authored *inside* a resource — anchored on the resource field
//! that stores the discriminator state. Closed grammar:
//!
//! ```text
//! lifecycle status
//!   state draft initial
//!   state published
//!   state archived terminal
//!   transition publish
//!     from draft
//!     to published
//!     policy @policy.publish
//!     audit actor, target.id
//!     timestamps published_at
//!     emits Published
//!     requires title.present?
//!     tests
//!       "publish.lifecycle"
//!     previously activate
//!   invariant total_immutable
//!   invariant_handler "./handlers/lifecycle.go"
//! ```
//!
//! ## Closed-catalog rules enforced here
//!
//! - At least two `state` declarations.
//! - At least one `transition`.
//! - Each transition declares at most one `to`, `policy`, `audit`,
//!   `timestamps`, `requires`, `tests` clause; `from`, `emits`, and
//!   `previously` accept repeats.
//! - `transition` requires both `from` (≥1 state) and `to` (exactly
//!   one identifier).
//!
//! Visibility: `parse_lifecycle_block` is `pub(super)` — the only
//! caller is the resource parser in `mod.rs` (`handle_resource_lifecycle`).

use super::super::common::{SourceLine, is_trivia, line_error, line_error_owned, split_lzx_list};
use super::super::error::ParseError;
use super::types::{
    LifecycleBlockAst, LifecycleInvariantAst, LifecycleStateAst, LifecycleTransitionAst,
};

use crate::ast::Span;

pub(super) fn parse_lifecycle_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LifecycleBlockAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let rest = header
        .text
        .trim_start()
        .strip_prefix("lifecycle ")
        .unwrap_or("");
    let discriminator_field = rest.trim().to_owned();
    let child_indent = block_indent + 2;
    let grandchild_indent = block_indent + 4;

    let mut states = Vec::new();
    let mut transitions = Vec::new();
    let mut invariants = Vec::new();
    let mut invariant_handlers = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= block_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "lifecycle children use one indentation level deeper than the `lifecycle` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("state ") {
            let ((name, kind_keyword), span) = parse_lifecycle_state(line, rest)?;
            states.push(LifecycleStateAst {
                name,
                kind_keyword,
                span,
            });
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("transition ") {
            let transition_name = rest.trim();
            if transition_name.is_empty() {
                return Err(line_error(line, "`transition` requires a name"));
            }
            let (transition, next) =
                parse_lifecycle_transition(lines, i, transition_name, grandchild_indent)?;
            transitions.push(transition);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("invariant_handler ") {
            invariant_handlers.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("invariant ") {
            invariants.push(LifecycleInvariantAst {
                raw: rest.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "lifecycle children are `state`, `transition`, `invariant`, `invariant_handler`",
            ));
        }
    }

    if states.len() < 2 {
        return Err(line_error(
            header,
            "lifecycle requires at least 2 `state` declarations",
        ));
    }
    if transitions.is_empty() {
        return Err(line_error(
            header,
            "lifecycle requires at least 1 `transition`",
        ));
    }

    Ok((
        LifecycleBlockAst {
            discriminator_field,
            states,
            transitions,
            invariants,
            invariant_handlers,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_lifecycle_state(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<((String, Option<String>), Span), ParseError> {
    let mut parts = rest.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| line_error(line, "`state` requires a name"))?
        .to_owned();
    let kind_keyword = match parts.next() {
        None => None,
        Some(k @ ("initial" | "terminal")) => Some(k.to_owned()),
        Some(other) => {
            return Err(line_error_owned(
                line,
                format!("`state` modifier must be `initial` or `terminal`, got `{other}`"),
            ));
        }
    };
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`state` accepts at most one modifier (initial | terminal)",
        ));
    }
    Ok(((name, kind_keyword), Span::new(line.start, line.end)))
}

fn parse_lifecycle_transition(
    lines: &[SourceLine<'_>],
    start: usize,
    name: &str,
    child_indent: usize,
) -> Result<(LifecycleTransitionAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let tests_indent = child_indent + 2;
    let mut from = Vec::new();
    let mut to = None;
    let mut policy = None;
    let mut audit = None;
    let mut timestamps = None;
    let mut emits = Vec::new();
    let mut requires = None;
    let mut tests = Vec::new();
    let mut tests_seen = false;
    let mut previously = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= block_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "transition children use one indentation level deeper than the `transition` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("from ") {
            let states = split_lzx_list(rest);
            if states.is_empty() {
                return Err(line_error(line, "`from` requires at least one state"));
            }
            from.extend(states);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("to ") {
            ensure_lifecycle_once(line, "to", to.is_some())?;
            let target = parse_lifecycle_single_identifier(line, "`to`", rest)?;
            to = Some(target);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            ensure_lifecycle_once(line, "policy", policy.is_some())?;
            policy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            ensure_lifecycle_once(line, "audit", audit.is_some())?;
            audit = Some(format!("audit {}", rest.trim()));
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timestamps ") {
            ensure_lifecycle_once(line, "timestamps", timestamps.is_some())?;
            let field = parse_lifecycle_single_identifier(line, "`timestamps`", rest)?;
            timestamps = Some(field);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            let event = rest.trim();
            if event.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("requires ") {
            ensure_lifecycle_once(line, "requires", requires.is_some())?;
            requires = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "tests" {
            ensure_lifecycle_once(line, "tests", tests_seen)?;
            tests_seen = true;
            last_end = line.end;
            i += 1;
            while i < lines.len() {
                let test_line = &lines[i];
                let test_trimmed = test_line.text.trim_start();
                if is_trivia(test_trimmed) {
                    i += 1;
                    continue;
                }
                if test_line.indent <= child_indent {
                    break;
                }
                if test_line.indent != tests_indent {
                    return Err(line_error(
                        test_line,
                        "`tests` children use one indentation level deeper than the `tests` header",
                    ));
                }
                tests.push(test_trimmed.to_owned());
                last_end = test_line.end;
                i += 1;
            }
        } else if let Some(rest) = trimmed.strip_prefix("previously ") {
            previously.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "transition children are `from`, `to`, `policy`, `audit`, `timestamps`, `emits`, `requires`, `tests`, or `previously`",
            ));
        }
    }

    if from.is_empty() {
        return Err(line_error(
            header,
            "`transition` requires at least one `from`",
        ));
    }
    let to = to.ok_or_else(|| line_error(header, "`transition` requires `to <state>`"))?;

    Ok((
        LifecycleTransitionAst {
            name: name.to_owned(),
            from,
            to,
            policy,
            audit,
            timestamps,
            emits,
            requires,
            tests,
            previously,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn ensure_lifecycle_once(
    line: &SourceLine<'_>,
    keyword: &'static str,
    already_seen: bool,
) -> Result<(), ParseError> {
    if already_seen {
        return Err(line_error_owned(
            line,
            format!("transition declares `{keyword}` at most once"),
        ));
    }
    Ok(())
}

fn parse_lifecycle_single_identifier(
    line: &SourceLine<'_>,
    keyword: &'static str,
    rest: &str,
) -> Result<String, ParseError> {
    let mut parts = rest.split_whitespace();
    let value = parts.next().ok_or_else(|| {
        line_error_owned(line, format!("{keyword} requires exactly one identifier"))
    })?;
    if parts.next().is_some() {
        return Err(line_error_owned(
            line,
            format!("{keyword} requires exactly one identifier"),
        ));
    }
    Ok(value.to_owned())
}
