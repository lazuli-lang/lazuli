//! COMPOSE-SUBSELECT-CATALOG-001 — sub-select outside the closed scalar catalog.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` sub-select tries to express a **grouped /
//! ordered sub-list** instead of a scalar-per-row value. The closed catalog is
//! four kinds (`count` / `exists` / `latest` / `aggregate`) returning ONE
//! scalar per root row; `order` is meaningful only for `latest` (pick the
//! newest row). An `order` clause on a `count` / `exists` / `aggregate`
//! sub-select is the IR fingerprint of a grouped/ordered sub-list (the top-5
//! services `GROUP BY ... ORDER BY count LIMIT 5` shape,
//! `get_property_dashboard.go:103` in the audit) — that returns rows, not a
//! scalar, and belongs in `query.sql`.
//!
//! The four kinds and five aggregate functions are themselves structurally
//! closed by the IR enums ([`lazuli_ir::SubselectKind`] / [`lazuli_ir::AggFn`]),
//! so the surface can never reopen into arbitrary subqueries. This rule guards
//! the one residual way to smuggle a non-scalar shape through the closed
//! kinds — an ordered aggregation — keeping grouped sub-lists pushed to the
//! escape hatch.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles — a grouped sub-list
//! masquerading as a scalar sub-select would generate wrong SQL.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! subselect top_revenue = aggregate sum amount of ServiceTransaction
//!   related_by service_transaction.property
//!   order amount desc        # `order` on a non-`latest` ⇒ grouped sub-list
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-SUBSELECT-CATALOG-001`) + §3.1 (closed-catalog discipline) + §6
//! (grouped sub-list stays `query.sql`). Diagnostic ID / code constant:
//! `COMPOSE-SUBSELECT-CATALOG-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{ComposeSubselect, Feature, SubselectKind};

use super::composes_of;

/// One COMPOSE-SUBSELECT-CATALOG-001 finding — a sub-select using a
/// non-scalar (grouped/ordered) shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The offending sub-select.
    pub subselect: String,
    /// The sub-select kind tag (`count` / `exists` / `aggregate`).
    pub kind: &'static str,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-SUBSELECT-CATALOG-001";

    /// Render the closed-catalog message.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} sub-select `{}` ({}) declares `order`, but `order` is only valid on \
             `latest` (which picks one row). An ordered/grouped aggregation returns rows, not a \
             scalar — it is outside the closed scalar sub-select catalog; express it with \
             `query.sql`.",
            self.query_name, self.subselect, self.kind
        )
    }
}

/// Run COMPOSE-SUBSELECT-CATALOG-001 over one feature.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::subselect_catalog_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for compose in composes_of(feature) {
        for sub in &compose.subselects {
            if let Some(kind) = non_scalar_ordered_kind(sub) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    query_name: compose.name.clone(),
                    subselect: sub.name.clone(),
                    kind,
                });
            }
        }
    }
    findings
}

/// `Some(kind_tag)` when the sub-select declares `order` on a kind other than
/// `latest` (the grouped/ordered-sub-list fingerprint); `None` otherwise.
fn non_scalar_ordered_kind(sub: &ComposeSubselect) -> Option<&'static str> {
    if sub.order.is_empty() {
        return None;
    }
    match sub.kind {
        SubselectKind::Latest { .. } => None,
        SubselectKind::Count(_) => Some("count"),
        SubselectKind::Exists { .. } => Some("exists"),
        SubselectKind::Aggregate { .. } => Some("aggregate"),
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
    fn ordered_aggregate_fires() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
    resource ServiceTransaction
      property: Property required
      amount: Integer required
    query.compose property_kpis
      from Property
      subselect top_revenue = aggregate sum amount of ServiceTransaction
        related_by service_transaction.property
        order amount desc
      select
        property_id = self.id
        top_revenue = top_revenue
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].subselect, "top_revenue");
        assert_eq!(findings[0].kind, "aggregate");
        assert_eq!(Finding::CODE, "COMPOSE-SUBSELECT-CATALOG-001");
    }

    #[test]
    fn order_on_latest_does_not_fire() {
        // `order` IS valid on `latest` — it picks the newest row.
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
    resource ChatMessage
      chat: Chat required
      body: Text required
      created_at: DateTime required
    query.compose chat_inbox
      from Chat
      subselect last = latest body of ChatMessage
        related_by chat_message.chat
        order created_at desc
      select
        chat_id = self.id
        last = last
"#,
        );

        assert!(check(&feature, Path::new("messaging.lzi")).is_empty());
    }

    #[test]
    fn scalar_count_without_order_does_not_fire() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
    resource ChatMessage
      chat: Chat required
    query.compose chat_inbox
      from Chat
      subselect unread = count ChatMessage
        related_by chat_message.chat
      select
        chat_id = self.id
        unread = unread
"#,
        );

        assert!(check(&feature, Path::new("messaging.lzi")).is_empty());
    }
}
