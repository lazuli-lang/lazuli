//! RBAC catalog vocab — top-level `permission <ident>` and `role <name>`
//! declarations. Package-scoped (sibling of `feature`); see
//! `docs/proposals/rbac-catalog-vocab.md`.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// A single permission declaration: `permission users:read`.
/// Stored as the verbatim source token plus its colon-split segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDeclAst {
    /// Full identifier (e.g., `users:read` or `report:repasse:mark`).
    pub name: String,
    /// Colon-split segments (2-4 entries; grammar-enforced).
    pub segments: Vec<String>,
    pub span: Span,
}

/// A single role declaration with optional `inherits` and one of
/// `grants` / `grants_all` / neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDeclAst {
    pub name: String,
    /// Optional single-parent inheritance (`inherits <role>`).
    /// Multi-parent (`inherits A, B`) is rejected at parse time.
    pub inherits: Option<String>,
    pub grants: RoleGrantsAst,
    pub span: Span,
}

/// Authored shape of a role's grants. `Explicit` carries one permission
/// ref per line (bare colon-identifiers, resolved against the catalog by
/// the analyzer). `All` is the `grants_all` shorthand. `InheritedOnly`
/// is no `grants*` block at all — the role's grants come entirely from
/// the inheritance chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RoleGrantsAst {
    Explicit(Vec<String>),
    All,
    InheritedOnly,
}
