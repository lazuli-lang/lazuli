//! Per-keyword constraint parsers — `min`/`max`/`length`, `pattern`,
//! `between`, `in`, `validate`, `validator`. Each returns the parsed
//! value (or enum) plus the unconsumed tail so the orchestrator in
//! `field_constraints/mod.rs` can keep peeling.
//!
//! Extracted from the original monolithic `field_constraints.rs`.

use super::super::super::common::{line_error, line_error_owned, SourceLine};
use super::super::super::error::ParseError;
use super::super::split_top_level_commas;

pub(super) enum ParsedValidateConstraint {
    SanitizeHtml(String),
    Utf8Safe,
    MaxRecursion(u32),
    MaxSize(u64),
}

/// Parse `<keyword> <integer> [tail...]`. Returns the parsed integer
/// and the tail after the integer (which may carry further
/// constraints or be empty).
pub(super) fn parse_constraint_int(
    line: &SourceLine<'_>,
    text: &str,
    keyword: &str,
) -> Result<(i64, String), ParseError> {
    // text starts with `<keyword> ` already verified by caller.
    let rest = text.trim_start();
    let rest = rest
        .strip_prefix(keyword)
        .ok_or_else(|| {
            line_error_owned(line, format!("expected `{}` constraint keyword", keyword))
        })?
        .trim_start();
    // Take next whitespace-delimited token as the integer.
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let value_str = &rest[..end];
    let tail = rest[end..].to_owned();
    let n: i64 = value_str.parse().map_err(|_| {
        line_error_owned(
            line,
            format!(
                "`{}` constraint expects an integer, got `{}`",
                keyword, value_str
            ),
        )
    })?;
    Ok((n, tail))
}

/// Parse `pattern "<STRING>" [tail...]`. The string is delimited by
/// double quotes; embedded quotes are not supported (RE2 doesn't need
/// them in the common case — `\"` is rare).
pub(super) fn parse_constraint_string(
    line: &SourceLine<'_>,
    text: &str,
    keyword: &str,
) -> Result<(String, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix(keyword)
        .ok_or_else(|| line_error_owned(line, format!("expected `{}` keyword", keyword)))?
        .trim_start();
    if !rest.starts_with('"') {
        return Err(line_error_owned(
            line,
            format!(
                "`{}` constraint expects a quoted string (e.g. `pattern \"^[a-z]+$\"`)",
                keyword
            ),
        ));
    }
    let body = &rest[1..];
    let end = body.find('"').ok_or_else(|| {
        line_error_owned(
            line,
            format!("`{}` constraint string is missing a closing `\"`", keyword),
        )
    })?;
    let value = body[..end].to_owned();
    let tail = body[end + 1..].to_owned();
    Ok((value, tail))
}

/// Parse `between <A> and <B> [tail...]`.
pub(super) fn parse_constraint_between(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(i64, i64, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("between")
        .ok_or_else(|| line_error(line, "expected `between` keyword"))?
        .trim_start();
    // Parse first integer.
    let end = rest
        .find(|c: char| c.is_whitespace())
        .ok_or_else(|| line_error(line, "`between` constraint requires `<A> and <B>`"))?;
    let lo_str = &rest[..end];
    let lo: i64 = lo_str.parse().map_err(|_| {
        line_error_owned(line, format!("`between` expects integer, got `{}`", lo_str))
    })?;
    let rest = rest[end..].trim_start();
    let rest = rest
        .strip_prefix("and")
        .ok_or_else(|| line_error(line, "`between <A> and <B>` requires the `and` keyword"))?
        .trim_start();
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let hi_str = &rest[..end];
    let hi: i64 = hi_str.parse().map_err(|_| {
        line_error_owned(line, format!("`between` expects integer, got `{}`", hi_str))
    })?;
    let tail = rest[end..].to_owned();
    Ok((lo, hi, tail))
}

/// Parse `in [a, b, c] [tail...]`. Returns the list values and the
/// tail. Quoted-string items are unquoted; bare integers stay as
/// their text form (the analyzer interprets per field type).
pub(super) fn parse_constraint_in_list(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(Vec<String>, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("in")
        .ok_or_else(|| line_error(line, "expected `in` keyword"))?
        .trim_start();
    if !rest.starts_with('[') {
        return Err(line_error(
            line,
            "`in` constraint expects a bracketed list (e.g. `in [\"a\", \"b\"]`)",
        ));
    }
    // Find matching `]`.
    let body = &rest[1..];
    let close = body
        .find(']')
        .ok_or_else(|| line_error(line, "`in` constraint list is missing a closing `]`"))?;
    let inner = &body[..close];
    let tail = body[close + 1..].to_owned();
    let values: Vec<String> = split_top_level_commas(inner)
        .into_iter()
        .map(|piece| {
            let trimmed = piece.trim();
            // Strip surrounding double quotes if present.
            if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
                trimmed[1..trimmed.len() - 1].to_owned()
            } else {
                trimmed.to_owned()
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    Ok((values, tail))
}

pub(super) fn parse_constraint_validate(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(ParsedValidateConstraint, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("validate ")
        .ok_or_else(|| line_error(line, "expected `validate <constraint>`"))?
        .trim_start();
    if let Some(inside) = rest.strip_prefix("sanitize_html(") {
        let end = inside.find(')').ok_or_else(|| {
            line_error(
                line,
                "`validate sanitize_html` profile is missing a closing `)`",
            )
        })?;
        let profile = inside[..end].trim();
        if profile.is_empty() {
            return Err(line_error(
                line,
                "`validate sanitize_html` requires a profile",
            ));
        }
        let tail = inside[end + 1..].to_owned();
        return Ok((
            ParsedValidateConstraint::SanitizeHtml(profile.to_owned()),
            tail,
        ));
    }
    if let Some(tail) = rest.strip_prefix("utf8_safe") {
        return Ok((ParsedValidateConstraint::Utf8Safe, tail.to_owned()));
    }
    if let Some(after) = rest.strip_prefix("max_recursion:") {
        let (raw, tail) = take_constraint_value(after);
        let value = raw.parse::<u32>().map_err(|_| {
            line_error_owned(
                line,
                format!("`validate max_recursion` expects u32, got `{}`", raw),
            )
        })?;
        return Ok((ParsedValidateConstraint::MaxRecursion(value), tail));
    }
    if let Some(after) = rest.strip_prefix("max_size:") {
        let (raw, tail) = take_constraint_value(after);
        let value = raw.parse::<u64>().map_err(|_| {
            line_error_owned(
                line,
                format!("`validate max_size` expects bytes as u64, got `{}`", raw),
            )
        })?;
        return Ok((ParsedValidateConstraint::MaxSize(value), tail));
    }
    Err(line_error(
        line,
        "`validate` supports `sanitize_html(<profile>)`, `utf8_safe`, `max_recursion:<n>`, or `max_size:<bytes>`",
    ))
}

pub(super) fn parse_constraint_validator(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(String, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("validator ")
        .ok_or_else(|| line_error(line, "expected `validator covers_pii`"))?
        .trim_start();
    let (raw, tail) = take_constraint_value(rest);
    let value = raw.strip_prefix("covers_pii:").unwrap_or(raw);
    if value != "covers_pii" && !value.starts_with("covers_pii_") {
        return Err(line_error(
            line,
            "`validator` currently supports `covers_pii` entries only",
        ));
    }
    Ok((value.to_owned(), tail))
}

fn take_constraint_value(text: &str) -> (&str, String) {
    let trimmed = text.trim_start();
    let end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    (&trimmed[..end], trimmed[end..].to_owned())
}
