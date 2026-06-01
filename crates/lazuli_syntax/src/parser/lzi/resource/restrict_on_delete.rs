//! Spec 0014 — resource-scoped `restrict on_delete references <relation>
//! via <fk> [where <predicate>]` referential-guard line parser.
//!
//! `restrict on_delete` declares that the protected resource cannot be
//! deleted while live rows of `<relation>` still point at it through the
//! `<fk>` column. The clause is repeatable (one per inbound relation).
//! The optional trailing `where <predicate>` subset filter narrows the
//! guard to a subset of references (e.g. only *open* activities).
//!
//! ```text
//! resource BillingType
//!   restrict on_delete references invoice via billing_type_id
//!   restrict on_delete references activity via step_id where status = 'open'
//! ```
//!
//! The parser is intentionally permissive about the relation / fk / where
//! text — it captures the raw tokens and lets the analyzer resolve the
//! relation, derive the tenant-scope + soft-delete predicates from its
//! schema, and type-check the `where` predicate.
//!
//! Visibility: `parse_resource_restrict_on_delete` is `pub(super)` — the
//! only caller is the `restrict ` dispatch arm in `resource/mod.rs`.

use super::super::super::common::{SourceLine, line_error};
use super::super::super::error::ParseError;

use crate::ast::{ResourceRestrictOnDelete, Span};

/// Parse the text AFTER the leading `restrict ` keyword:
/// `on_delete references <relation> via <fk> [where <predicate>]`.
pub(super) fn parse_resource_restrict_on_delete(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ResourceRestrictOnDelete, ParseError> {
    let rest = rest.trim();
    let after_on_delete = rest.strip_prefix("on_delete ").ok_or_else(|| {
        line_error(
            line,
            "`restrict` requires `on_delete references <relation> via <fk> [where <predicate>]` \
             (e.g. `restrict on_delete references invoice via billing_type_id`)",
        )
    })?;
    let after_references = after_on_delete.trim().strip_prefix("references ").ok_or_else(|| {
        line_error(
            line,
            "`restrict on_delete` requires `references <relation> via <fk>` \
             (e.g. `restrict on_delete references invoice via billing_type_id`)",
        )
    })?;
    // Split the relation off the ` via ` connector.
    let (relation, after_via) = after_references.split_once(" via ").ok_or_else(|| {
        line_error(
            line,
            "`restrict on_delete references <relation>` requires a `via <fk>` clause naming \
             the referencing column (e.g. `via billing_type_id`)",
        )
    })?;
    let relation = relation.trim();
    if relation.is_empty() || relation.split_whitespace().count() != 1 {
        return Err(line_error(
            line,
            "`restrict on_delete references` takes exactly one relation name before `via`",
        ));
    }
    // The fk runs until an optional ` where ` subset clause.
    let (fk, extra_where) = match after_via.split_once(" where ") {
        Some((fk, pred)) => {
            let pred = pred.trim();
            if pred.is_empty() {
                return Err(line_error(
                    line,
                    "`restrict on_delete ... where` requires a predicate (e.g. `where status = 'open'`)",
                ));
            }
            (fk.trim(), Some(pred.to_owned()))
        }
        None => (after_via.trim(), None),
    };
    if fk.is_empty() || fk.split_whitespace().count() != 1 {
        return Err(line_error(
            line,
            "`restrict on_delete ... via <fk>` takes exactly one foreign-key column name",
        ));
    }
    Ok(ResourceRestrictOnDelete {
        relation: relation.to_owned(),
        fk: fk.to_owned(),
        extra_where,
        span: Span::new(line.start, line.end),
    })
}
