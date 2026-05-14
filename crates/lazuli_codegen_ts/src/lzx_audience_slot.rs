//! L0 #3 `.lzx` integration codegen — Cells C.1 + C.2.
//!
//! - **Cell C.1** (`audience_sdk`): audience-scoped SDK projection per
//!   `docs/proposals/lzx-integration-codegen.md` §7. Computes which
//!   commands/queries are reachable from a given set of audience
//!   declarations and emits a filtered `<feat>.gen.ts`. The filter is
//!   compile-time: a `public` bundle simply does not export
//!   `deleteSlug` because the policy intersection misses, so any
//!   import attempt is a TypeScript error.
//!
//! - **Cell C.2** (`slot_interface`): slot props interface emitter per
//!   §8. For each `cells <field> @client.<slot>` binding, the
//!   generated `*Props` interface declares `value: Resource["field"]`
//!   and `row: Resource`, deriving the TS type by index access. For
//!   section slots (no field axis), `value: void` is emitted.
//!
//! This module owns a **local `.lzx` IR stub** matching the shape in
//! L0 #3 §6. The dedicated parser cell publishes types of identical
//! shape into `lazuli_ir`; once that lands, the stub is deleted and
//! consumers re-point to `lazuli_ir::Surface` / `Audience` / `View*`.
//! Until then this stub keeps Cells C.1 + C.2 testable end-to-end
//! under `cargo test -p lazuli_codegen_ts`.

#[path = "audience_sdk.rs"]
pub mod audience_sdk;

#[path = "slot_interface.rs"]
pub mod slot_interface;

pub use audience_sdk::{
    AudienceProjection, compute_audience_projection, emit_feature_sdk_filtered,
};
pub use slot_interface::emit_slot_interface;

// ---------------------------------------------------------------------------
// Local `.lzx` IR stub — replaced by `lazuli_ir::Surface` once the parser
// cell lands. Shapes are taken verbatim from L0 #3 §6.
// ---------------------------------------------------------------------------

pub mod ir {
    //! Local stub of the `.lzx` IR. The real producers (parser/analyzer)
    //! emit types of identical shape into `lazuli_ir`; this module is
    //! deleted once those land.

    /// A `.lzx` surface — one file per (feature, target) tuple. Lives
    /// at `features/<feature>/<feature>.web.lzx` or `.mobile.lzx`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Surface {
        /// Feature name (matches `surface <feature> web|mobile` header).
        pub feature: String,
        /// Web or mobile.
        pub target: SurfaceTarget,
        /// Audience blocks declared on the surface.
        pub audiences: Vec<Audience>,
    }

    /// Surface target — selects between web and mobile codegen
    /// pipelines.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SurfaceTarget {
        Web,
        Mobile,
    }

    impl SurfaceTarget {
        /// Dist prefix used in `dist/ts-<prefix>` per L0 #1 §4.
        pub fn dist_prefix(self) -> &'static str {
            match self {
                SurfaceTarget::Web => "ts-web",
                SurfaceTarget::Mobile => "ts-mobile",
            }
        }
    }

    /// `audience <name>` block — names map verbatim to the dist
    /// subpath (`dist/ts-web/<feat>/views/<audience>/...`). Kebab-case
    /// and snake_case are both legal in source; pascalization happens
    /// when emitting hook identifiers.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Audience {
        pub name: String,
        /// `requires @scope.<name>` atoms — OR semantics per §7.2. The
        /// emitter compares these against each command's effective
        /// policy atom set; any overlap admits the command into this
        /// audience's projection.
        pub requires: Vec<PolicyAtom>,
        /// View kinds nested inside the audience block.
        pub views: Vec<View>,
    }

    /// Three closed view kinds per §5. New kinds enter via a Lazuli
    /// language proposal — distros cannot extend.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum View {
        List(ViewList),
        Detail(ViewDetail),
        Create(ViewCreate),
    }

    impl View {
        pub fn name(&self) -> &str {
            match self {
                View::List(v) => &v.name,
                View::Detail(v) => &v.name,
                View::Create(v) => &v.name,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ViewList {
        pub name: String,
        pub route: Option<String>,
        pub source: QueryRef,
        pub columns: Vec<String>,
        pub search: Vec<String>,
        pub filter: Vec<String>,
        pub cells: Vec<CellBinding>,
        pub actions: Vec<CommandRef>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ViewDetail {
        pub name: String,
        pub route: Option<String>,
        pub source: QueryRef,
        pub route_params: Vec<RouteParam>,
        pub sections: Vec<String>,
        pub cells: Vec<CellBinding>,
        pub actions: Vec<CommandRef>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ViewCreate {
        pub name: String,
        pub route: Option<String>,
        pub submit: CommandRef,
        pub fields: Vec<String>,
        pub cells: Vec<CellBinding>,
    }

    /// Reference to a feature query.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct QueryRef {
        pub feature: String,
        pub kind: QueryKind,
        pub name: String,
    }

    /// Query kind — distinguishes list (returns arrays), lookup
    /// (returns one record), and raw SQL (returns whatever `returns
    /// <Type>` declares).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum QueryKind {
        List,
        Lookup,
        Sql,
    }

    /// Reference to a feature command.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CommandRef {
        pub feature: String,
        pub name: String,
    }

    /// `cells <field> @client.<slot>` — links a column/field to a
    /// slot component file under `features/<feat>/<target>/cells/`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CellBinding {
        pub field: String,
        pub slot: String,
    }

    /// `route <name>: <Type> from path` — typed path-parameter binding.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RouteParam {
        pub name: String,
        pub type_ref: String,
    }

    /// `@<namespace>.<name>` atom — currently always `@scope.<name>`
    /// inside audience `requires` blocks. Kept structured for forward
    /// compatibility with `@policy.X` and `@role.X` namespaces.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PolicyAtom {
        pub namespace: String,
        pub name: String,
    }

    impl PolicyAtom {
        /// Render as the canonical `@<namespace>.<name>` string. Used
        /// for set-membership checks against `Command.policy_atoms`
        /// (which the runtime spec already stores as
        /// `(namespace, name)` pairs).
        pub fn to_qualified(&self) -> String {
            format!("@{}.{}", self.namespace, self.name)
        }
    }
}

pub use ir::*;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Convert a snake_case / kebab-case / mixed identifier to PascalCase.
/// Acronyms (`id`, `url`, `api`, ...) are preserved as fully-uppercase
/// segments to match the runtime emitter's casing conventions.
pub(crate) fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(|c: char| c == '_' || c == '-' || c == ' ') {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    out
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}

#[cfg(test)]
mod helper_tests {
    use super::pascal_case;

    #[test]
    fn pascal_case_snake_to_pascal() {
        assert_eq!(pascal_case("type_badge"), "TypeBadge");
    }

    #[test]
    fn pascal_case_kebab_to_pascal() {
        assert_eq!(pascal_case("workspace-admin"), "WorkspaceAdmin");
    }

    #[test]
    fn pascal_case_acronym_uppercase() {
        assert_eq!(pascal_case("api_key"), "APIKey");
    }
}
