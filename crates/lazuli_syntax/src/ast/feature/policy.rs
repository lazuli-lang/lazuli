//! Policy atoms, expressions, and the `policies` block surface AST.
//!
//! `PolicyAtomAst` and `PolicyExprAst` (RB.S6 structured policy form)
//! live here because they're consumed by *every* callable surface and
//! authoring them in the same file as the `policies` block keeps the
//! policy vocabulary co-located with its consumers (`PoliciesDecl` /
//! `PolicyCategoryDecl` / `FieldPolicyDecl`).
//!
//! The `policies` block lives at indent 2 under the feature header. Its
//! direct children are either:
//!
//!   * Named category atoms (indent 4): `create: @role.admin, @role.sales`.
//!   * Per-resource field overrides (indent 4): `fields <Resource>` with
//!     grandchild field names at indent 6 and `read:` / `write:` at indent 8.
//!
//! The IR shape (`ir::Policies` / `ir::PolicyCategory` / `ir::FieldPolicies`
//! / `ir::FieldPolicy`) is mirrored 1:1 so lowering is structural.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// `@<namespace>.<name>` — currently always `@scope.<x>` inside an
/// audience's `requires` block, but kept structured for future
/// `@role.x` / `@actor.x` expansion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyAtomAst {
    pub namespace: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    pub span: Span,
}

/// RB.S6 — structured `policy <expr>` form used by command / query / job /
/// webhook / api / notification / agent declarations. Coexists with the
/// raw `policy: Option<String>` field for back-compat; populated only when
/// the policy string parses as an expression (contains `has_role` /
/// `has_permission` / `authenticated` keywords or boolean combinators).
///
/// Closed shape: atoms (`@role.X`, `@scope.X`, `@actor.X`), the
/// `authenticated` keyword, `has_role <ident>`, `has_permission
/// <segment>:<segment>...`, plus `and` / `or` / `not` combinators with
/// optional parens. See `docs/proposals/rbac-catalog-vocab.md`
/// §Composition with `policy` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyExprAst {
    /// `authenticated` — true when ctx.User != nil.
    Authenticated,
    /// `has_role <name>` — true when actor's role is `<name>` or
    /// transitively inherits it.
    HasRole(String),
    /// `has_permission <resource>:<action>[:...]` — true when actor's
    /// role grants the permission via the catalog closure.
    HasPermission(String),
    /// `@<ns>.<name>` policy atom embedded in an expression.
    Atom(PolicyAtomAst),
    /// `<a> and <b>` — boolean conjunction (n-ary).
    And(Vec<PolicyExprAst>),
    /// `<a> or <b>` — boolean disjunction (n-ary).
    Or(Vec<PolicyExprAst>),
    /// `not <a>` — boolean negation.
    Not(Box<PolicyExprAst>),
}

/// Feature-scope `policies` block — closed-children container.
///
/// Holds named category atoms ([`PolicyCategoryDecl`]) and per-resource
/// field overrides ([`FieldPoliciesDecl`]). At most one `policies` block
/// per feature; doctor enforces that elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoliciesDecl {
    pub categories: Vec<PolicyCategoryDecl>,
    pub fields: Vec<FieldPoliciesDecl>,
    pub span: Span,
}

/// One named-category row inside a [`PoliciesDecl`] block, e.g.
/// `create: @role.admin, @role.sales`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyCategoryDecl {
    pub name: String,
    /// Verbatim atom literals (`@role.admin`, `@scope.same_org`, ...).
    /// Atoms not prefixed with `@` are dropped silently — matches the
    /// retired `collect_policy_atoms` walker.
    ///
    /// Unconditional atoms only; predicate-gated atoms
    /// (`@policy.admin when input.scope = "production"`) live in
    /// [`Self::conditional_atoms`] (GAP-09).
    pub atoms: Vec<String>,
    /// GAP-09 — input-value-predicate atoms. Each carries the atom literal
    /// plus the verbatim `when` predicate text (lowered through the shared
    /// `parse_closed_predicate` entry point analyzer-side). Empty in the
    /// common case so existing fixtures round-trip unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional_atoms: Vec<PolicyConditionalAtomAst>,
    /// IR Error-Vocab (Cell PARSE-1) — optional `when_denied
    /// @translation.<key>` child at indent 6 declaring the per-policy
    /// default message for `policy_denied`. Lowered into
    /// `ir::PolicyCategory.when_denied`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §2.B.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_denied: Option<TranslationKeyRefAst>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_denied_route: Option<WhenDeniedRouteAst>,
    pub span: Span,
}

/// GAP-09 — one input-value-predicate atom inside a
/// [`PolicyCategoryDecl`]: `@policy.admin when input.scope = "production"`.
/// `atom` is the verbatim atom literal; `when` is the raw predicate text
/// after the `when` keyword, lowered analyzer-side via the shared
/// `parse_closed_predicate` entry point (same machinery as
/// `unique ... when` / `invariant when`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyConditionalAtomAst {
    /// Verbatim atom literal (`@policy.admin`, `@role.host`, ...).
    pub atom: String,
    /// Raw predicate text after `when` (`input.scope = "production"`).
    pub when: String,
    pub span: Span,
}

/// Route-redirect sidecar on a [`PolicyCategoryDecl`] — drives the
/// frontend `when_denied route { ... }` block. Decides where the UI
/// sends an actor who fails the policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhenDeniedRouteAst {
    /// `unauthenticated -> <target>` — actor has no session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unauthenticated: Option<RouteRedirectTargetAst>,
    /// `role_mismatch <role> -> <target>` arms, in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_mismatch: Vec<RoleMismatchArmAst>,
    /// `default -> <target>` — fallback when no other arm matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<RouteRedirectTargetAst>,
    pub span: Span,
}

/// One `role_mismatch <role> -> <target>` arm inside [`WhenDeniedRouteAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleMismatchArmAst {
    /// Role name (sans `@role.` prefix).
    pub role: String,
    pub target: RouteRedirectTargetAst,
    pub span: Span,
}

/// Closed-catalog target of a `when_denied` redirect arm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RouteRedirectTargetAst {
    /// `route view <ViewName>` — declared surface view.
    View(String),
    /// `route path "/login"` — raw path string.
    Path(String),
}

/// IR Error-Vocab (Cell PARSE-1) — surface AST mirror of
/// `ir::TranslationKeyRef`. Carries the key name parsed from
/// `@translation.<key>` plus the source span for downstream doctor
/// diagnostics (ERR-VOCAB-002, `translation_key_unknown`).
///
/// See `docs/proposals/ir-error-messages-vocab.md` §3.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKeyRefAst {
    /// The bare key name extracted from `@translation.<key>`.
    pub key: String,
    pub span: Span,
}

/// Per-resource field-policy override block inside a [`PoliciesDecl`].
/// Authored as `fields <Resource>` with one [`FieldPolicyDecl`] per
/// named field below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPoliciesDecl {
    /// `fields <Resource>` — captured verbatim (qualifier-free identifier
    /// in the fixture).
    pub resource: String,
    pub fields: Vec<FieldPolicyDecl>,
    pub span: Span,
}

/// One field override row inside a [`FieldPoliciesDecl`].
///
/// `read` / `write` are independent allow-lists of policy atoms; either
/// can be absent (inherit the category default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicyDecl {
    pub field: String,
    /// `read: @role.x, @scope.y` allow-list, if authored.
    pub read: Option<Vec<String>>,
    /// `write: @role.x, @scope.y` allow-list, if authored.
    pub write: Option<Vec<String>>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_expr_atom_serde_token_tagged() {
        let e = PolicyExprAst::Authenticated;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "Authenticated");
    }

    #[test]
    fn route_redirect_target_view_serde_snake_case() {
        let r = RouteRedirectTargetAst::View("Login".into());
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["kind"], "view");
        assert_eq!(v["value"], "Login");
    }

    #[test]
    fn field_policy_decl_optional_allowlists() {
        let p = FieldPolicyDecl {
            field: "email".into(),
            read: Some(vec!["@role.admin".into()]),
            write: None,
            span: Span::new(0, 0),
        };
        assert!(p.write.is_none());
        assert_eq!(p.read.as_ref().unwrap()[0], "@role.admin");
    }
}
