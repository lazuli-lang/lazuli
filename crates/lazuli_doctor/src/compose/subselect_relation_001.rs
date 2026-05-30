//! COMPOSE-SUBSELECT-RELATION-001 — subselect correlation FK is wrong/missing.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` subselect's `related_by <fk.path>` does not
//! correlate its child resource back to the compose root:
//!
//! - a hop in the path names no FK field on the child resource (a wrong FK —
//!   e.g. `related_by chat_message.chatt`), OR
//! - the path resolves but lands on a resource that is NOT the compose root
//!   (the correlation points at the wrong table).
//!
//! W2 already rejects a *missing* `related_by` cleanly
//! (`AnalyzeError::ComposeSubselectMissingRelation`); this rule is the W3
//! backstop for a *present-but-wrong* correlation — the exact bug a
//! hand-written `WHERE child.fk = root.id` makes when the FK column is wrong
//! (silent empty / cartesian result; see `list_chat_inbox.go` in the audit).
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles — a wrong
//! correlation silently changes the result set, a concrete correctness bug.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! query.compose chat_inbox
//!   from Chat
//!   subselect unread = count ChatMessage
//!     related_by chat_message.bogus    # `bogus` is not an FK on ChatMessage
//!   select
//!     chat_id = self.id
//!     unread = unread
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-SUBSELECT-RELATION-001`) + §3.2 #3. Diagnostic ID / code constant:
//! `COMPOSE-SUBSELECT-RELATION-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{ComposeSubselect, Feature, Resource, SubselectKind, TypeRef};

use super::{composes_of, fk_target_of, pascal_to_snake, resource_by_name};

/// One COMPOSE-SUBSELECT-RELATION-001 finding — a subselect whose `related_by`
/// correlation FK is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The subselect whose correlation is wrong.
    pub subselect: String,
    /// The `related_by` path text.
    pub related_by: String,
    /// Human reason — `"unresolved FK segment `bogus`"` or `"correlates to
    /// `Other`, not root `Chat`"`.
    pub reason: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-SUBSELECT-RELATION-001";

    /// Render the wrong-correlation message.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} subselect `{}` `related_by {}` does not correlate to the compose \
             root: {}. The correlation FK must walk the child resource back to root, or the \
             sub-select returns a silently wrong (empty / cartesian) result.",
            self.query_name, self.subselect, self.related_by, self.reason
        )
    }
}

/// Run COMPOSE-SUBSELECT-RELATION-001 over one feature.
///
/// Correlation is enforced only when the child resource AND the root are
/// in-feature; cross-feature children defer to the Module-graph pass.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::subselect_relation_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("messaging.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for compose in composes_of(feature) {
        let Some(root_name) = in_feature_root_name(&compose.root) else {
            continue;
        };
        for sub in &compose.subselects {
            if let Some(reason) =
                correlation_problem(sub, &root_name, &feature.resources)
            {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    query_name: compose.name.clone(),
                    subselect: sub.name.clone(),
                    related_by: sub.related_by.segments.join("."),
                    reason,
                });
            }
        }
    }
    findings
}

/// Describe how a subselect's `related_by` fails to correlate to `root_name`,
/// or `None` when it correlates (or can't be resolved in-feature → defer).
fn correlation_problem(
    sub: &ComposeSubselect,
    root_name: &str,
    resources: &[Resource],
) -> Option<String> {
    let child_name = subselect_resource_name(&sub.kind)?;
    // Child resource must be in-feature to walk the FK path; otherwise defer.
    let child = resource_by_name(resources, &child_name)?;

    let (anchor, hops) = sub.related_by.segments.split_first()?;
    // The path must be anchored at the child resource (snake name). A path
    // anchored elsewhere is something the Module-graph pass owns; defer.
    if *anchor != pascal_to_snake(&child.name) {
        return None;
    }
    // An empty hop list means `related_by <child>` with no FK column — it
    // cannot reach root.
    if hops.is_empty() {
        return Some(format!(
            "path names no correlation FK on `{}`",
            child.name
        ));
    }

    let mut current = child;
    for hop in hops {
        let Some(target) = fk_target_of(current, hop) else {
            return Some(format!("unresolved FK segment `{hop}` on `{}`", current.name));
        };
        match resource_by_name(resources, &target) {
            Some(next) => current = next,
            // Hop leaves the feature; can't verify the landing here — defer.
            None => return None,
        }
    }

    if current.name == root_name {
        None
    } else {
        Some(format!(
            "correlates to `{}`, not root `{root_name}`",
            current.name
        ))
    }
}

/// The child resource name a subselect kind targets.
fn subselect_resource_name(kind: &SubselectKind) -> Option<String> {
    let type_ref = match kind {
        SubselectKind::Count(r) => r,
        SubselectKind::Exists { resource, .. } => resource,
        SubselectKind::Latest { resource, .. } => resource,
        SubselectKind::Aggregate { resource, .. } => resource,
    };
    type_ref_name(type_ref)
}

/// The in-feature root name for a compose root `TypeRef`, or `None` for
/// cross-feature / builtin roots.
fn in_feature_root_name(root: &TypeRef) -> Option<String> {
    let TypeRef::UserDefined(qname) = root else {
        return None;
    };
    if qname.feature.is_some() {
        return None;
    }
    Some(qname.name.clone())
}

/// The bare resource name of a `UserDefined` type ref, else `None`.
fn type_ref_name(type_ref: &TypeRef) -> Option<String> {
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
    fn wrong_fk_segment_fires() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
    resource ChatMessage
      chat: Chat required
      body: Text required
    query.compose chat_inbox
      from Chat
      subselect unread = count ChatMessage
        related_by chat_message.bogus
      select
        chat_id = self.id
        unread = unread
"#,
        );

        let findings = check(&feature, Path::new("messaging.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].subselect, "unread");
        assert!(findings[0].reason.contains("unresolved FK segment"));
        assert_eq!(Finding::CODE, "COMPOSE-SUBSELECT-RELATION-001");
    }

    #[test]
    fn correlation_to_wrong_resource_fires() {
        // `related_by review.author` lands on `User`, not the compose root
        // `ServiceTransaction` — a wrong correlation.
        let feature = lower(
            r#"
feature trust
  domain
    resource ServiceTransaction
      org: Org required
    resource User
      org: Org required
    resource Review
      transaction: ServiceTransaction required
      author: User required
    query.compose pending
      from ServiceTransaction
      subselect already = exists Review
        related_by review.author
      select
        transaction_id = self.id
        already = already
"#,
        );

        let findings = check(&feature, Path::new("trust.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].reason.contains("not root"));
    }

    #[test]
    fn correct_correlation_does_not_fire() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
    resource ChatMessage
      chat: Chat required
      body: Text required
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
