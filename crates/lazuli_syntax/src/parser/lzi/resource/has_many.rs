//! Resource-scoped `has_many <name>: <Type> [inverse <field>]` parser.
//!
//! `has_many` declares a one-to-many relation from a parent resource to
//! a child resource. The `inverse` clause is optional and names the
//! belongs-to column on the child side (used by codegen when emitting
//! foreign-key joins).
//!
//! ```text
//! resource Customer
//!   has_many notes: CustomerNote inverse customer
//! ```
//!
//! The parser is intentionally permissive about the type text — it
//! stores the raw RHS and lets the analyzer resolve the reference
//! against the feature's resource set.
//!
//! Visibility: `parse_resource_has_many` is `pub(super)` — the only
//! caller is `handle_resource_has_many` in `resource/mod.rs`.

use super::super::super::common::{SourceLine, line_error};
use super::super::super::error::ParseError;

use crate::ast::{ResourceHasMany, Span};

pub(super) fn parse_resource_has_many(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ResourceHasMany, ParseError> {
    let (name, after) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`has_many` requires `<name>: <Type> [inverse <field>]` (e.g. `has_many notes: CustomerNote inverse customer`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`has_many` requires a relation name before `:`",
        ));
    }
    let after = after.trim();
    let (type_text, inverse) = if let Some(idx) = after.find(" inverse ") {
        (
            after[..idx].trim().to_owned(),
            Some(after[idx + " inverse ".len()..].trim().to_owned()),
        )
    } else {
        (after.to_owned(), None)
    };
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`has_many` requires a resource type after `:`",
        ));
    }
    Ok(ResourceHasMany {
        name: name.to_owned(),
        type_text,
        inverse,
        span: Span::new(line.start, line.end),
    })
}
