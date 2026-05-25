//! Feature-level `cache <name>` profile parser (CL.C.3 vocabulary).
//!
//! Cache profiles let an author declare a reusable read-side cache
//! recipe once, then attach it to multiple `query`/`api`/`report`
//! callables. The block sits at feature-child indent (2 spaces) with
//! body children at agent-child indent (4 spaces).
//!
//! ## Grammar (closed)
//!
//! ```text
//! cache <profile_name>
//!   key <expr>                       # required
//!   ttl <literal>                    # required
//!   namespace <label>                # optional
//!   tags <l1>[, <l2>, ...]           # optional
//!   stale_while_revalidate <literal> # optional
//!   coalesce true | false            # optional
//!   sliding true | false             # optional
//! ```
//!
//! `key` accepts arbitrary expressions verbatim; lowering interprets.
//! `ttl` and `stale_while_revalidate` accept duration literals that
//! the analyzer parses (e.g. `5m`, `1h`).
//!
//! Boolean decorators (`coalesce`, `sliding`) honour only `true`/`false` —
//! catalog-closed for cold-readability.
//!
//! ## See also
//!
//! - `docs/proposals/cache-profile-vocab.md`
//! - `lazuli_ir::nodes::cache` — typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::error::ParseError;
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD};

use crate::ast::{CacheProfileDecl, Span};

pub(super) fn parse_cache_profile(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CacheProfileDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("cache ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "cache profile header must be `cache <name>`"))?;
    if name.is_empty() {
        return Err(line_error(
            header,
            "feature-level `cache` header requires a profile name",
        ));
    }

    let mut key: Option<String> = None;
    let mut ttl: Option<String> = None;
    let mut namespace: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut stale_while_revalidate: Option<String> = None;
    let mut coalesce: Option<bool> = None;
    let mut sliding: Option<bool> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`cache <name>` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("key ") {
            key = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("ttl ") {
            ttl = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("namespace ") {
            namespace = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("tags ") {
            for part in rest.split(',') {
                let label = part.trim();
                if !label.is_empty() {
                    tags.push(label.to_owned());
                }
            }
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("stale_while_revalidate ") {
            stale_while_revalidate = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("coalesce ") {
            coalesce = Some(parse_cache_bool(line, rest.trim())?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("sliding ") {
            sliding = Some(parse_cache_bool(line, rest.trim())?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`cache <name>` children are `key <expr>`, `ttl <literal>`, \
                 `namespace <label>`, `tags <l1>[, <l2>...]`, \
                 `stale_while_revalidate <literal>`, `coalesce <bool>`, \
                 or `sliding <bool>`",
            ));
        }
    }

    let key = key
        .ok_or_else(|| line_error(header, "`cache <name>` requires a `key <expr>` declaration"))?;
    let ttl = ttl.ok_or_else(|| {
        line_error(
            header,
            "`cache <name>` requires a `ttl <literal>` declaration",
        )
    })?;

    Ok((
        CacheProfileDecl {
            name,
            key,
            ttl,
            namespace,
            tags,
            stale_while_revalidate,
            coalesce,
            sliding,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_cache_bool(line: &SourceLine<'_>, value: &str) -> Result<bool, ParseError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(line_error_owned(
            line,
            format!(
                "`cache` boolean decorators (`coalesce`, `sliding`) accept `true` or `false`, found `{other}`"
            ),
        )),
    }
}
