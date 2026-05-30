//! COMPOSE-PROJECTION-SOURCE-001 — a projected column has no source column.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` projection (`<field> = self.<col>` or
//! `<field> = <alias>.<col>`) names a column that does not exist on its
//! resolved resource:
//!
//! - `self.<col>` — `<col>` is not a field on the root resource (nor a
//!   framework-implicit column like `id` / `created_at` / `updated_at` /
//!   `deleted_at`).
//! - `<alias>.<col>` — `<col>` is not a field on the resource the join alias
//!   resolves to.
//!
//! W2 resolved the projection *shape* (that the alias / subselect *name* is
//! declared); the column-against-the-resource check is the doctor concern W2
//! defers (see `crates/lazuli_analyzer/src/query/compose.rs` —
//! "root column existence is a doctor concern"). This is the typed-projection
//! guarantee `query.sql`'s `returns` never gives: the generated record and the
//! read cannot drift, because every projected column resolves to a real source
//! column (`canonical-semantics.md:648`).
//!
//! Subselect-sourced projections (`<field> = <subselect_name>`) are not checked
//! here — their name is resolved by W2 and their shape by
//! `COMPOSE-SUBSELECT-*`.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles — a projection with
//! no source column is a concrete bug (typo, renamed column), not style drift.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! query.compose property_cards
//!   from Property
//!   join property.host as h
//!   select
//!     host_name = h.nme        # `nme` is not a field on Host
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-PROJECTION-SOURCE-001`). Diagnostic ID / code constant:
//! `COMPOSE-PROJECTION-SOURCE-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{
    ComposeJoin, ComposeQuery, Feature, ProjectionSource, Resource, TypeRef,
};

use super::{composes_of, resolve_fk_path_target};

/// Framework-implicit columns present on every resource regardless of whether
/// the author declared them — `id` is the synthetic PK; the timestamp /
/// soft-delete columns are added by `timestamps` / `soft_delete`. A projection
/// of one of these never counts as a missing source column.
const IMPLICIT_COLUMNS: &[&str] = &["id", "created_at", "updated_at", "deleted_at"];

/// One COMPOSE-PROJECTION-SOURCE-001 finding — a projected column with no
/// resolvable source column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The output field whose source column is unresolvable.
    pub field: String,
    /// The unresolvable source text (`self.<col>` / `<alias>.<col>`).
    pub source_text: String,
    /// The resource the column was looked up on.
    pub on_resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-PROJECTION-SOURCE-001";

    /// Render the unresolved-projection-source message.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} projection `{}` reads `{}`, but `{}` has no such column. \
             Every `query.compose` projection must resolve to a real source column so the \
             generated record cannot drift from the read.",
            self.query_name, self.field, self.source_text, self.on_resource
        )
    }
}

/// Run COMPOSE-PROJECTION-SOURCE-001 over one feature.
///
/// Column resolution is enforced only for in-feature resources; a projection
/// whose root / joined resource lives cross-feature (or whose join path didn't
/// resolve in-feature) is skipped — `COMPOSE-JOIN-PATH-001` owns the unresolved
/// hop, and cross-feature column existence is validated when the Module graph
/// is present.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::projection_source_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for compose in composes_of(feature) {
        for proj in &compose.projections {
            if let Some((source_text, on_resource)) =
                unresolved_source(compose, proj, &feature.resources)
            {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    query_name: compose.name.clone(),
                    field: proj.name.clone(),
                    source_text,
                    on_resource,
                });
            }
        }
    }
    findings
}

/// Return `Some((source_text, resource_name))` when a projection's column does
/// not exist on its resolved in-feature resource. `None` when it resolves, or
/// when the resource can't be resolved in-feature (deferred).
fn unresolved_source(
    compose: &ComposeQuery,
    proj: &lazuli_ir::ComposeProjection,
    resources: &[Resource],
) -> Option<(String, String)> {
    match &proj.source {
        ProjectionSource::SelfCol(col) => {
            let root = in_feature_root(&compose.root, resources)?;
            if column_exists(root, col) {
                None
            } else {
                Some((format!("self.{col}"), root.name.clone()))
            }
        }
        ProjectionSource::Joined(alias, col) => {
            let join = compose.joins.iter().find(|j| &j.alias == alias)?;
            let target = joined_resource(compose, join, resources)?;
            if column_exists(target, col) {
                None
            } else {
                Some((format!("{alias}.{col}"), target.name.clone()))
            }
        }
        // Subselect-sourced projections are validated by COMPOSE-SUBSELECT-*.
        ProjectionSource::Subselect(_) => None,
    }
}

/// Resolve a join alias to the in-feature resource it lands on (walking its FK
/// path). `None` when the path's anchor isn't the root or a hop leaves the
/// feature.
fn joined_resource<'a>(
    compose: &ComposeQuery,
    join: &ComposeJoin,
    resources: &'a [Resource],
) -> Option<&'a Resource> {
    let root = in_feature_root(&compose.root, resources)?;
    resolve_fk_path_target(root, &join.path, resources)
}

/// The in-feature root resource for a compose, or `None` for a cross-feature /
/// undeclared root (column checks are then deferred).
fn in_feature_root<'a>(root: &TypeRef, resources: &'a [Resource]) -> Option<&'a Resource> {
    let TypeRef::UserDefined(qname) = root else {
        return None;
    };
    if qname.feature.is_some() {
        return None;
    }
    resources.iter().find(|r| r.name == qname.name)
}

/// Whether `col` is an authored field on `resource` or a framework-implicit
/// column.
fn column_exists(resource: &Resource, col: &str) -> bool {
    IMPLICIT_COLUMNS.contains(&col) || resource.fields.iter().any(|f| f.name == col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn joined_column_typo_fires() {
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
        host_name = h.nme
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "host_name");
        assert_eq!(findings[0].source_text, "h.nme");
        assert_eq!(findings[0].on_resource, "Host");
        assert_eq!(Finding::CODE, "COMPOSE-PROJECTION-SOURCE-001");
    }

    #[test]
    fn self_column_typo_fires() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
      title: Text required
    query.compose property_cards
      from Property
      join property.org as o
      select
        bad = self.ttle
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].source_text, "self.ttle");
        assert_eq!(findings[0].on_resource, "Property");
    }

    #[test]
    fn all_columns_resolving_does_not_fire() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource Property
      org: Org required
      host: Host required
      title: Text required
    resource Host
      org: Org required
      name: Text required
    query.compose property_cards
      from Property
      join property.host as h
      select
        property_id = self.id
        title = self.title
        host_name = h.name
"#,
        );

        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }
}
