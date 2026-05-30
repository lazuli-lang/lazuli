//! `query.compose` W2 — predicate / order / key-clause lifting.
//!
//! The closed-predicate sublanguage helpers used by [`super::compose`] to
//! lower a `query.compose`'s `subselect` `where`/`filter` bodies, its local
//! safety `scope` predicates, root + `latest` `order` clauses, and the
//! single-row `key` clause. Split from `compose.rs` to keep each concern
//! file within the Rails-style 500-LOC ceiling.
//!
//! `where`/`filter`/`scope` bodies reuse the comparison lifter
//! ([`super::parse_query_filter_line`]) plus a `has` collection-membership
//! arm and the `in [...]` literal-set form (lowered to an OR of equalities —
//! the only set form the parser admits, §3.1). `AND`/`OR` combinators fold a
//! flat predicate list into a left-associative [`ir::Predicate`] tree.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::helpers::find_top_level_operator;

use super::{filter_rhs_expr, parse_query_filter_line};

/// Lower a list of parsed `subselect` predicates (`where`/`filter`) into the
/// closed [`ir::Predicate`] sublanguage. Each predicate is a scalar
/// comparison (`=`/`!=`), a `has` collection membership, or an `in
/// [literal,...]` literal-set test (the only set form — the parser already
/// rejected `in (subselect)` / `in params.x`). `AND`/`OR` combinators fold
/// the flat list into a single [`ir::Predicate`] tree (left-associative).
pub(super) fn lower_subselect_preds(preds: &[syntax::ComposeSubselectPred]) -> Vec<ir::Predicate> {
    fold_combined_predicates(preds.iter().map(lower_one_subselect_pred_paired).collect())
}

/// Lower verbatim `scope` body lines (`participants has ctx.user.id`,
/// `deleted_at = nil`) into the closed predicate sublanguage. A `has`
/// collection-membership line lifts to [`ir::Predicate::Has`] (the canonical
/// §4.1 safety predicate); everything else reuses the list-query filter-line
/// lifter so the comparison spelling stays consistent with `query.list`.
pub(super) fn lower_compose_scope_lines(lines: &[String]) -> Vec<ir::Predicate> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            // `<collection> has <element>` — collection membership, the one
            // closed predicate the comparison lifter doesn't model. Match the
            // ` has ` infix at a word boundary so column names containing the
            // substring don't trip it.
            if let Some(idx) = trimmed.find(" has ") {
                let collection = trimmed[..idx].trim();
                let element = trimmed[idx + " has ".len()..].trim();
                if !collection.is_empty() && !element.is_empty() {
                    return Some(ir::Predicate::Has {
                        collection: ir::Expr::Path(ir::Path::from_segments(
                            collection.split('.').map(str::to_owned),
                        )),
                        element: filter_rhs_expr(element),
                    });
                }
            }
            parse_query_filter_line(trimmed).map(|f| f.predicate)
        })
        .collect()
}

/// Lower a single parsed subselect predicate into one [`ir::Predicate`]
/// comparison/has node (combinator handled by the caller's fold).
fn lower_one_subselect_pred(pred: &syntax::ComposeSubselectPred) -> ir::Predicate {
    let left = ir::Expr::Path(ir::Path::from_segments(
        pred.left.split('.').map(str::to_owned),
    ));
    match &pred.op {
        syntax::ComposeSubselectPredOp::Eq(rhs) => ir::Predicate::Comparison {
            left,
            op: ir::CompareOp::Eq,
            right: filter_rhs_expr(rhs),
        },
        syntax::ComposeSubselectPredOp::Ne(rhs) => ir::Predicate::Comparison {
            left,
            op: ir::CompareOp::Ne,
            right: filter_rhs_expr(rhs),
        },
        syntax::ComposeSubselectPredOp::Has(rhs) => ir::Predicate::Has {
            collection: left,
            element: filter_rhs_expr(rhs),
        },
        // `in [a, b]` literal-set ⇒ `OR` of equalities over the same column
        // (the closed predicate sublanguage has no native set node; the
        // literal-set is the shipped inline-constraint form, §4.3 note).
        syntax::ComposeSubselectPredOp::In(values) => {
            let arms: Vec<ir::Predicate> = values
                .iter()
                .map(|v| ir::Predicate::Comparison {
                    left: left.clone(),
                    op: ir::CompareOp::Eq,
                    right: filter_rhs_expr(v),
                })
                .collect();
            match arms.len() {
                1 => arms.into_iter().next().expect("len checked"),
                _ => ir::Predicate::Or(arms),
            }
        }
    }
}

/// Fold a flat predicate list carrying per-element `AND`/`OR` combinators
/// into a single left-associative [`ir::Predicate`] tree. The first element
/// has no combinator; each subsequent element's combinator joins it to the
/// accumulated head. Returns the list verbatim when there are 0/1 predicates
/// (no folding needed) so the common single-predicate `where` stays flat.
fn fold_combined_predicates(parts: Vec<(Option<syntax::ComposePredCombinator>, ir::Predicate)>) -> Vec<ir::Predicate> {
    if parts.len() <= 1 {
        return parts.into_iter().map(|(_, p)| p).collect();
    }
    let mut iter = parts.into_iter();
    let (_, mut acc) = iter.next().expect("len > 1");
    for (combinator, pred) in iter {
        acc = match combinator {
            Some(syntax::ComposePredCombinator::Or) => ir::Predicate::Or(vec![acc, pred]),
            // `AND` (and the defensive `None`) compose conjunctively.
            _ => ir::Predicate::And(vec![acc, pred]),
        };
    }
    vec![acc]
}

/// Adapter so `lower_subselect_preds` can `.map(...)` a borrow into the
/// `(combinator, predicate)` pair `fold_combined_predicates` consumes.
fn lower_one_subselect_pred_paired(
    pred: &syntax::ComposeSubselectPred,
) -> (Option<syntax::ComposePredCombinator>, ir::Predicate) {
    (pred.combinator, lower_one_subselect_pred(pred))
}

/// Lower `order <field> <asc|desc>` lines into typed [`ir::OrderBy`] entries.
/// Bare `<field>` defaults to ascending; `desc` (case-insensitive) flips it.
/// Reused by the compose root `order` and the `latest` subselect `order`.
pub(super) fn lower_order_lines(lines: &[String]) -> Vec<ir::OrderBy> {
    lines
        .iter()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let field = parts.next()?.to_owned();
            let direction = match parts.next().map(str::to_ascii_lowercase).as_deref() {
                Some("desc") => ir::OrderDir::Desc,
                _ => ir::OrderDir::Asc,
            };
            Some(ir::OrderBy { field, direction })
        })
        .collect()
}

/// Lower a `key <path> = <expr>` clause into an [`ir::KeyClause`]. The LHS is
/// a column path (`self.id` → `["self","id"]`); the RHS is a value
/// expression (`params.property_id` → `Expr::Path`). Returns `None` when the
/// clause has no top-level `=` (defensive — the parser shapes it).
pub(super) fn parse_key_clause(text: &str) -> Option<ir::KeyClause> {
    let idx = find_top_level_operator(text, "=")?;
    let (lhs, rhs) = text.split_at(idx);
    let lhs = lhs.trim();
    let rhs = rhs[1..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some(ir::KeyClause {
        path: ir::Path::from_segments(lhs.split('.').map(str::to_owned)),
        equals: filter_rhs_expr(rhs),
    })
}
