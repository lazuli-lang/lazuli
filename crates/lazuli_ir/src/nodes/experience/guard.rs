//! View guard IR — the four-layer resolution chain for route policies and
//! redirects (view → audience → app → built-in framework default).
//!
//! See `docs/proposals/ir-route-guards.md` §3 for the design. The IR carries
//! the *authored* shape only; the analyzer resolves the chain and writes
//! the final policy onto `resolved_guard_policy` slots on views.
//!
//! ## Catalog
//!
//! - [`ViewGuard`] — `policy` + `on_unauthenticated` + `on_unauthorized` +
//!   `forbid_when` + `requires_lifecycle` + `on_lifecycle_pending`. The
//!   same shape sits on `ExperienceView`, `PlatformView`, `AppRoute`, and
//!   `AudienceSurface`.
//! - [`ForbidWhen`] — `forbid_when <atom> dispatch_to "<url>"`.
//! - [`RouteGuardDefaults`] — app-level catch-all defaults block.

use serde::{Deserialize, Serialize};

use crate::PolicyAtom;
use crate::SpanRef;
use crate::nodes::experience::lifecycle_gate::RequiresLifecycle;

/// `ir-route-guards` §3.1 — declarative policy + redirect targets for a
/// view (experience view, platform view, audience, or app route). The
/// same `PolicyRef` shape already used by `command.policy`; redirects
/// are local-path string slots.
///
/// See `docs/proposals/ir-route-guards.md` §3.1, §2.A.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewGuard {
    /// Audience policies admitted — OR-semantics. Single-policy form
    /// `policy @policy.X` parses to `vec!["@policy.X"]`; list form
    /// `policy [@policy.A, @policy.B]` parses to vec with both atoms.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy: Vec<String>,
    /// `on_unauthenticated redirect "<path>"` — where to send a user
    /// who is not signed in. When `None`, runtime resolves up the chain
    /// to the audience / app default (§2.D).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_unauthenticated: Option<String>,
    /// `on_unauthorized redirect "<path>"` — where to send a signed-in
    /// user who fails the policy check. When `None`, runtime resolves
    /// up the chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_unauthorized: Option<String>,
    /// `requires_lifecycle <Resource> = <state>` — lifecycle state
    /// required before the view may render.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_lifecycle: Option<RequiresLifecycle>,
    /// `on_lifecycle_pending @resume <name>` — resume router used when
    /// the lifecycle gate does not match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_lifecycle_pending: Option<String>,
    /// router-w3 Tier 3 — positive-state redirects. Each entry pairs
    /// a policy atom (`@role.host`, `@scope.X`, etc.) with a URL.
    /// When the actor satisfies the atom, codegen throws a redirect
    /// BEFORE running the main policy gate. Use case: a "choose role"
    /// route that should never paint for users who already chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbid_when: Vec<ForbidWhen>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// router-w3 Tier 3 — `forbid_when <atom> dispatch_to "<url>"` slot
/// under a route's `policy` block. Codegen emits an
/// `evaluatePolicy(actor, atom)` check BEFORE the main guard; if the
/// actor satisfies the atom, redirect to `dispatch_to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForbidWhen {
    /// The policy atom as authored, e.g. `@role.host`.
    pub atom_ref: String,
    /// Resolved atom (namespace + name) used by codegen to emit a
    /// `LazuliRouteGuardPolicy` literal.
    pub atom: PolicyAtom,
    /// URL the actor is redirected to when the atom matches.
    pub dispatch_to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `ir-route-guards` §3.6 — app-level defaults for the resolution
/// chain's tail layer. Authored under an `app.route_guard` block; each
/// inner slot is independently optional (when absent, the built-in
/// framework defaults apply at runtime).
///
/// See `docs/proposals/ir-route-guards.md` §3.6, §2.C.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteGuardDefaults {
    /// Catch-all policy reference applied to any route that does not
    /// declare its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_policy: Option<String>,
    /// Catch-all redirect for unauthenticated users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_unauthenticated: Option<String>,
    /// Catch-all redirect for unauthorized users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_unauthorized: Option<String>,
    /// `skeleton @client.<name>` — block reference rendered while the
    /// actor query is hydrating. When `None`, runtime renders nothing
    /// (blank background) until the verdict is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_guard_empty_serialises_as_empty_object() {
        let v = ViewGuard {
            policy: vec![],
            on_unauthenticated: None,
            on_unauthorized: None,
            requires_lifecycle: None,
            on_lifecycle_pending: None,
            forbid_when: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{}");
    }

    #[test]
    fn view_guard_round_trips_with_policy_list() {
        let v = ViewGuard {
            policy: vec!["@policy.member".into()],
            on_unauthenticated: Some("/login".into()),
            on_unauthorized: None,
            requires_lifecycle: None,
            on_lifecycle_pending: None,
            forbid_when: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ViewGuard = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn forbid_when_round_trips() {
        let v = ForbidWhen {
            atom_ref: "@role.host".into(),
            atom: PolicyAtom {
                namespace: "role".into(),
                name: "host".into(),
                args: None,
            },
            dispatch_to: "/host".into(),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ForbidWhen = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn route_guard_defaults_omits_unset_slots() {
        let v = RouteGuardDefaults {
            default_policy: Some("@policy.authenticated".into()),
            on_unauthenticated: None,
            on_unauthorized: None,
            skeleton: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("default_policy"));
        assert!(!s.contains("skeleton"));
    }
}
