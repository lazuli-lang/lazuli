//! `poller.retry` sub-block — `max_attempts <int>` + `backoff <strategy>
//! [base <duration>] [cap <duration>]`. Plus the catalog
//! `retry_quirk <kind>` block.
//!
//! Extracted from the original monolithic `poller.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::super::error::ParseError;
use super::super::types::{PollerRetryAst, PollerRetryQuirkAst};
use crate::ast::Span;

pub(super) fn parse_poller_retry(
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

pub(super) fn parse_poller_retry_quirk(
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
    // `mutate` populates both fields together above, so if mutate_field
    // is Some, mutate_transform is also Some. Defensive fallback uses the
    // same mutate-shape error.
    let mutate_transform = mutate_transform.ok_or_else(|| {
        line_error(
            header,
            "`retry_quirk` requires a `mutate <field> = <transform>` child",
        )
    })?;

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
