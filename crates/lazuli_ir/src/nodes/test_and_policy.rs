//! Inline declarative tests + the feature-level policy registry.
//!
//! These two clusters live in one file because their authoring is
//! intertwined: the `tests` block on a command / rule / workflow
//! transition / extensible view directly references the policy
//! categories declared in the feature-level `policies` block.
//!
//! ## Why a declarative test grammar
//!
//! Lazuli treats authoring tests as a **shape declaration**, not a
//! mock framework. [`TestBlock::assertions`] is a verb list (the
//! [`TestAssertion`] enum is closed) that the analyzer projects
//! against the parent construct's policy matrix. The author writes
//! `tests permits @role.admin` once; codegen emits the Go-level
//! `TestXxx_permits_admin` case that exercises the actual policy
//! evaluation in the runtime — no DSL macro, no test scaffold to
//! maintain. The verbs are scoped: `allows extension` only on extensible
//! views, `allows from` only on workflow transitions, etc., and the
//! analyzer rejects misplaced verbs at lower time.
//!
//! ## Why the policy registry sits next to tests
//!
//! [`Policies`] is the feature-local registry where commands and queries
//! resolve their `policy @name` references. Two shapes coexist:
//!
//! - **Category** — atom list (`create: @role.admin, @scope.same_org`).
//!   Resolves through the registry catalog enforced by
//!   `lazuli_lsp::is_allowed_reference_namespace`.
//! - **Field policies** — per-resource read/write rules with
//!   field-level granularity. Used by codegen to gate column access
//!   in generated SQL and by inspect to render the security matrix.
//!
//! The route-only [`WhenDeniedRoute`] slot on [`PolicyCategory`] is the
//! one place where redirect targets bleed into policy land. Command/
//! query/api codegen ignores it; the route-guard codegen consumes it
//! when a view guard references the same policy. Splitting it into a
//! separate file would force readers to chase the redirect rule across
//! two files when authoring a single new policy.
//!
//! ## Closed catalogs
//!
//! - [`TestAssertion`] — every test verb. Adding a verb requires the
//!   analyzer to learn how to project it against the parent construct.
//! - [`RouteRedirectTarget`] — `view` vs `path`. The runtime resolves
//!   `view` against the surface's route map; `path` is a literal URL.
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` "Tests" — the verb catalog
//! - `docs/proposals/ir-error-messages-vocab.md` §3.2 — `when_denied`
//!   resolution chain
//! - [`crate::Predicate`] — the typed `when` expression that drives
//!   `AllowsWhen` / `DeniesWhen`

use serde::{Deserialize, Serialize};

use crate::{EvalPredicate, Predicate, QualifiedName, SpanRef, TranslationKeyRef};

/// Inline declarative assertions about IR shape. A `TestBlock` is the last
/// child of a command, workflow transition, rule, or extensible view. See
/// `docs/canonical-semantics.md` "Tests" for the verb catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestBlock {
    pub assertions: Vec<TestAssertion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of test verbs. The analyzer rejects assertions that do not
/// belong to the parent construct (e.g. `allows extension` on a command).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", content = "value")]
pub enum TestAssertion {
    /// Generated command policy matrix row: `permits @role.admin, @role.sales`.
    PolicyAllow { actors: Vec<String> },
    /// Generated command policy matrix row: `forbids @role.viewer`.
    PolicyDeny { actors: Vec<String> },
    /// Command/rule predicate: `allows when target.status = active` or
    /// `allows when self.status = active`, depending on the parent construct.
    AllowsWhen { predicate: Predicate },
    /// Command/rule predicate: `denies when target.deleted_at != nil` or
    /// `denies when self.deleted_at != nil`, depending on the parent construct.
    DeniesWhen { predicate: Predicate },
    /// Workflow transition state edge: `allows from active`.
    AllowsFrom { state: String },
    /// Workflow transition state edge: `denies from paused`.
    DeniesFrom { state: String },
    /// Workflow transition policy: `allows as @role.admin`.
    AllowsAs { actor: String },
    /// Workflow transition policy: `denies as @role.viewer`.
    DeniesAs { actor: String },
    /// Combined transition: `allows from active as @role.admin`.
    AllowsFromAs { state: String, actor: String },
    /// Combined transition: `denies from active as @role.sales`.
    DeniesFromAs { state: String, actor: String },
    /// Extensible view whitelist: `allows extension customer_tags`.
    AllowsExtension { feature: String },
    /// Extensible view whitelist: `denies extension billing`.
    DeniesExtension { feature: String },
}

/// Feature-level `policies` block. Categories are named atom lists; field
/// policies are per-resource read/write rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policies {
    pub categories: Vec<PolicyCategory>,
    pub fields: Vec<FieldPolicies>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Named feature-local policy: `create: @role.admin, @role.sales`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyCategory {
    pub name: String,
    /// Atom names like `@role.admin`, `@scope.same_org`, `@actor.system`. The
    /// analyzer validates that each atom resolves through the registry.
    ///
    /// Unconditional atoms only — atoms guarded by a `when <predicate>`
    /// clause live in [`Self::conditional_atoms`] instead (GAP-09). This
    /// keeps every existing reader of `atoms` (codegen, doctor, inspect)
    /// behaviour-identical while the new predicate-gated form rides on a
    /// strictly additive slot.
    pub atoms: Vec<String>,
    /// GAP-09 — input-value-predicate policy atoms. Each entry pairs an
    /// atom (`@policy.admin`, `@role.host`, ...) with a closed predicate
    /// over `input.*` fields of the consuming command. The atom only
    /// applies when the predicate holds; codegen emits a `When` guard on
    /// the runtime `PolicyAtom`, and `EvalPolicyInput` evaluates the
    /// predicate against the command input before counting the atom.
    /// Empty in the common (unconditional) case so legacy serde shape is
    /// unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional_atoms: Vec<ConditionalPolicyAtom>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// IR Error-Vocab — per-policy default error message. When `Some`,
    /// every command/query using this policy emits `policy_denied` with
    /// the resolved `TranslationKeyRef` unless the operation declares
    /// its own `policy_when_denied`. Resolution-chain step 2
    /// (proposal §2.E step 2). See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_denied: Option<TranslationKeyRef>,
    /// Route-only denial redirect targets. Command/query/api codegen ignores
    /// this slot; route guard codegen consumes it when a view guard references
    /// this policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_denied_route: Option<WhenDeniedRoute>,
}

/// GAP-09 — one input-value-predicate policy atom: `@policy.admin when
/// input.scope = "production"`. The `atom` is a normal policy atom
/// string (same vocabulary as [`PolicyCategory::atoms`]); the
/// `when` predicate is a closed predicate over the consuming command's
/// `input.*` fields. The atom is only enforced when the predicate holds
/// against the request input — when it does not hold the atom is
/// skipped (neither grant nor deny), so a category may carry several
/// mutually-exclusive predicate-gated atoms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalPolicyAtom {
    /// The guarded atom literal (`@policy.admin`, `@role.host`, ...).
    pub atom: String,
    /// Closed predicate over `input.*`. Lowered from the verbatim `when`
    /// text through the shared `parse_closed_predicate` entry point so it
    /// shares the `unique ... when` / `invariant when` predicate shape.
    pub when: EvalPredicate,
}

/// `when_denied` block on a policy — declarative redirect routing
/// when access is denied. Three arms: unauthenticated callers,
/// role-mismatch arms (per-role redirect target), and the default
/// fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhenDeniedRoute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unauthenticated: Option<RouteRedirectTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_mismatch: Vec<RoleMismatchArm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<RouteRedirectTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One role-keyed redirect arm inside [`WhenDeniedRoute`]. Codegen
/// emits a runtime guard that checks the caller's role and routes
/// to `target` on mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleMismatchArm {
    pub role: String,
    pub target: RouteRedirectTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of redirect targets in [`WhenDeniedRoute`].
/// `View` names a typed experience view; `Path` is the literal URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RouteRedirectTarget {
    /// Typed view reference (`view <name>`).
    View(String),
    /// Literal URL path (`"/login"`).
    Path(String),
}

/// Per-resource field policies: `fields Customer\n  email\n    read: ...`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicies {
    pub resource: QualifiedName,
    pub fields: Vec<FieldPolicy>,
}

/// Per-field read/write policy. `None` on either axis means inherit
/// the feature-level read/write policy. Lives inside [`FieldPolicies`]
/// per resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicy {
    pub field: String,
    /// Atom list governing reads. `None` = inherit feature-level read policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<Vec<String>>,
    /// Atom list governing writes. `None` = inherit feature-level write policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assertion_uses_verb_content_envelope() {
        let a = TestAssertion::PolicyAllow {
            actors: vec!["@role.admin".into()],
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"verb\":\"PolicyAllow\""));
        assert!(s.contains("\"actors\""));
        let back: TestAssertion = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn workflow_transition_combined_round_trips() {
        let a = TestAssertion::AllowsFromAs {
            state: "active".into(),
            actor: "@role.admin".into(),
        };
        let s = serde_json::to_string(&a).unwrap();
        let back: TestAssertion = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn route_redirect_target_snake_case() {
        let v = RouteRedirectTarget::View("dashboard".into());
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("\"kind\":\"view\""));
        assert!(s.contains("\"value\":\"dashboard\""));
        let back: RouteRedirectTarget = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn policies_default_is_empty_with_none_span() {
        let p = Policies::default();
        assert!(p.categories.is_empty());
        assert!(p.fields.is_empty());
        assert!(p.span_ref.is_none());
    }

    #[test]
    fn field_policy_skips_none_lists() {
        let fp = FieldPolicy {
            field: "email".into(),
            read: Some(vec!["@role.admin".into()]),
            write: None,
            previous_names: vec![],
        };
        let s = serde_json::to_string(&fp).unwrap();
        assert!(s.contains("\"read\""));
        assert!(!s.contains("\"write\""));
        let back: FieldPolicy = serde_json::from_str(&s).unwrap();
        assert_eq!(back, fp);
    }

    #[test]
    fn policy_category_optional_when_denied_round_trips() {
        let cat = PolicyCategory {
            name: "create".into(),
            atoms: vec!["@role.admin".into()],
            conditional_atoms: vec![],
            previous_names: vec![],
            when_denied: None,
            when_denied_route: None,
        };
        let s = serde_json::to_string(&cat).unwrap();
        assert!(!s.contains("when_denied"));
        // GAP-09 — empty conditional_atoms is omitted from the wire so the
        // legacy serde shape stays byte-identical.
        assert!(!s.contains("conditional_atoms"));
        let back: PolicyCategory = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cat);
    }

    // GAP-09 — a `ConditionalPolicyAtom` carrying the canonical closed
    // predicate (`input.scope = "production"`) round-trips through serde.
    // This exercises the adjacent-tagged `EvalPredicate` fix that lets the
    // `Closed(Predicate)` variant survive JSON (it could not under the
    // previous internal tagging).
    #[test]
    fn policy_category_conditional_atom_round_trips() {
        use crate::{CompareOp, Expr, Path, Predicate};
        let cat = PolicyCategory {
            name: "create".into(),
            atoms: vec![],
            conditional_atoms: vec![ConditionalPolicyAtom {
                atom: "@policy.admin".into(),
                when: EvalPredicate::Closed(Predicate::Comparison {
                    left: Expr::Path(Path::from_segments(["input", "scope"])),
                    op: CompareOp::Eq,
                    right: Expr::String("production".into()),
                }),
            }],
            previous_names: vec![],
            when_denied: None,
            when_denied_route: None,
        };
        let s = serde_json::to_string(&cat).unwrap();
        assert!(s.contains("conditional_atoms"));
        assert!(s.contains("@policy.admin"));
        let back: PolicyCategory = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cat);
    }

    // GAP-09 — the `Unparsed` fallback variant also round-trips (it could
    // not serialize at all under the previous internal tagging).
    #[test]
    fn policy_category_unparsed_predicate_atom_round_trips() {
        let cat = PolicyCategory {
            name: "create".into(),
            atoms: vec![],
            conditional_atoms: vec![ConditionalPolicyAtom {
                atom: "@policy.admin".into(),
                when: EvalPredicate::Unparsed("scope and weird".into()),
            }],
            previous_names: vec![],
            when_denied: None,
            when_denied_route: None,
        };
        let s = serde_json::to_string(&cat).unwrap();
        let back: PolicyCategory = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cat);
    }

    #[test]
    fn role_mismatch_arm_round_trip() {
        let arm = RoleMismatchArm {
            role: "viewer".into(),
            target: RouteRedirectTarget::Path("/forbidden".into()),
            span_ref: None,
        };
        let s = serde_json::to_string(&arm).unwrap();
        let back: RoleMismatchArm = serde_json::from_str(&s).unwrap();
        assert_eq!(back, arm);
    }

    #[test]
    fn test_block_skips_none_span() {
        let tb = TestBlock {
            assertions: vec![TestAssertion::AllowsExtension {
                feature: "billing".into(),
            }],
            span_ref: None,
        };
        let s = serde_json::to_string(&tb).unwrap();
        assert!(!s.contains("span_ref"));
        let back: TestBlock = serde_json::from_str(&s).unwrap();
        assert_eq!(back, tb);
    }
}
