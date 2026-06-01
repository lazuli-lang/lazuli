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
/// `on_delete references <relation> via <fk> [error <CODE>] [where <predicate>]`.
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
    // Grammar after `via`: `<fk> [error <CODE>] [where <predicate>]`.
    //
    // The `where` predicate is free-form (runs to end of line), so it must be
    // split off FIRST. The `error <CODE>` clause is a single bareword and sits
    // between the fk and any `where`, so we split it off the pre-`where` head.
    let (before_where, extra_where) = match after_via.split_once(" where ") {
        Some((head, pred)) => {
            let pred = pred.trim();
            if pred.is_empty() {
                return Err(line_error(
                    line,
                    "`restrict on_delete ... where` requires a predicate (e.g. `where status = 'open'`)",
                ));
            }
            (head, Some(pred.to_owned()))
        }
        None => (after_via, None),
    };
    // Spec 0014 GAP-2 — optional `error <CODE>` clause (pins a per-guard
    // domain error code; absent → bare `ErrReferencedInUse` sentinel).
    let (fk, error_code) = match before_where.split_once(" error ") {
        Some((fk, code)) => {
            let code = code.trim();
            if code.is_empty() || code.split_whitespace().count() != 1 {
                return Err(line_error(
                    line,
                    "`restrict on_delete ... error <CODE>` takes exactly one domain error code \
                     (e.g. `error CATEGORY_HAS_CUSTOMERS`)",
                ));
            }
            (fk.trim(), Some(code.to_owned()))
        }
        None => {
            // Bare trailing `error` with no code (e.g. `via fk error`) lands
            // here because there is no ` error ` (trailing-space) separator;
            // catch it so the author gets the actionable diagnostic.
            let trimmed = before_where.trim();
            if trimmed.ends_with(" error") || trimmed == "error" {
                return Err(line_error(
                    line,
                    "`restrict on_delete ... error <CODE>` requires a domain error code \
                     (e.g. `error CATEGORY_HAS_CUSTOMERS`)",
                ));
            }
            (trimmed, None)
        }
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
        error_code,
        span: Span::new(line.start, line.end),
    })
}
