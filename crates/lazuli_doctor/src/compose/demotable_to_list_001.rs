//! COMPOSE-DEMOTABLE-TO-LIST-001 — a compose with no joins and no subselects.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` declares **zero** `join` and **zero**
//! `subselect` — it is a single-resource read in `query.compose` costume.
//! Such a read has no cross-resource projection and no per-row sub-select, so
//! it is exactly what `query.list` (or `query.lookup`, when keyed) expresses.
//! Determinism (`grading-rubric.md` C5 — one canonical form): a single-resource
//! read MUST be `query.list`, not a `query.compose` that happens to project
//! only `self.*` columns.
//!
//! ## Severity profile
//!
//! Severity: `warning` in both strict and production profiles — a hygiene /
//! determinism nudge, not a correctness failure. The read still works; it just
//! has a more canonical spelling.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! feature catalog
//!   domain
//!     resource Property
//!       org: Org required
//!     query.compose property_rows
//!       from Property
//!       select
//!         property_id = self.id
//! ```
//!
//! Canonical fix — express it as a `query.list`:
//!
//! ```lzi
//! query.list property_rows
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-DEMOTABLE-TO-LIST-001`) + §3 (closed-form discipline). Diagnostic
//! ID / code constant: `COMPOSE-DEMOTABLE-TO-LIST-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use super::composes_of;

/// One COMPOSE-DEMOTABLE-TO-LIST-001 finding — a compose with no joins and no
/// subselects that should be a `query.list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-DEMOTABLE-TO-LIST-001";

    /// Render the determinism nudge toward `query.list`.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} has no `join` and no `subselect` — it is a single-resource read in \
             compose costume. Express it as `query.list` (the one canonical form for a \
             single-resource read), or `query.lookup` when it is keyed.",
            self.query_name
        )
    }
}

/// Run COMPOSE-DEMOTABLE-TO-LIST-001 over one feature.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::demotable_to_list_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    composes_of(feature)
        .filter(|compose| compose.joins.is_empty() && compose.subselects.is_empty())
        .map(|compose| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            query_name: compose.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn compose_without_joins_or_subselects_fires() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
    query.compose property_rows
      from Property
      select
        property_id = self.id
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].query_name, "property_rows");
        assert_eq!(Finding::CODE, "COMPOSE-DEMOTABLE-TO-LIST-001");
    }

    #[test]
    fn compose_with_a_join_does_not_fire() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
      host: Host required
    resource Host
      org: Org required
      name: Text required
    query.compose property_cards
      from Property
      join property.host as h
      select
        property_id = self.id
        host_name = h.name
"#,
        );

        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }

    #[test]
    fn compose_with_a_subselect_does_not_fire() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
    resource ServiceTransaction
      property: Property required
    query.compose property_kpis
      from Property
      subselect bookings = count ServiceTransaction
        related_by service_transaction.property
      select
        property_id = self.id
        bookings = bookings
"#,
        );

        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }
}
