//! COMPOSE-SUBSELECT-PREDICATE-FIELD-001 — subselect predicate field/op invalid.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` subselect `where` / `filter` predicate either:
//!
//! - references a column (`<field>` or `self.<field>`) that does not exist on
//!   the subselect's resource — the mistyped / dropped anti-join clause (e.g.
//!   `where authr = ctx.user.id`); OR
//! - uses a forbidden ordered operator (`<` / `<=` / `>` / `>=`) — the §3.2 #3
//!   closed-predicate rule (ordered comparisons belong in `query.sql`).
//!
//! This is the IR-level backstop for the closed subselect-predicate language.
//! The parser already rejects the dynamic-set / correlated-subquery backdoor
//! (`in (subselect)` / `in params.x` / `in <expr>` are not productions); the
//! literal-set `in [...]` form lowers to an OR of equalities, so this rule's
//! field-existence check also covers the **semantic field-typing of the
//! literal-set members' LHS** (§7 — "doctor covers semantic field-typing of
//! the literal-set members"). The `author = ctx.user.id AND subject_kind = ...`
//! anti-join (`list_my_pending_reviews_as_traveler.go:53` in the audit) can no
//! longer drop or mistype a clause and still ship.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles — a predicate over
//! a non-existent field, or a forbidden operator, is a concrete bug.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! subselect already = exists Review
//!   related_by review.transaction
//!   where authr = ctx.user.id       # `authr` is not a field on Review
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-SUBSELECT-PREDICATE-FIELD-001`) + §3.1 (`literal_set`) + §3.2 #3.
//! Diagnostic ID / code constant: `COMPOSE-SUBSELECT-PREDICATE-FIELD-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{
    CompareOp, ComposeSubselect, Expr, Feature, Predicate, Resource, SubselectKind, TypeRef,
};

use super::{composes_of, resource_by_name};

/// One COMPOSE-SUBSELECT-PREDICATE-FIELD-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The subselect whose predicate is invalid.
    pub subselect: String,
    /// `"where"` or `"filter"` — which clause carried the problem.
    pub clause: &'static str,
    /// Human reason — `"field `authr` is not on `Review`"` or `"forbidden
    /// ordered operator"`.
    pub reason: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-SUBSELECT-PREDICATE-FIELD-001";

    /// Render the invalid-predicate message.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} subselect `{}` {} predicate: {}. Subselect predicates use the closed \
             language (`=`/`!=`/`has`/`AND`/`OR` + `in [literals]`) over the subselect resource's \
             own fields; ordered comparisons (`<`/`>`) and unknown fields are rejected — move them \
             to `query.sql`.",
            self.query_name, self.subselect, self.clause, self.reason
        )
    }
}

/// Run COMPOSE-SUBSELECT-PREDICATE-FIELD-001 over one feature.
///
/// Field existence is enforced only when the subselect's resource is
/// in-feature; the forbidden-operator check fires regardless of resolution
/// (it is a structural rule, not a reference check).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::subselect_predicate_field_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("trust.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for compose in composes_of(feature) {
        for sub in &compose.subselects {
            let child = subselect_resource_name(&sub.kind)
                .and_then(|name| resource_by_name(&feature.resources, &name));
            collect_clause(
                &sub.where_pred,
                "where",
                sub,
                child,
                compose.name.as_str(),
                &feature.name,
                path,
                &mut findings,
            );
            collect_clause(
                &sub.filter_pred,
                "filter",
                sub,
                child,
                compose.name.as_str(),
                &feature.name,
                path,
                &mut findings,
            );
        }
    }
    findings
}

/// Walk one clause's predicate list, pushing a finding per problem.
#[allow(clippy::too_many_arguments)]
fn collect_clause(
    preds: &[Predicate],
    clause: &'static str,
    sub: &ComposeSubselect,
    child: Option<&Resource>,
    query_name: &str,
    feature_name: &str,
    path: &Path,
    out: &mut Vec<Finding>,
) {
    let mut reasons = Vec::new();
    for pred in preds {
        walk_predicate(pred, child, &mut reasons);
    }
    for reason in reasons {
        out.push(Finding {
            path: path.to_path_buf(),
            feature: feature_name.to_owned(),
            query_name: query_name.to_owned(),
            subselect: sub.name.clone(),
            clause,
            reason,
        });
    }
}

/// Recursively collect predicate problems into `reasons`.
fn walk_predicate(pred: &Predicate, child: Option<&Resource>, reasons: &mut Vec<String>) {
    match pred {
        Predicate::And(arms) | Predicate::Or(arms) => {
            for arm in arms {
                walk_predicate(arm, child, reasons);
            }
        }
        Predicate::Has { collection, .. } => {
            check_self_field(collection, child, reasons);
        }
        Predicate::Comparison { left, op, .. } => {
            if is_ordered(*op) {
                reasons.push("forbidden ordered operator (`<`/`<=`/`>`/`>=`)".to_owned());
            }
            check_self_field(left, child, reasons);
        }
    }
}

/// When `expr` references one of the subselect resource's OWN columns
/// (`<field>` or `self.<field>`), verify it exists on `child`. References to
/// `ctx.*` or a joined `<alias>.*` are left alone — they are not the
/// subselect resource's fields.
fn check_self_field(expr: &Expr, child: Option<&Resource>, reasons: &mut Vec<String>) {
    let Expr::Path(p) = expr else { return };
    let field = match p.segments.as_slice() {
        [field] if field != "ctx" => field.as_str(),
        [head, field] if head == "self" => field.as_str(),
        _ => return,
    };
    let Some(resource) = child else { return };
    if !column_exists(resource, field) {
        reasons.push(format!("field `{field}` is not on `{}`", resource.name));
    }
}

/// Whether `col` is an authored field on `resource` or a framework-implicit
/// column.
fn column_exists(resource: &Resource, col: &str) -> bool {
    const IMPLICIT: &[&str] = &["id", "created_at", "updated_at", "deleted_at"];
    IMPLICIT.contains(&col) || resource.fields.iter().any(|f| f.name == col)
}

/// Whether the comparison operator is an ordered (forbidden) one.
fn is_ordered(op: CompareOp) -> bool {
    matches!(op, CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge)
}

/// The child resource name a subselect kind targets, when `UserDefined`
/// in-feature.
fn subselect_resource_name(kind: &SubselectKind) -> Option<String> {
    let type_ref = match kind {
        SubselectKind::Count(r) => r,
        SubselectKind::Exists { resource, .. } => resource,
        SubselectKind::Latest { resource, .. } => resource,
        SubselectKind::Aggregate { resource, .. } => resource,
    };
    match type_ref {
        TypeRef::UserDefined(qname) if qname.feature.is_none() => Some(qname.name.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn mistyped_where_field_fires() {
        // `authr` is a typo for `author` — the dropped/mistyped anti-join clause.
        let feature = lower(
            r#"
feature trust
  domain
    resource ServiceTransaction
      org: Org required
    resource Review
      transaction: ServiceTransaction required
      author: User required
    query.compose pending
      from ServiceTransaction
      subselect already = exists Review
        related_by review.transaction
        where authr = ctx.user.id
      select
        transaction_id = self.id
        already = already
"#,
        );

        let findings = check(&feature, Path::new("trust.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].subselect, "already");
        assert_eq!(findings[0].clause, "where");
        assert!(findings[0].reason.contains("authr"));
        assert_eq!(Finding::CODE, "COMPOSE-SUBSELECT-PREDICATE-FIELD-001");
    }

    #[test]
    fn literal_set_member_field_typo_in_filter_fires() {
        // `staus in [paid, completed]` — the literal-set LHS field is mistyped.
        // The filter lowers to an OR of equalities; the LHS field is checked.
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
    resource ServiceTransaction
      property: Property required
      total_amount_cents: Integer required
      status: Text required
    query.compose property_kpis
      from Property
      subselect revenue = aggregate sum total_amount_cents of ServiceTransaction
        related_by service_transaction.property
        filter staus in [paid, completed]
      select
        property_id = self.id
        revenue = revenue
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));
        assert!(!findings.is_empty(), "mistyped literal-set LHS must fire");
        assert_eq!(findings[0].clause, "filter");
        assert!(findings[0].reason.contains("staus"));
    }

    #[test]
    fn valid_predicate_does_not_fire() {
        let feature = lower(
            r#"
feature trust
  domain
    resource ServiceTransaction
      org: Org required
    resource Review
      transaction: ServiceTransaction required
      author: User required
      subject_kind: Text required
    query.compose pending
      from ServiceTransaction
      subselect already = exists Review
        related_by review.transaction
        where author = ctx.user.id AND subject_kind = "traveler_to_host"
      select
        transaction_id = self.id
        already = already
"#,
        );

        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }
}
