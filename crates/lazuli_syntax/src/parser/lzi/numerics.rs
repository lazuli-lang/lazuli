//! Numeric and rate-limit lexer helpers shared across every `.lzi`
//! construct that admits a numeric tail. The `parse_*` family error out
//! with parser-friendly messages anchored to the offending source line
//! and lets the call sites stay declarative.
//!
//! ## Functions
//!
//! - `parse_float` — decimal literal (`f64`).
//! - `parse_uint32` — non-negative integer.
//! - `parse_int64` — signed integer.
//! - `parse_rate_limit_line_body` — splits `"<spec>"` from optional
//!   `in <env_list>` tail; returns the literal + envs.
//! - `fold_rate_limit_line` — accumulates parsed lines into the
//!   in-progress `RateLimitSpecAst` (default + by_env), enforcing the
//!   `rate_limit_duplicate_default` rule.
//!
//! The rate-limit helpers are `pub(super)` so every declaration that
//! carries a `rate_limit` line (agent, command, api, auth, query)
//! shares the same fold. Numeric helpers are `pub(super)` to feed the
//! agent block (temperature / max_tokens / top_p / seed).

use super::super::common::{SourceLine, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use crate::ast::{RateLimitByEnvAst, RateLimitSpecAst, Span};

pub(super) fn parse_float(line: &SourceLine<'_>, rest: &str) -> Result<f64, ParseError> {
    rest.trim()
        .parse::<f64>()
        .map_err(|_| line_error(line, "expected a decimal value (e.g. `0`, `0.2`)"))
}

pub(super) fn parse_uint32(line: &SourceLine<'_>, rest: &str) -> Result<u32, ParseError> {
    rest.trim()
        .parse::<u32>()
        .map_err(|_| line_error(line, "expected a non-negative integer"))
}

pub(super) fn parse_int64(line: &SourceLine<'_>, rest: &str) -> Result<i64, ParseError> {
    rest.trim()
        .parse::<i64>()
        .map_err(|_| line_error(line, "expected a signed integer"))
}

pub(super) fn parse_rate_limit_line_body(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<Vec<String>>), ParseError> {
    let trimmed = rest.trim_start();

    // The literal is a double-quoted string. Find the closing quote so
    // we can isolate the optional `in <env_list>` tail.
    if !trimmed.starts_with('"') {
        return Err(line_error(
            line,
            "`rate_limit` requires a quoted spec literal (e.g. `rate_limit \"5 per minute per ip\"`)",
        ));
    }
    let after_open = &trimmed[1..];
    let close_offset = after_open.find('"').ok_or_else(|| {
        line_error(
            line,
            "`rate_limit` spec literal is missing its closing quote",
        )
    })?;
    let literal = &after_open[..close_offset];
    if literal.is_empty() {
        return Err(line_error(
            line,
            "`rate_limit` spec literal must be non-empty",
        ));
    }

    let tail = after_open[close_offset + 1..].trim_start();
    if tail.is_empty() {
        return Ok((literal.to_owned(), None));
    }

    // The only legal tail today is `in <env_list>`.
    let Some(after_in) = tail.strip_prefix("in") else {
        return Err(line_error(
            line,
            "`rate_limit` literal may be followed only by `in <env_list>`",
        ));
    };
    // `in` must be separated from what follows by whitespace (so
    // `intermediate` doesn't accidentally parse). When the suffix is
    // empty (just `in` on its own), `strip_prefix` returns "", which
    // we explicitly reject below as an empty env list.
    let env_list_text = if let Some(stripped) = after_in.strip_prefix(char::is_whitespace) {
        stripped.trim()
    } else if after_in.is_empty() {
        ""
    } else {
        return Err(line_error(
            line,
            "`rate_limit` literal may be followed only by `in <env_list>`",
        ));
    };

    if env_list_text.is_empty() {
        return Err(line_error(
            line,
            "`rate_limit \"X\" in` requires at least one env name (e.g. `in dev, staging, test`)",
        ));
    }

    let mut envs: Vec<String> = Vec::new();
    for part in env_list_text.split(',') {
        let name = part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`rate_limit` env list cannot contain empty entries (drop the trailing/extra comma)",
            ));
        }
        // The closed catalog is validated at the analyzer / Cell 3
        // doctor layer; the parser accepts any identifier here and
        // surfaces unknown names as a doctor warning later.
        envs.push(name.to_owned());
    }

    Ok((literal.to_owned(), Some(envs)))
}

/// `ir-rate-limit-env-aware` cell 1 — accumulate one `rate_limit` line
/// into the in-progress `RateLimitSpecAst` for a declaration.
///
/// The first unqualified line establishes the default; further
/// unqualified lines are rejected (`rate_limit_duplicate_default`).
/// Qualified lines are appended to `by_env` in source order.
pub(super) fn fold_rate_limit_line(
    line: &SourceLine<'_>,
    spec: &mut Option<RateLimitSpecAst>,
    literal: String,
    envs: Option<Vec<String>>,
) -> Result<(), ParseError> {
    let entry_span = Span::new(line.start, line.end);
    let aggregate = spec.get_or_insert_with(|| RateLimitSpecAst {
        default: None,
        by_env: Vec::new(),
        span: entry_span,
    });
    aggregate.span = Span::new(aggregate.span.start, entry_span.end);

    match envs {
        None => {
            if aggregate.default.is_some() {
                return Err(line_error(
                    line,
                    "rate_limit_duplicate_default: declaration already carries an unqualified `rate_limit` line — add `in <env_list>` to differentiate or remove one",
                ));
            }
            aggregate.default = Some(unquote_lzx_value(&literal).to_owned());
        }
        Some(envs) => {
            aggregate.by_env.push(RateLimitByEnvAst {
                limit: literal,
                envs,
                span: entry_span,
            });
        }
    }

    Ok(())
}
