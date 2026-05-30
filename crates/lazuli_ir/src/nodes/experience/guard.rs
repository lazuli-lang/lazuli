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
//!   `forbid_when` + `requires_lifecycle` + `requires_lifecycle_in` +
//!   `requires_field` + `on_lifecycle_pending`. The same shape sits on
//!   `ExperienceView`, `PlatformView`, `AppRoute`, and `AudienceSurface`.
//! - [`ForbidWhen`] — `forbid_when <atom> dispatch_to "<url>" [only_when
//!   lifecycle <R> = <state>]`.
//! - [`RequiresLifecycleIn`] — `requires_lifecycle_in <Resource> [s1, …]`
//!   allow-list variant (per `ir-route-guard-escape-hatch-2026-05-28.md`
//!   §4.2).
//! - [`RequiresField`] — `requires <feature>.lookup_my.<field> = <lit>
//!   on_unmet redirect "<path>"` row-field predicate gate.
//! - [`RouteGuardDefaults`] — app-level catch-all defaults block.

use serde::{Deserialize, Serialize};

use crate::nodes::experience::lifecycle_gate::RequiresLifecycle;
use crate::{DefaultValue, PolicyAtom, SpanRef};

/// `ir-route-guards` §3.1 — declarative policy + redirect targets for a
/// view (experience view, platform view, audience, or app route). The
/// same `PolicyRef` shape already used by `command.policy`; redirects
/// are local-path string slots.
///
/// See `docs/proposals/ir-route-guards.md` §3.1, §2.A.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    /// `ir-route-guard-escape-hatch-2026-05-28` §4.2 — allow-list
    /// variant of `requires_lifecycle`. Mutually exclusive with the
    /// exact-match form (doctor rule
    /// `ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001`). When set, the view
    /// paints if and only if the resolved state is in
    /// [`RequiresLifecycleIn::allowed_states`]; otherwise dispatch via
    /// the resource's `lifecycle_routes` helper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_lifecycle_in: Option<RequiresLifecycleIn>,
    /// `ir-route-guard-escape-hatch-2026-05-28` §4.2 — row-field
    /// predicate gates. Each entry resolves a `lookup_my_<resource>`
    /// query, reads the named field, and redirects when it does not
    /// equal the expected literal. Evaluated AFTER the lifecycle gate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires_field: Vec<RequiresField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `requires_lifecycle_in <Resource> [<state>, ...]` allow-list slot.
///
/// Per `ir-route-guard-escape-hatch-2026-05-28.md` §4.2. Same
/// `<Resource>` resolution as `requires_lifecycle`; lowering folds
/// the allowed states into the same `ResolvedLifecycleGate` cache.
/// Doctor codes `ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001`,
/// `ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002`, and
/// `ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003` enforce the invariants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiresLifecycleIn {
    /// PascalCase resource name matching `Resource.name`.
    pub resource: String,
    /// snake_case lifecycle state names. Ordering is the authored
    /// order; codegen typically lowers into a `BTreeSet` for stable
    /// emit, but the IR preserves source order for diagnostic
    /// fidelity (single-state shorthand authored as `[<s>]` keeps
    /// its position).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_states: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `requires <feature>.lookup_my.<field> = <literal> on_unmet redirect
/// "<path>"` row-field predicate slot.
///
/// Per `ir-route-guard-escape-hatch-2026-05-28.md` §4.2. Resolves a
/// `lookup_my_<resource>` query on the named feature, reads
/// [`Self::field`], and redirects to [`Self::on_unmet_redirect`] when
/// the value does not equal [`Self::expected`]. Doctor codes
/// `ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004` through
/// `ROUTE-GUARD-FIELD-TYPE-MISMATCH-006` plus
/// `ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001` enforce invariants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiresField {
    /// snake_case feature identifier (the LHS of `<feature>.lookup_my.<field>`).
    pub feature: String,
    /// Field identifier inside the `lookup_my_<resource>` row.
    pub field: String,
    /// Literal the field is compared against. Reuses the IR's
    /// [`crate::DefaultValue`] enum so the wire shape stays in one
    /// place.
    pub expected: DefaultValue,
    /// Redirect target when the predicate does not match.
    pub on_unmet_redirect: String,
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
    /// `ir-route-guard-escape-hatch-2026-05-28` §4.2 — optional
    /// `only_when lifecycle <R> = <state>` sub-slot. Composes the
    /// atom check with a lifecycle predicate so the dispatch fires
    /// only when BOTH the atom is satisfied AND the authored
    /// lifecycle state matches. Without this, `forbid_when` fires on
    /// atom match alone — the rigid form that produced the
    /// 2026-05-28 choose_role loop bug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_when_lifecycle: Option<RequiresLifecycle>,
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
        let v = ViewGuard::default();
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{}");
    }

    #[test]
    fn view_guard_round_trips_with_policy_list() {
        let v = ViewGuard {
            policy: vec!["@policy.member".into()],
            on_unauthenticated: Some("/login".into()),
            ..ViewGuard::default()
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
            only_when_lifecycle: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ForbidWhen = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn forbid_when_round_trips_with_only_when_lifecycle() {
        let v = ForbidWhen {
            atom_ref: "@role.host".into(),
            atom: PolicyAtom {
                namespace: "role".into(),
                name: "host".into(),
                args: None,
            },
            dispatch_to: "/host".into(),
            only_when_lifecycle: Some(crate::RequiresLifecycle {
                resource: "Host".into(),
                state: "complete".into(),
                substep: None,
                span_ref: None,
            }),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("only_when_lifecycle"));
        let back: ForbidWhen = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn requires_lifecycle_in_round_trips() {
        let v = RequiresLifecycleIn {
            resource: "Host".into(),
            allowed_states: vec!["basic_details_pending".into(), "address_pending".into()],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("allowed_states"));
        let back: RequiresLifecycleIn = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn requires_field_round_trips() {
        let v = RequiresField {
            feature: "user".into(),
            field: "is_phone_verified".into(),
            expected: DefaultValue::Boolean(true),
            on_unmet_redirect: "/onboarding/host/phone-verification".into(),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("is_phone_verified"));
        assert!(s.contains("on_unmet_redirect"));
        let back: RequiresField = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn view_guard_round_trips_with_new_slots() {
        let v = ViewGuard {
            policy: vec!["@policy.authenticated".into()],
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Host".into(),
                allowed_states: vec!["basic_details_pending".into()],
                span_ref: None,
            }),
            requires_field: vec![RequiresField {
                feature: "user".into(),
                field: "is_phone_verified".into(),
                expected: DefaultValue::Boolean(true),
                on_unmet_redirect: "/onboarding/host/phone-verification".into(),
                span_ref: None,
            }],
            ..ViewGuard::default()
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("requires_lifecycle_in"));
        assert!(s.contains("requires_field"));
        let back: ViewGuard = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn view_guard_back_compat_legacy_json_without_new_slots() {
        // Pre-this-cell fixtures lack the three new fields; ensure they
        // still deserialize cleanly with defaults.
        let legacy = serde_json::json!({
            "policy": ["@policy.member"],
            "on_unauthenticated": "/login"
        });
        let parsed: ViewGuard = serde_json::from_value(legacy).expect("deserialize legacy guard");
        assert_eq!(parsed.policy, vec!["@policy.member"]);
        assert_eq!(parsed.on_unauthenticated.as_deref(), Some("/login"));
        assert!(parsed.requires_lifecycle_in.is_none());
        assert!(parsed.requires_field.is_empty());
        for fw in &parsed.forbid_when {
            assert!(fw.only_when_lifecycle.is_none());
        }
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
