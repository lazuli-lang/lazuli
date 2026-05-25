//! Shared parsers for resource-level DDL constraints — `index on`,
//! `unique (...)`, `fts on (...)`.
//!
//! All three handlers in `resource/mod.rs` consume the same primitives:
//!
//! - `parse_resource_index_target` — accepts either a bare field name
//!   (`index on email btree`) or a parenthesised field list
//!   (`index on (org_id, created_at)`), then optionally trailing
//!   `[using] <method>`.
//! - `parse_parenthesized_field_list_with_trailing` — splits
//!   `(a, b, c) <trailing...>` into a `Vec<String>` and the remainder.
//!   Used by `index`, `unique`, and `fts`.
//! - `parse_resource_field_list` — validates a comma-separated identifier
//!   list (rejects empties, runs `is_policy_identifier` on each field).
//! - `parse_resource_index_method` — closed catalog `btree | gin | gist`
//!   with optional `using ` prefix.
//!
//! Visibility: every parser here is `pub(super)` — the resource
//! dispatcher in `resource/mod.rs` is the only caller.

use super::super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::super::error::ParseError;
use super::super::is_policy_identifier;

use crate::ast::ResourceIndexMethodAst;

pub(super) fn parse_resource_index_target(
    line: &SourceLine<'_>,
    target: &str,
) -> Result<(Vec<String>, Option<ResourceIndexMethodAst>), ParseError> {
    let (fields, trailing) = if target.starts_with('(') {
        parse_parenthesized_field_list_with_trailing(line, target)?
    } else {
        let mut parts = target.splitn(2, char::is_whitespace);
        let field = parts.next().unwrap_or("").trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "`index on` requires a field name or parenthesized field list",
            ));
        }
        if !is_policy_identifier(field) {
            return Err(line_error_owned(
                line,
                format!("`{field}` is not a valid field name in `index on`"),
            ));
        }
        (vec![field.to_owned()], parts.next().unwrap_or("").trim())
    };
    let method = parse_resource_index_method(line, trailing.trim())?;
    Ok((fields, method))
}

pub(super) fn parse_parenthesized_field_list_with_trailing<'a>(
    line: &SourceLine<'_>,
    text: &'a str,
) -> Result<(Vec<String>, &'a str), ParseError> {
    let text = text.trim();
    if !text.starts_with('(') {
        return Err(line_error(line, "expected parenthesized field list"));
    }
    let Some(end) = text.find(')') else {
        return Err(line_error(line, "field list is missing its closing `)`"));
    };
    let inner = &text[1..end];
    let fields = parse_resource_field_list(line, inner)?;
    Ok((fields, &text[end + 1..]))
}

fn parse_resource_field_list(
    line: &SourceLine<'_>,
    fields: &str,
) -> Result<Vec<String>, ParseError> {
    let parsed: Vec<String> = fields
        .split(',')
        .map(|field| field.trim().to_owned())
        .filter(|field| !field.is_empty())
        .collect();
    if parsed.is_empty() {
        return Err(line_error(
            line,
            "field list requires at least one field name",
        ));
    }
    for field in &parsed {
        if !is_policy_identifier(field) {
            return Err(line_error_owned(
                line,
                format!("`{field}` is not a valid field name in this list"),
            ));
        }
    }
    Ok(parsed)
}

fn parse_resource_index_method(
    line: &SourceLine<'_>,
    trailing: &str,
) -> Result<Option<ResourceIndexMethodAst>, ParseError> {
    let trailing = trailing.trim();
    if trailing.is_empty() {
        return Ok(None);
    }
    let method = trailing
        .strip_prefix("using ")
        .map(str::trim)
        .unwrap_or(trailing);
    let parsed = match method {
        "btree" => ResourceIndexMethodAst::Btree,
        "gin" => ResourceIndexMethodAst::Gin,
        "gist" => ResourceIndexMethodAst::Gist,
        other => {
            return Err(line_error_owned(
                line,
                format!(
                    "`index on` supports optional methods `btree`, `gin`, or `gist` (got `{other}`)"
                ),
            ));
        }
    };
    Ok(Some(parsed))
}
