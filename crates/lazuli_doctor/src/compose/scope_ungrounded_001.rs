//! COMPOSE-SCOPE-UNGROUNDED-001 — composite read drops its tenant scope.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` rooted at a **tenant-bearing** resource
//! (`tenancy org` / `tenancy team`) has its inherited tenant + soft-delete
//! scope **overridden** (`scope_origin == Overridden`, i.e. the author wrote
//! `scope override`) WITHOUT an accompanying `policy`. A composite read that
//! suppresses the generated tenant predicate and is not gated by an explicit
//! policy is a cross-tenant leak: every row of every tenant becomes reachable.
//!
//! This is the moat-completing security code. The whole thesis of
//! `query.compose` is that the tenant predicate is GENERATED from `tenancy`
//! (so it cannot be forgotten the way a hand-written `WHERE org_id = $N`
//! can — see `list_chat_inbox.go:43` in the audit). The one way to lose that
//! guarantee is `scope override`; this rule makes an *ungrounded* override
//! (overridden tenant scope on a tenant resource with no policy gate) a
//! build-time error, mirroring `invariants.md:410-414`.
//!
//! ## How it reads `scope_origin`
//!
//! The analyzer (W2) stamps [`lazuli_ir::ComposeScopeOrigin`] at lowering:
//! `Inherited` (the secure default — codegen injects the tenant + soft-delete
//! predicates) or `Overridden` (the author opted out with `scope override`).
//! This rule reads that resolved fact directly — it never re-derives scope
//! from text. An `Overridden` origin on a tenant-bearing root, with
//! `policy == PolicyRef::None` and no structured `policy_expr`, fires.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles — a dropped tenant
//! predicate is a correctness/security failure, never style drift.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! feature messaging
//!   domain
//!     resource Chat
//!       org: Org required
//!     query.compose all_chats
//!       from Chat
//!       select
//!         chat_id = self.id
//!       scope override
//! ```
//!
//! Canonical fix — gate the override with an explicit policy (and a reason):
//!
//! ```lzi
//! query.compose all_chats
//!   from Chat
//!   select
//!     chat_id = self.id
//!   scope override
//!     reason "admin cross-tenant audit console"
//!   policy @policy.admin
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-SCOPE-UNGROUNDED-001`) + §3.2 #5 + §4.4 + §5.3. Diagnostic ID /
//! code constant: `COMPOSE-SCOPE-UNGROUNDED-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{ComposeQuery, ComposeScopeOrigin, Feature, PolicyRef, Tenancy, TypeRef};

use super::composes_of;

/// One COMPOSE-SCOPE-UNGROUNDED-001 finding — a tenant-bearing composite read
/// overrode its inherited tenant scope with no policy gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The tenant-bearing root resource whose scope was dropped.
    pub root: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-SCOPE-UNGROUNDED-001";

    /// Render the cross-tenant-leak message and prompt the author to either
    /// drop the `scope override` (inherit tenancy) or gate it behind a policy.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} overrides the inherited tenant scope on tenant-bearing root `{}` \
             with no policy. The generated tenant + soft-delete predicate is suppressed, so the \
             read can return rows from every tenant (a cross-tenant leak). Remove `scope override` \
             to inherit tenancy, or gate the override with `policy @policy.<name>` plus a \
             `reason`.",
            self.query_name, self.root
        )
    }
}

/// Run COMPOSE-SCOPE-UNGROUNDED-001 over one feature.
///
/// Resolves each compose's root against the feature's own resources to learn
/// whether the root is tenant-bearing. Cross-feature roots (carrying a
/// `feature` segment, or not declared in-feature) are conservatively skipped —
/// the override-without-policy half still fires only when tenancy is *known*
/// tenant-bearing, so the rule never false-positives on an intentionally
/// tenant-`none` resource.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::scope_ungrounded_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("messaging.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    composes_of(feature)
        .filter(|compose| is_ungrounded(compose, feature))
        .map(|compose| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            query_name: compose.name.clone(),
            root: root_name(&compose.root),
        })
        .collect()
}

/// The leak predicate: an `Overridden` scope origin, on a tenant-bearing root,
/// with no `policy` (neither legacy atom nor structured expr) to gate it.
fn is_ungrounded(compose: &ComposeQuery, feature: &Feature) -> bool {
    if compose.scope_origin != ComposeScopeOrigin::Overridden {
        return false;
    }
    if !root_is_tenant_bearing(&compose.root, feature) {
        return false;
    }
    matches!(compose.policy, PolicyRef::None) && compose.policy_expr.is_none()
}

/// Whether the compose's root resolves to an in-feature resource whose
/// **effective** tenancy axis is `org`/`team` (tenant-bearing). Effective
/// tenancy mirrors codegen's `effective_tenancy`: the resource's own
/// `tenancy` axis when declared, else the feature `defaults.tenancy` it
/// inherits — so a resource under `defaults tenancy org` (the common case,
/// where individual resources rarely redeclare the axis) is correctly seen
/// as tenant-bearing. A root that bottoms out at `tenancy none`, an
/// unresolved tenancy, or a cross-feature/undeclared root returns `false` —
/// the rule only fires when the dropped scope is *provably* a tenant scope.
fn root_is_tenant_bearing(root: &TypeRef, feature: &Feature) -> bool {
    let TypeRef::UserDefined(qname) = root else {
        return false;
    };
    if qname.feature.is_some() {
        return false;
    }
    let Some(resource) = feature.resources.iter().find(|r| r.name == qname.name) else {
        return false;
    };
    let effective = resource
        .tenancy
        .clone()
        .or_else(|| feature.defaults.tenancy.clone());
    matches!(effective, Some(Tenancy::Org) | Some(Tenancy::Team))
}

/// Display name for the root [`TypeRef`] — the resource name for a
/// `UserDefined` root, a best-effort label otherwise (never panics).
fn root_name(root: &TypeRef) -> String {
    match root {
        TypeRef::UserDefined(qname) => qname.name.clone(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    /// THE anti-theater proof: a `scope override` on a tenant-bearing `Chat`
    /// with no policy is a real cross-tenant leak, and it fires at build time.
    #[test]
    fn tenant_override_without_policy_fires() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      tenancy org
      org: Org required
    query.compose all_chats
      from Chat
      select
        chat_id = self.id
      scope override
"#,
        );

        let findings = check(&feature, Path::new("messaging.lzi"));
        assert_eq!(findings.len(), 1, "ungrounded tenant override must fire");
        assert_eq!(findings[0].query_name, "all_chats");
        assert_eq!(findings[0].root, "Chat");
        assert_eq!(Finding::CODE, "COMPOSE-SCOPE-UNGROUNDED-001");
        assert!(findings[0].message().contains("cross-tenant leak"));
    }

    /// The common case: tenancy is declared at the feature `defaults` level
    /// (not redeclared on the resource), exactly like the canonical
    /// `examples/full-capsule` `customer` feature and real Hostpoint reads. A
    /// `scope override` with no policy on such a root is STILL a cross-tenant
    /// leak — the rule must read EFFECTIVE tenancy (resource axis OR feature
    /// default), not only the resource's own axis.
    #[test]
    fn defaults_level_tenant_override_without_policy_fires() {
        let feature = lower(
            r#"
feature customer
  defaults
    tenancy org
  domain
    resource Customer
      org: Org required
      name: Text required
    query.compose all_customers
      from Customer
      select
        customer_id = self.id
      scope override
"#,
        );

        let findings = check(&feature, Path::new("customer.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "an override on a defaults-tenant-bearing root must fire (effective tenancy)"
        );
        assert_eq!(findings[0].query_name, "all_customers");
        assert_eq!(findings[0].root, "Customer");
    }

    /// A `scope override` gated by an explicit policy (+ reason) is the
    /// blessed admin escape hatch — it must stay silent.
    #[test]
    fn override_with_policy_does_not_fire() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      tenancy org
      org: Org required
    query.compose all_chats
      from Chat
      select
        chat_id = self.id
      scope override
        reason "admin cross-tenant audit console"
      policy @policy.admin
"#,
        );

        assert!(
            check(&feature, Path::new("messaging.lzi")).is_empty(),
            "policy-gated override is the blessed hatch, must be silent"
        );
    }

    /// The secure default — a compose that INHERITS tenant scope
    /// (`scope_origin == Inherited`) never fires, even with no policy.
    #[test]
    fn inherited_scope_does_not_fire() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
    query.compose chat_inbox
      from Chat
      select
        chat_id = self.id
      scope
        participants has ctx.user.id
"#,
        );

        assert!(
            check(&feature, Path::new("messaging.lzi")).is_empty(),
            "inherited (generated) tenant scope is the secure default"
        );
    }

    /// An override on a NON-tenant resource (`tenancy none`) is not a tenant
    /// leak — there is no tenant scope to drop, so the rule stays silent.
    #[test]
    fn override_on_non_tenant_root_does_not_fire() {
        let feature = lower(
            r#"
feature catalog
  domain
    resource PublicListing
      tenancy none
      title: Text required
    query.compose all_listings
      from PublicListing
      select
        listing_id = self.id
      scope override
"#,
        );

        assert!(
            check(&feature, Path::new("catalog.lzi")).is_empty(),
            "non-tenant root has no tenant scope to leak"
        );
    }
}
