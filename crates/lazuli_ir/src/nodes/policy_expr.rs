//! Structured `policy <expr>` form used by command / query / job /
//! webhook / api / notification declarations.
//!
//! Coexists with the existing `PolicyRef` field for back-compat;
//! populated only when the authored policy text contained
//! `has_role` / `has_permission` / `authenticated` predicates or
//! boolean combinators (and / or / not).
//!
//! See `docs/proposals/rbac-catalog-vocab.md` §"Composition with the
//! existing `policy` block" for the dictionary-vs-predicate split:
//! `PolicyAtom` carries the dictionary-resolved atoms; `PolicyExpr`
//! carries the predicate composition.

use serde::{Deserialize, Serialize};

/// Atomic policy reference like `@scope.workspace_admin`, `@role.editor`,
/// `@actor.workspace_owner`. Parser populates `namespace` ("scope" |
/// "role" | "actor") + `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyAtom {
    pub namespace: String,
    pub name: String,
    /// Optional argument literal. Currently only
    /// `@mfa.required(within:<dur>)` populates this —
    /// `args == Some("within:15m")`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

/// RB.S6 — structured `policy <expr>` form used by command / query /
/// job / webhook / api / notification declarations. Coexists with the
/// existing `PolicyRef` field for back-compat; populated only when the
/// authored policy text contained `has_role` / `has_permission` /
/// `authenticated` predicates or boolean combinators.
///
/// See `docs/proposals/rbac-catalog-vocab.md` §"Composition with the
/// existing `policy` block" for the dictionary-vs-predicate split.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyExpr {
    /// `authenticated` — true when the actor is a logged-in user.
    Authenticated,
    /// `has_role <name>` — true when actor's role matches `name` (the
    /// catalog closure subsumes inheritance at codegen time).
    HasRole(String),
    /// `has_permission <resource>:<action>[:...]` — true when actor's
    /// role grants the permission via the catalog closure.
    HasPermission(String),
    /// `@<ns>.<name>` atom embedded in an expression.
    Atom(PolicyAtom),
    /// `<a> and <b>` — boolean conjunction (n-ary; collected from
    /// left-associative parse).
    And(Vec<PolicyExpr>),
    /// `<a> or <b>` — boolean disjunction (n-ary).
    Or(Vec<PolicyExpr>),
    /// `not <a>` — boolean negation.
    Not(Box<PolicyExpr>),
}
