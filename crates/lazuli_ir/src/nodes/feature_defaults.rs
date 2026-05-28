//! Feature-level defaults, resource enrichment, extensions, escape routes.
//!
//! These types are the **opinionated middle ground** between the feature
//! header and the per-resource declarations. They cover four orthogonal
//! concerns the canonical-indent grammar groups together at the same
//! authoring depth:
//!
//! - **Defaults** ([`Defaults`], [`Tenancy`]) — feature-wide knobs that
//!   each resource inherits unless it opts out. Lowered without
//!   resolution: the analyzer's derived pass applies `Defaults` onto
//!   every `Resource` whose own slot is `None`.
//! - **Resource enrichment** ([`Constraint`], [`UniqueConstraint`],
//!   [`IndexConstraint`], [`IndexMethod`], [`FieldValidation`]) —
//!   schema-shaped overlays that live on `Resource` but don't fit on
//!   `Field` (they span multiple fields or cross into the `validate`
//!   escape hatch).
//! - **Extensions** ([`Extension`], [`ExtensionContract`]) — the
//!   single canonical way the language references author-written code
//!   (Go, TSX) that lives outside the `.lzi` surface. Each contract is
//!   a closed enum variant; doctor and codegen branch on the variant
//!   and rely on `PathRef` to know whether the path came from
//!   convention or an explicit `at "..."` clause.
//! - **Escape routes** ([`EscapeRoute`]) — pages Lazuli should know
//!   about (for guard/routing wiring) but not govern internally.
//!
//! ## Why one file
//!
//! Splitting this group further would force readers to chase
//! `Defaults → Tenancy → Resource → Constraint → IndexMethod` across
//! four files when authoring a single new feature. The cluster is small
//! (~150 LOC) and the four concerns share the same authoring depth in
//! the grammar — keeping them adjacent matches the cognitive model.
//!
//! ## Closed-catalog discipline
//!
//! Two enums are intentionally closed and require a proposal to widen:
//!
//! - [`IndexMethod`] — `btree | gin | gist`. Each method maps to a
//!   different PostgreSQL operator class set; codegen branches on the
//!   value when emitting `CREATE INDEX ... USING <method>`.
//! - [`ExtensionContract`] — the catalog of host-language contracts
//!   the framework knows how to wire (`CellRenderer`, `Hook`,
//!   `Validator`, `Function`, `IntegrationAdapter`, …). Adding a
//!   contract is a minor ABI bump; changing one is a major bump. See
//!   `docs/canonical-semantics.md` "Extension Path Convention" for
//!   the full table.
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` — extension path convention table
//! - [`crate::PolicyRef`] — used by [`Defaults::policy`] and
//!   [`EscapeRoute::policy`]
//! - [`crate::Feature::defaults`] / [`crate::Feature::escape_routes`] —
//!   the owning slots

use serde::{Deserialize, Serialize};

use crate::{EvalPredicate, PolicyRef, SpanRef, TypeRef, is_false};

/// One `non_goals.<key> "<description>"` entry under a feature. Carries
/// the boundary-key + prose pair the capsule's `delegated_to` /
/// `out_of_scope` lists flatten into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonGoal {
    /// Boundary key. Canonical capsules group these under `delegated_to`
    /// or `out_of_scope`; the lowered IR keeps a flat key for now.
    pub key: String,
    pub description: String,
}

/// Feature-level `defaults` block. Resource-local declarations override these.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenancy: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub timestamps: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
}

/// Tenancy axis. Non-tenant resources use `Tenancy::None` (the explicit
/// `tenancy none` opt-out); resources inheriting feature defaults carry
/// `Resource.tenancy = None` until the derived pass resolves them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "axis", content = "value")]
pub enum Tenancy {
    Org,
    Team,
    /// Custom axis identifier (`tenancy workspace`, etc.).
    Custom(String),
    /// Explicit `tenancy none` opt-out.
    None,
}

/// Closed sum over resource-level constraint shapes. `Unique` flags
/// uniqueness (with optional per-axis qualifier); `Index` declares
/// secondary indexes with method + full-text flag.
///
/// `Eq` is intentionally omitted: `UniqueConstraint.when` carries an
/// `EvalPredicate` (transitively reaching `Expr`, which is not `Eq`)
/// for the GAP-NEW-001 partial-index form. Consumers needing equality
/// use `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Constraint {
    Unique(UniqueConstraint),
    Index(IndexConstraint),
}

/// `unique <fields> [per <axis>] [when <predicate>]` constraint. The
/// `per` slot scopes uniqueness per-tenant / per-org / per-team etc.
///
/// GAP-NEW-001 — when `when` is `Some`, the constraint lowers to a
/// PARTIAL unique index (`CREATE UNIQUE INDEX ... WHERE <predicate>`)
/// rather than a table-level `UNIQUE (...)` clause, since Postgres only
/// supports the `WHERE` qualifier on indexes, not table constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueConstraint {
    pub fields: Vec<String>,
    /// `unique email per org` -> `qualifier = Some("org")`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per: Option<String>,
    /// GAP-NEW-001 — `when <predicate>` partial-index predicate. `None`
    /// is the unconditional UNIQUE constraint form. Parsed from verbatim
    /// source text via the shared closed-predicate parser; `Unparsed`
    /// when the shape isn't recognized so doctor can echo the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<EvalPredicate>,
}

/// `index <fields> [using <method>]` constraint. The `full_text` flag
/// switches the emitted index to `to_tsvector('english', ...)` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConstraint {
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<IndexMethod>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub full_text: bool,
}

/// Closed catalog of Postgres index methods admitted in
/// [`IndexConstraint`]. Adding entries requires a proposal so the
/// migration emitter stays a closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexMethod {
    /// Default — B-tree index for equality + range queries.
    Btree,
    /// GIN — inverted index for JSON / array / full-text columns.
    Gin,
    /// GiST — generalized search tree for geometric / range types.
    Gist,
}

/// `validate <field> path "<...>"` declaration on a resource. Binds the
/// named field to a validator file (extension contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValidation {
    pub field: String,
    pub path: PathRef,
}

/// An extension contract declared under `extensions` and resolved to a
/// filesystem implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
    pub contract: ExtensionContract,
    pub resolved_path: PathRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of extension contracts. Adding a contract is a minor bump;
/// changing one is a major bump. See `docs/canonical-semantics.md`
/// "Extension Path Convention" for the full table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ExtensionContract {
    /// `client <name>: CellRenderer[X]`
    CellRenderer { type_arg: TypeRef },
    /// `client <name>: ViewBlock[X]` or single-use `block <name>: ViewBlock[X]`
    ViewBlock { type_arg: TypeRef },
    /// `client <name>: FormField[X]`
    FormField { type_arg: TypeRef },
    /// `hook <name>: Hook[X]`
    Hook { type_arg: TypeRef },
    /// `validator <name>: Validator[X]`
    Validator { type_arg: TypeRef },
    /// `fn <name>: Function[X, Y]`
    Function { input: TypeRef, output: TypeRef },
    /// `query_modifier <name>: QueryModifier[X]`
    QueryModifier { type_arg: TypeRef },
    /// `adapter <name>: IntegrationAdapter[X]`
    IntegrationAdapter { type_arg: TypeRef },
}

/// Filesystem path with provenance. `Convention` paths are derived from the
/// extension name + contract kind via the table in `docs/canonical-semantics.md`;
/// `Authored` paths come from an explicit `at "..."` clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRef {
    pub path: String,
    pub source: PathSource,
}

/// Closed catalog distinguishing convention-derived paths from
/// author-supplied paths. Doctor uses this to surface "this is the
/// conventional location; rename or supply an `at` clause" hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSource {
    /// Derived from the extension name + contract kind via the
    /// canonical convention table.
    Convention,
    /// Explicit `at "..."` clause supplied by the author.
    Authored,
}

impl PathRef {
    /// Build a [`PathRef`] whose path comes from the convention table.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::{PathRef, PathSource};
    ///
    /// let p = PathRef::convention("./client/foo.ts");
    /// assert_eq!(p.source, PathSource::Convention);
    /// ```
    pub fn convention(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: PathSource::Convention,
        }
    }

    /// Build a [`PathRef`] whose path was authored explicitly with `at`.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::{PathRef, PathSource};
    ///
    /// let p = PathRef::authored("./custom.ts");
    /// assert_eq!(p.source, PathSource::Authored);
    /// ```
    pub fn authored(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: PathSource::Authored,
        }
    }
}

/// Pages Lazuli should know about but should not govern internally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeRoute {
    pub route: String,
    pub at: PathRef,
    pub policy: PolicyRef,
    /// Coarse tenant axis for the escape page. `None` = no tenant scope claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenancy_uses_axis_content_envelope() {
        let t = Tenancy::Custom("workspace".into());
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"axis\":\"Custom\""));
        assert!(s.contains("\"value\":\"workspace\""));
        let back: Tenancy = serde_json::from_str(&s).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn tenancy_org_round_trips() {
        let s = serde_json::to_string(&Tenancy::Org).unwrap();
        let back: Tenancy = serde_json::from_str(&s).unwrap();
        assert_eq!(back, Tenancy::Org);
    }

    #[test]
    fn defaults_skip_empty_fields_in_json() {
        let d = Defaults::default();
        let s = serde_json::to_string(&d).unwrap();
        assert_eq!(s, "{}");
    }

    #[test]
    fn index_method_snake_case() {
        let s = serde_json::to_string(&IndexMethod::Btree).unwrap();
        assert_eq!(s, "\"btree\"");
        let s = serde_json::to_string(&IndexMethod::Gin).unwrap();
        assert_eq!(s, "\"gin\"");
        let s = serde_json::to_string(&IndexMethod::Gist).unwrap();
        assert_eq!(s, "\"gist\"");
    }

    #[test]
    fn constraint_tag_envelope() {
        let c = Constraint::Unique(UniqueConstraint {
            fields: vec!["email".into()],
            per: Some("org".into()),
            when: None,
        });
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains("\"kind\":\"Unique\""));
        assert!(s.contains("\"per\":\"org\""));
        // Unconditional form omits the `when` slot entirely.
        assert!(!s.contains("\"when\""));
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn path_ref_constructors_preserve_source() {
        let c = PathRef::convention("features/x/extensions/y.go");
        assert_eq!(c.source, PathSource::Convention);
        let a = PathRef::authored("custom/y.go");
        assert_eq!(a.source, PathSource::Authored);
    }

    #[test]
    fn extension_contract_round_trips() {
        let e = ExtensionContract::Function {
            input: TypeRef::Builtin(crate::BuiltinType::Text),
            output: TypeRef::Builtin(crate::BuiltinType::Boolean),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"kind\":\"Function\""));
        let back: ExtensionContract = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn escape_route_skips_optional_tenant_and_span() {
        let er = EscapeRoute {
            route: "/admin/about".into(),
            at: PathRef::convention("pages/admin/about.tsx"),
            policy: PolicyRef::Local("admin".into()),
            tenant: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&er).unwrap();
        assert!(!s.contains("\"tenant\""));
        assert!(!s.contains("\"span_ref\""));
    }
}
