//! Resource-level concurrency / primary-key shape parsers
//! (Roadmap §1.5 — CL.C.2):
//!
//! - `lock <strategy>` is a single-line decorator at most once per
//!   resource. Closed catalog:
//!   - `lock optimistic version_field: <field>`
//!   - `lock pessimistic`
//!   - `lock row_level`
//!
//! - `composite_key` is a block at child indent; the body lives at
//!   grandchild indent and accepts:
//!   - `fields <a>, <b>, ...` (required, non-empty)
//!   - `primary true|false`  (optional; defaults to `false`)
//!
//! ```text
//! resource OrderLine
//!   lock optimistic version_field: lock_version
//!   composite_key
//!     fields order, line_number
//!     primary true
//! ```
//!
//! Visibility: both parsers are `pub(super)` — only the resource
//! dispatcher in `resource/mod.rs` calls them.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::super::error::ParseError;

use crate::ast::{ResourceCompositeKey, ResourceLock, Span};

pub(super) fn parse_resource_lock(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ResourceLock, ParseError> {
    let rest = rest.trim();
    if let Some(after) = rest.strip_prefix("optimistic") {
        let after = after.trim_start();
        let after = after.strip_prefix("version_field").ok_or_else(|| {
            line_error(
                line,
                "`lock optimistic` requires `version_field: <field>` (e.g. `lock optimistic version_field: lock_version`)",
            )
        })?;
        let after = after.trim_start();
        let after = after.strip_prefix(':').ok_or_else(|| {
            line_error(
                line,
                "`lock optimistic version_field` expects `:` followed by the column name",
            )
        })?;
        let version_field = after.trim().to_owned();
        if version_field.is_empty() {
            return Err(line_error(
                line,
                "`lock optimistic version_field:` requires a non-empty field name",
            ));
        }
        return Ok(ResourceLock::Optimistic { version_field });
    }
    match rest {
        "pessimistic" => Ok(ResourceLock::Pessimistic),
        "row_level" => Ok(ResourceLock::RowLevel),
        other => Err(line_error_owned(
            line,
            format!(
                "`lock` expects `optimistic version_field: <field>`, `pessimistic`, or `row_level` (got `{}`)",
                other
            ),
        )),
    }
}

pub(super) fn parse_resource_composite_key(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(ResourceCompositeKey, usize), ParseError> {
    let header = &lines[start];

    let mut fields: Vec<String> = Vec::new();
    let mut primary: bool = false;
    let mut saw_primary = false;
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
                "`composite_key` children use one indentation level deeper than the header",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("fields ") {
            if !fields.is_empty() {
                return Err(line_error(
                    line,
                    "duplicate `fields` line in `composite_key`",
                ));
            }
            let names: Vec<String> = rest
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                return Err(line_error(
                    line,
                    "`fields` requires at least one field name (e.g. `fields order, line_number`)",
                ));
            }
            fields = names;
            last_end = line.end;
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("primary ") {
            if saw_primary {
                return Err(line_error(
                    line,
                    "duplicate `primary` line in `composite_key`",
                ));
            }
            primary = match rest.trim() {
                "true" => true,
                "false" => false,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!("`primary` expects `true` or `false` (got `{}`)", other),
                    ));
                }
            };
            saw_primary = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        return Err(line_error(
            line,
            "`composite_key` children are `fields <list>` and `primary true|false`",
        ));
    }

    if fields.is_empty() {
        return Err(line_error(
            header,
            "`composite_key` requires a `fields <a>, <b>, ...` child",
        ));
    }

    Ok((
        ResourceCompositeKey {
            fields,
            primary,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
