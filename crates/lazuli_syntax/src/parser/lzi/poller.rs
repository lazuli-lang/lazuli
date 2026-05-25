//! Feature-level `poller <name>` block parser.
//!
//! A poller is the canonical contract for "drive a cursor row through
//! a closed-state lifecycle by ticking external work". Closed catalog,
//! no escape hatch — every child below is enforced here so the AST
//! the analyzer sees is fully typed.
//!
//! ```text
//! poller <name>
//!   source <Resource>
//!   cursor
//!     eligible_when <next_at>, <resolved_at>
//!     attempts <attempts_field>
//!   retry
//!     max_attempts <int>
//!     backoff <fixed|linear|exponential> [base <duration>] [cap <duration>]
//!   states
//!     <state>            # one of: initial | intermediate | terminal
//!     <state> terminal
//!   resolve via @fn.<name>
//!   terminal_status_field <field>
//!   terminal_result_field <field>
//!   tick every <duration> [batch <int>]
//!   tenant_from row.<axis>_id
//!   idempotency by row.<field>[, row.<field>]*
//!   audit <subject>
//!   emits <event>
//!   retry_quirk <kind>
//!     when <predicate>
//!     counter <field>
//!     mutate row.<field> = <transform>
//! ```
//!
//! Every child appears at most once except `states` (>= 2 entries),
//! `emits`, and `retry_quirk` (zero-or-many catalog entries).
//!
//! Visibility: only `parse_poller_block` leaves the file as
//! `pub(super)`; the feature-skeleton walker dispatches to it from
//! `mod.rs`. Every other helper stays private — there are no
//! cross-cluster consumers.
//!
//! Source-of-truth: `docs/proposals/poller-vocab.md` §3 + the canonical
//! `full-capsule` fixture.

use super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::error::ParseError;
use super::types::{
    PollerBlockAst, PollerCursorAst, PollerRetryAst, PollerRetryQuirkAst, PollerStateAst,
    PollerTickAst,
};

use crate::ast::Span;

pub(super) fn parse_poller_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(PollerBlockAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let child_indent = block_indent + 2;
    let grandchild_indent = block_indent + 4;
    let name = header
        .text
        .trim_start()
        .strip_prefix("poller ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "poller header must be `poller <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "poller header requires a name"));
    }

    let mut source: Option<String> = None;
    let mut cursor: Option<PollerCursorAst> = None;
    let mut retry: Option<PollerRetryAst> = None;
    let mut states: Vec<PollerStateAst> = Vec::new();
    let mut resolve_handler: Option<String> = None;
    let mut terminal_status_field: Option<String> = None;
    let mut terminal_result_field: Option<String> = None;
    let mut tick: Option<PollerTickAst> = None;
    let mut tenant_from: Option<String> = None;
    let mut idempotency: Vec<String> = Vec::new();
    let mut idempotency_seen = false;
    let mut audit: Option<String> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut retry_quirks: Vec<PollerRetryQuirkAst> = Vec::new();

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
                "poller children use one indentation level deeper than the `poller` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            if source.is_some() {
                return Err(line_error(line, "poller declares `source` at most once"));
            }
            let val = rest.trim();
            if val.is_empty() {
                return Err(line_error(
                    line,
                    "`source` requires a resource name (`source <Resource>`)",
                ));
            }
            source = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "cursor" {
            if cursor.is_some() {
                return Err(line_error(line, "poller declares `cursor` at most once"));
            }
            let (block, next) = parse_poller_cursor(lines, i, child_indent)?;
            cursor = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "retry" {
            if retry.is_some() {
                return Err(line_error(line, "poller declares `retry` at most once"));
            }
            let (block, next) = parse_poller_retry(lines, i, child_indent)?;
            retry = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "states" {
            if !states.is_empty() {
                return Err(line_error(line, "poller declares `states` at most once"));
            }
            let (block, next) = parse_poller_states(lines, i, child_indent)?;
            states = block;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("resolve via ") {
            if resolve_handler.is_some() {
                return Err(line_error(line, "poller declares `resolve` at most once"));
            }
            let val = rest.trim();
            let handler = val.strip_prefix("@fn.").ok_or_else(|| {
                line_error(
                    line,
                    "`resolve via` requires `@fn.<name>` handler reference",
                )
            })?;
            if handler.is_empty() {
                return Err(line_error(
                    line,
                    "`resolve via @fn.<name>` requires a handler name",
                ));
            }
            resolve_handler = Some(handler.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("terminal_status_field ") {
            if terminal_status_field.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `terminal_status_field` at most once",
                ));
            }
            terminal_status_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("terminal_result_field ") {
            if terminal_result_field.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `terminal_result_field` at most once",
                ));
            }
            terminal_result_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("tick ") {
            if tick.is_some() {
                return Err(line_error(line, "poller declares `tick` at most once"));
            }
            tick = Some(parse_poller_tick(line, rest)?);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            if tenant_from.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `tenant_from` at most once",
                ));
            }
            let val = rest.trim();
            if !val.starts_with("row.") {
                return Err(line_error(
                    line,
                    "`tenant_from` requires `row.<axis>_id` (poller cursor row is the producer)",
                ));
            }
            tenant_from = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            if idempotency_seen {
                return Err(line_error(
                    line,
                    "poller declares `idempotency` at most once",
                ));
            }
            idempotency_seen = true;
            for entry in rest.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                if !entry.starts_with("row.") {
                    return Err(line_error(
                        line,
                        "`idempotency by` entries are `row.<field>` paths",
                    ));
                }
                idempotency.push(entry.to_owned());
            }
            if idempotency.is_empty() {
                return Err(line_error(
                    line,
                    "`idempotency by` requires at least one `row.<field>`",
                ));
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("audit ") {
            if audit.is_some() {
                return Err(line_error(line, "poller declares `audit` at most once"));
            }
            audit = Some(format!("audit {}", rest.trim()));
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("emits ") {
            let event = rest.trim();
            if event.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("retry_quirk ") {
            let (block, next) = parse_poller_retry_quirk(lines, i, rest, grandchild_indent)?;
            retry_quirks.push(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        return Err(line_error(
            line,
            "poller children are `source`, `cursor`, `retry`, `states`, `resolve via @fn.<name>`, \
             `terminal_status_field`, `terminal_result_field`, `tick every <duration>`, \
             `tenant_from row.<axis>_id`, `idempotency by row.<field>, ...`, `audit`, `emits`, `retry_quirk`",
        ));
    }

    let source =
        source.ok_or_else(|| line_error(header, "poller requires a `source <Resource>` child"))?;

    Ok((
        PollerBlockAst {
            name,
            source,
            cursor,
            retry,
            states,
            resolve_handler,
            terminal_status_field,
            terminal_result_field,
            tick,
            tenant_from,
            idempotency,
            audit,
            emits,
            retry_quirks,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_poller_cursor(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(PollerCursorAst, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut next_at_field: Option<String> = None;
    let mut resolved_at_field: Option<String> = None;
    let mut attempts_field: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`cursor` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("eligible_when ") {
            if next_at_field.is_some() {
                return Err(line_error(
                    line,
                    "`cursor` declares `eligible_when` at most once",
                ));
            }
            let mut parts = rest.split(',').map(str::trim);
            let na = parts.next().unwrap_or("");
            let ra = parts.next().unwrap_or("");
            if na.is_empty() || ra.is_empty() || parts.next().is_some() {
                return Err(line_error(
                    line,
                    "`eligible_when` requires two field names: \
                     `eligible_when <next_at_field>, <resolved_at_field>`",
                ));
            }
            next_at_field = Some(na.to_owned());
            resolved_at_field = Some(ra.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("attempts ") {
            if attempts_field.is_some() {
                return Err(line_error(
                    line,
                    "`cursor` declares `attempts` at most once",
                ));
            }
            let val = rest.trim();
            if val.is_empty() {
                return Err(line_error(line, "`attempts` requires a field name"));
            }
            attempts_field = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`cursor` body accepts only `eligible_when <a>, <b>` and `attempts <field>`",
        ));
    }

    let next_at_field = next_at_field
        .ok_or_else(|| line_error(header, "`cursor` requires an `eligible_when` child"))?;
    let resolved_at_field = resolved_at_field.expect("resolved_at parsed alongside next_at");
    let attempts_field = attempts_field
        .ok_or_else(|| line_error(header, "`cursor` requires an `attempts <field>` child"))?;

    Ok((
        PollerCursorAst {
            next_at_field,
            resolved_at_field,
            attempts_field,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_poller_retry(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(PollerRetryAst, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut max_attempts: Option<u32> = None;
    let mut backoff: Option<(String, Option<String>, Option<String>)> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`retry` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("max_attempts ") {
            if max_attempts.is_some() {
                return Err(line_error(
                    line,
                    "`retry` declares `max_attempts` at most once",
                ));
            }
            let val = rest.trim();
            let parsed = val
                .parse::<u32>()
                .map_err(|_| line_error(line, "`max_attempts` requires a non-negative integer"))?;
            max_attempts = Some(parsed);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("backoff ") {
            if backoff.is_some() {
                return Err(line_error(line, "`retry` declares `backoff` at most once"));
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                return Err(line_error(
                    line,
                    "`backoff` requires a strategy (fixed | linear | exponential)",
                ));
            }
            let strategy = parts[0].to_owned();
            if !matches!(strategy.as_str(), "fixed" | "linear" | "exponential") {
                return Err(line_error(
                    line,
                    "`backoff` strategy must be `fixed`, `linear`, or `exponential`",
                ));
            }
            let mut base: Option<String> = None;
            let mut cap: Option<String> = None;
            let mut j = 1;
            while j < parts.len() {
                match parts[j] {
                    "base" => {
                        j += 1;
                        if j >= parts.len() {
                            return Err(line_error(line, "`base` requires a duration value"));
                        }
                        base = Some(parts[j].to_owned());
                        j += 1;
                    }
                    "cap" => {
                        j += 1;
                        if j >= parts.len() {
                            return Err(line_error(line, "`cap` requires a duration value"));
                        }
                        cap = Some(parts[j].to_owned());
                        j += 1;
                    }
                    other => {
                        return Err(line_error_owned(
                            line,
                            format!(
                                "`backoff` modifiers are `base <duration>` and `cap <duration>` \
                                 (got `{other}`)"
                            ),
                        ));
                    }
                }
            }
            backoff = Some((strategy, base, cap));
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`retry` body accepts only `max_attempts <int>` and `backoff <strategy> [base <d>] [cap <d>]`",
        ));
    }

    let (backoff_strategy, backoff_base, backoff_cap) = backoff
        .ok_or_else(|| line_error(header, "`retry` requires a `backoff <strategy>` child"))?;
    let max_attempts = max_attempts
        .ok_or_else(|| line_error(header, "`retry` requires a `max_attempts <int>` child"))?;

    Ok((
        PollerRetryAst {
            max_attempts,
            backoff_strategy,
            backoff_base,
            backoff_cap,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_poller_states(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(Vec<PollerStateAst>, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut states: Vec<PollerStateAst> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`states` body uses one indentation level deeper than the header",
            ));
        }

        let mut parts = trimmed.split_whitespace();
        let name = parts
            .next()
            .ok_or_else(|| line_error(line, "state entry requires a name"))?
            .to_owned();
        let kind_keyword = match parts.next() {
            None => None,
            Some(k @ ("initial" | "intermediate" | "terminal")) => Some(k.to_owned()),
            Some(other) => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "state kind must be `initial`, `intermediate`, or `terminal` (got `{other}`)"
                    ),
                ));
            }
        };
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "state entry accepts at most one kind modifier (initial | intermediate | terminal)",
            ));
        }
        states.push(PollerStateAst {
            name,
            kind_keyword,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    if states.len() < 2 {
        return Err(line_error(
            header,
            "poller `states` requires at least 2 entries",
        ));
    }

    Ok((states, i))
}

fn parse_poller_tick(line: &SourceLine<'_>, rest: &str) -> Result<PollerTickAst, ParseError> {
    let rest = rest.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "every" {
        return Err(line_error(
            line,
            "`tick` requires `tick every <duration> [batch <int>]`",
        ));
    }
    let every = parts[1].to_owned();
    let mut batch: Option<u32> = None;
    if parts.len() > 2 {
        if parts.len() != 4 || parts[2] != "batch" {
            return Err(line_error(
                line,
                "`tick` modifier is `batch <int>` after `every <duration>`",
            ));
        }
        let parsed = parts[3]
            .parse::<u32>()
            .map_err(|_| line_error(line, "`batch` requires a non-negative integer"))?;
        batch = Some(parsed);
    }
    Ok(PollerTickAst {
        every,
        batch,
        span: Span::new(line.start, line.end),
    })
}

fn parse_poller_retry_quirk(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
    grandchild_indent: usize,
) -> Result<(PollerRetryQuirkAst, usize), ParseError> {
    let header = &lines[start];
    let kind = head_rest.trim().to_owned();
    if kind.is_empty() {
        return Err(line_error(
            header,
            "`retry_quirk` requires a catalog kind (e.g. `retry_quirk gender_flip_once`)",
        ));
    }
    let mut when: Option<String> = None;
    let mut counter_field: Option<String> = None;
    let mut mutate_field: Option<String> = None;
    let mut mutate_transform: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header.indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`retry_quirk` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("when ") {
            if when.is_some() {
                return Err(line_error(
                    line,
                    "`retry_quirk` declares `when` at most once",
                ));
            }
            when = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("counter ") {
            if counter_field.is_some() {
                return Err(line_error(
                    line,
                    "`retry_quirk` declares `counter` at most once",
                ));
            }
            counter_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("mutate ") {
            if mutate_field.is_some() {
                return Err(line_error(
                    line,
                    "`retry_quirk` declares `mutate` at most once",
                ));
            }
            let rest = rest.trim();
            let (lhs, rhs) = rest
                .split_once('=')
                .ok_or_else(|| line_error(line, "`mutate` requires `<field> = <transform>`"))?;
            let lhs = lhs.trim();
            let rhs = rhs.trim();
            let field = lhs.strip_prefix("row.").unwrap_or(lhs);
            if field.is_empty() {
                return Err(line_error(line, "`mutate` requires a field on `row.*`"));
            }
            mutate_field = Some(field.to_owned());
            mutate_transform = Some(rhs.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`retry_quirk` body accepts only `when <predicate>`, `counter <field>`, and `mutate <field> = <transform>`",
        ));
    }

    let when = when
        .ok_or_else(|| line_error(header, "`retry_quirk` requires a `when <predicate>` child"))?;
    let counter_field = counter_field
        .ok_or_else(|| line_error(header, "`retry_quirk` requires a `counter <field>` child"))?;
    let mutate_field = mutate_field.ok_or_else(|| {
        line_error(
            header,
            "`retry_quirk` requires a `mutate <field> = <transform>` child",
        )
    })?;
    let mutate_transform = mutate_transform.expect("transform parsed alongside field");

    Ok((
        PollerRetryQuirkAst {
            kind,
            when,
            counter_field,
            mutate_field,
            mutate_transform,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
