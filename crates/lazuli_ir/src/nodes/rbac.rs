//! Role-based access control catalog — analyzer-baked permission closure.
//!
//! The RBAC catalog is the project-wide registry of **permissions** (atomic
//! capability identifiers like `users:read`) and **roles** (named bundles of
//! permissions, optionally inheriting from one parent role). It lives at
//! [`crate::Module::rbac`] — one catalog per project — so the analyzer can
//! resolve every `actor.role.<name>` / `actor.permission.<name>` reference
//! against the same source of truth.
//!
//! ## Why the closure is baked in
//!
//! The author writes `role admin inherits owner { grants users:read }`.
//! Downstream consumers (codegen, doctor, inspect, the runtime policy
//! evaluator) all need the **flattened** permission set per role to make a
//! decision in one lookup — never recompute closures at runtime, never
//! recurse through inheritance graphs in three different consumers.
//!
//! `lazuli_analyzer::analyze_rbac_catalog` performs the closure once,
//! producing [`RoleEntry::closure`] sorted lexicographically. The wire-thin
//! discipline of `lazuli_ir`: types here only **carry** the closure result;
//! they never compute it.
//!
//! ## Grants vocabulary — closed
//!
//! [`RoleGrants`] is a discriminated union with three shapes that match the
//! authoring grammar:
//!
//! - [`RoleGrants::Explicit`] — `grants p1, p2`. Direct permission refs.
//! - [`RoleGrants::All`] — `grants_all`. Closure = every declared permission.
//!   Reserved for `superuser`-style roles; doctor warns when used.
//! - [`RoleGrants::InheritedOnly`] — no `grants*` block. Closure = the parent's
//!   closure only. Useful for thin role aliases.
//!
//! Adding a new shape (e.g. wildcard patterns) requires a proposal — closure
//! semantics must remain decidable for the analyzer.
//!
//! ## See also
//!
//! - `docs/proposals/rbac-catalog-vocab.md` — surface design + grants vocabulary
//! - [`crate::Module::rbac`] — owning slot
//! - `lazuli_analyzer::analyze_rbac_catalog` — closure producer

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Project-wide RBAC catalog. Lives at `Module::rbac` so the analyzer can
/// resolve every `actor.role.<name>` / `actor.permission.<name>` reference
/// against the same source of truth. Empty vectors are skipped in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RbacCatalog {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<PermissionEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<RoleEntry>,
}

/// One permission row. `name` is the full identifier (`users:read`);
/// `segments` is the colon-split projection the analyzer uses to validate
/// dotted/colon references and to render grouped UI in inspect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionEntry {
    /// Full identifier (`users:read`).
    pub name: String,
    /// Colon-split segments.
    pub segments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One role row. `closure` is the analyzer-computed flat permission set
/// (own grants plus transitively inherited grants), sorted for determinism
/// so downstream snapshots remain stable across runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleEntry {
    pub name: String,
    /// Single-parent inheritance ref (v0.1). `None` for root roles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits: Option<String>,
    pub grants: RoleGrants,
    /// Analyzer-computed flat permission list (closure of own grants
    /// plus transitively inherited grants). Sorted for determinism.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closure: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of role-grants shapes. Tag+content serde envelope —
/// `{"kind":"Explicit","value":[...]}`, `{"kind":"All"}`,
/// `{"kind":"InheritedOnly"}`. Widening requires a proposal because
/// closure semantics must remain decidable for the analyzer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RoleGrants {
    /// Explicit `grants` block — direct permission refs (closure also
    /// folds in inherited).
    Explicit(Vec<String>),
    /// `grants_all` shorthand. Closure = every declared permission.
    All,
    /// No `grants*` block. Closure = inherited only.
    InheritedOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_grants_explicit_round_trip() {
        let g = RoleGrants::Explicit(vec!["users:read".into(), "users:write".into()]);
        let s = serde_json::to_string(&g).unwrap();
        assert!(s.contains("\"kind\":\"Explicit\""));
        assert!(s.contains("\"users:read\""));
        let back: RoleGrants = serde_json::from_str(&s).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn role_grants_all_unit_variant_round_trips() {
        let s = serde_json::to_string(&RoleGrants::All).unwrap();
        assert!(s.contains("\"kind\":\"All\""));
        let back: RoleGrants = serde_json::from_str(&s).unwrap();
        assert_eq!(back, RoleGrants::All);
    }

    #[test]
    fn role_grants_inherited_only_round_trips() {
        let s = serde_json::to_string(&RoleGrants::InheritedOnly).unwrap();
        let back: RoleGrants = serde_json::from_str(&s).unwrap();
        assert_eq!(back, RoleGrants::InheritedOnly);
    }

    #[test]
    fn permission_entry_round_trip() {
        let p = PermissionEntry {
            name: "users:read".into(),
            segments: vec!["users".into(), "read".into()],
            span_ref: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: PermissionEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn empty_catalog_serialises_compactly() {
        let cat = RbacCatalog {
            permissions: vec![],
            roles: vec![],
        };
        let s = serde_json::to_string(&cat).unwrap();
        assert_eq!(s, "{}");
    }

    #[test]
    fn role_with_inherits_and_closure_round_trips() {
        let r = RoleEntry {
            name: "admin".into(),
            inherits: Some("owner".into()),
            grants: RoleGrants::Explicit(vec!["users:write".into()]),
            closure: vec!["users:read".into(), "users:write".into()],
            span_ref: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"inherits\":\"owner\""));
        assert!(s.contains("\"closure\""));
        let back: RoleEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
