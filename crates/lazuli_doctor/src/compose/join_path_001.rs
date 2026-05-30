//! COMPOSE-JOIN-PATH-001 — an FK-path JOIN hop doesn't resolve.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` `join <fk.path>` has a segment that is not a
//! declared FK relation on the prior resource, resolved against the **Module
//! graph** (every feature in the compilation, not just the declaring one).
//!
//! W2's analyzer lowering resolves the FK path against the *in-feature*
//! relation graph and surfaces `AnalyzeError::ComposeJoinPathUnresolved` for an
//! in-feature failure — but it deliberately **trusts** a cross-feature root or
//! a hop whose target leaves the feature ("cross-feature roots / hops leaving
//! the feature are trusted; doctor validates with Module context", per
//! `crates/lazuli_analyzer/src/query/compose.rs`). This W3 rule is that
//! deferred Module-context resolution: it re-walks each join path across ALL
//! features' resources and rejects a hop that resolves nowhere — closing the
//! "JOIN target is a string" gap (`get_property_dashboard.go:35` open-codes
//! `ON h.id = p.host`; a renamed FK breaks it silently).
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles — an unresolved FK
//! join is a concrete bug (typo / renamed relation), not style drift.
//!
//! ## Fixture example (fires)
//!
//! A `messaging` compose joins onto a cross-feature `Account.profilee` (typo
//! for `profile`); `profilee` is not an FK on the cross-feature `Account`:
//!
//! ```lzi
//! feature messaging
//!   uses accounts
//!   domain
//!     query.compose chat_inbox
//!       from Account
//!       join account.profilee as p
//!       select
//!         chat_id = self.id
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-JOIN-PATH-001`) + §3.2 #2 + §5.1 (FK-path resolution). Diagnostic
//! ID / code constant: `COMPOSE-JOIN-PATH-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{ComposeJoin, Feature, Resource, TypeRef};

use super::{composes_of, fk_target_of, pascal_to_snake};

/// One COMPOSE-JOIN-PATH-001 finding — an unresolved FK-path join segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The join path text (`account.profilee`).
    pub join_path: String,
    /// The unresolved segment (`profilee`).
    pub segment: String,
    /// The resource the segment was looked up on.
    pub on_resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-JOIN-PATH-001";

    /// Render the unresolved-join-path message.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} join `{}` — `{}` is not a declared FK relation on `{}`. A \
             `query.compose` JOIN is an FK path resolved against the relation graph (never an \
             authored ON-clause); a segment that names no relation is a renamed/typo'd FK.",
            self.query_name, self.join_path, self.segment, self.on_resource
        )
    }
}

/// Run COMPOSE-JOIN-PATH-001 over one feature using only that feature's own
/// resources. Thin convenience wrapper over [`check_in_module`] for callers
/// without the Module graph; cross-feature hops then conservatively defer.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::join_path_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("messaging.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    check_in_module(feature, std::slice::from_ref(feature), path)
}

/// Run COMPOSE-JOIN-PATH-001 over one feature with the **full Module graph**
/// (`all_features`) available — the cross-feature hops W2 trusts/defers.
///
/// Resolution walks each join's FK path across the union of every feature's
/// resources. A hop whose target leaves the known graph (an undeclared
/// resource even module-wide) still resolves its *next* segment lookup against
/// the resource it landed on; when a segment names no FK on the current
/// resource, it fires. A path whose anchor / root resource is unknown
/// module-wide is conservatively skipped (nothing to resolve against).
pub fn check_in_module(feature: &Feature, all_features: &[Feature], path: &Path) -> Vec<Finding> {
    let graph: Vec<&Resource> = all_features.iter().flat_map(|f| f.resources.iter()).collect();

    let mut findings = Vec::new();
    for compose in composes_of(feature) {
        let Some(root) = root_resource(&compose.root, &graph) else {
            continue;
        };
        for join in &compose.joins {
            if let Some((segment, on_resource)) = unresolved_segment(join, root, &graph) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    query_name: compose.name.clone(),
                    join_path: join.path.segments.join("."),
                    segment,
                    on_resource,
                });
            }
        }
    }
    findings
}

/// Resolve a compose root `TypeRef` to a resource somewhere in the module
/// graph (same- or cross-feature). `None` for a builtin / unknown root.
fn root_resource<'a>(root: &TypeRef, graph: &[&'a Resource]) -> Option<&'a Resource> {
    let TypeRef::UserDefined(qname) = root else {
        return None;
    };
    graph.iter().copied().find(|r| r.name == qname.name)
}

/// Walk a join's FK path across the module graph; return `Some((segment,
/// resource))` for the first unresolved hop, `None` when the whole path
/// resolves (or the anchor doesn't match the root → defer).
fn unresolved_segment(
    join: &ComposeJoin,
    root: &Resource,
    graph: &[&Resource],
) -> Option<(String, String)> {
    let (anchor, hops) = join.path.segments.split_first()?;
    // The path is anchored at the root's snake name; a path anchored elsewhere
    // is not this root's join to resolve.
    if *anchor != pascal_to_snake(&root.name) {
        return None;
    }
    let mut current = root;
    for hop in hops {
        let Some(target) = fk_target_of(current, hop) else {
            return Some((hop.clone(), current.name.clone()));
        };
        match graph.iter().copied().find(|r| r.name == target) {
            Some(next) => current = next,
            // The hop resolves to a relation whose target resource is unknown
            // even module-wide — the FK exists, the target table is just not in
            // this compilation. Trust the remainder (nothing to resolve against).
            None => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower_all(source: &str) -> Vec<Feature> {
        let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("parse features");
        skeletons
            .iter()
            .map(|s| lazuli_analyzer::lower_feature_skeleton(s).expect("lower feature"))
            .collect()
    }

    /// Cross-feature resolution: the join hop `account.profilee` is a typo;
    /// `Account` lives in a sibling feature and has a `profile` FK, not
    /// `profilee`. W2 trusted the cross-feature root; W3 (with the Module
    /// graph) resolves it and fires.
    #[test]
    fn cross_feature_unresolved_hop_fires() {
        let features = lower_all(
            r#"
feature accounts
  domain
    resource Account
      org: Org required
      profile: Profile required
    resource Profile
      org: Org required
      name: Text required

feature messaging
  uses accounts
  domain
    query.compose chat_inbox
      from Account
      join account.profilee as p
      select
        chat_id = self.id
"#,
        );
        let messaging = features.iter().find(|f| f.name == "messaging").unwrap();

        let findings = check_in_module(messaging, &features, Path::new("messaging.lzi"));
        assert_eq!(findings.len(), 1, "cross-feature unresolved hop must fire");
        assert_eq!(findings[0].segment, "profilee");
        assert_eq!(findings[0].on_resource, "Account");
        assert_eq!(Finding::CODE, "COMPOSE-JOIN-PATH-001");
    }

    /// The corrected cross-feature path (`account.profile`) resolves across the
    /// Module graph and stays silent.
    #[test]
    fn cross_feature_resolved_hop_does_not_fire() {
        let features = lower_all(
            r#"
feature accounts
  domain
    resource Account
      org: Org required
      profile: Profile required
    resource Profile
      org: Org required
      name: Text required

feature messaging
  uses accounts
  domain
    query.compose chat_inbox
      from Account
      join account.profile as p
      select
        chat_id = self.id
        name = p.name
"#,
        );
        let messaging = features.iter().find(|f| f.name == "messaging").unwrap();

        assert!(check_in_module(messaging, &features, Path::new("messaging.lzi")).is_empty());
    }

    /// In-feature happy path through the single-feature `check`.
    #[test]
    fn in_feature_resolved_join_does_not_fire() {
        let features = lower_all(
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

        assert!(check(&features[0], Path::new("catalog.lzi")).is_empty());
    }
}
