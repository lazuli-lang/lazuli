//! `app.error_page <status>` — per-status HTTP error template.
//!
//! Declares a template (file path) and optional audience for a given
//! HTTP status code, letting the app override the framework defaults:
//!
//! ```text
//! error_page 404
//!   template "./errors/not_found.html"
//!   audience anonymous
//!
//! error_page 500
//!   template "./errors/internal.html"
//! ```
//!
//! `template` is mandatory; `audience` is optional and may carry a
//! single name (additional audiences require separate `error_page`
//! entries). Status must be a valid `u16` — anything that doesn't
//! parse triggers the canonical "must be an HTTP status code" error.

use crate::ast::{LzxErrorPage, Span};

use super::super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::super::error::ParseError;

pub(super) fn parse_lzx_error_page(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxErrorPage, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "error_page" {
        return Err(line_error(header, "error pages use `error_page <status>`"));
    }
    let status = parts[1]
        .parse::<u16>()
        .map_err(|_| line_error(header, "error page status must be an HTTP status code"))?;

    let mut template = None;
    let mut audience = None;
    let mut index = start + 1;

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
                "error_page children use four-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "error_page children are `template \"./...\"` or `audience <name>` declarations",
            ));
        }
        index += 1;
    }

    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`error_page` requires a `template \"./...\"` declaration",
        )
    })?;

    Ok((
        LzxErrorPage {
            status,
            template,
            audience,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}
