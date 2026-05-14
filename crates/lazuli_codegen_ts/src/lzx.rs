//! `.lzx` view-kind emitters — compile-time MVVM hooks per
//! `docs/proposals/lzx-integration-codegen.md` §6.
//!
//! Three view kinds each emit a single `<view>.gen.ts` to:
//!
//! ```text
//! dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts
//! ```
//!
//! Per L0 #1 §4 file canon. `<target>` is `ts-web` or `ts-mobile`
//! depending on the surface's `SurfaceTarget`; `<feat>`, `<audience>`,
//! `<view-name>` come straight off the IR — kebab-case and snake_case
//! are preserved verbatim in the path, then pascal-cased for hook
//! identifiers.
//!
//! Module layout:
//!
//! - `ir` — the canonical `.lzx` IR stub. The real types land in
//!   `lazuli_ir` via a separate cell; until that arrives this module
//!   owns the contract end-to-end so the emitters can be exercised
//!   under `cargo test -p lazuli_codegen_ts`.
//! - `lzx_view_list` / `lzx_view_detail` / `lzx_view_create` — the
//!   three per-kind emitters. Each is independent and shares only the
//!   helpers in this file (banner, qualified imports, identifier
//!   casing, file paths).
//! - `lzx_router_adapter` — the per-target `useParams` import switch
//!   for `view detail`.
//! - `emit_surface_views` — top-level walker. Orchestrator wires this
//!   into `generate()` post-merge.

#[path = "lzx_router_adapter.rs"]
pub mod lzx_router_adapter;

#[path = "lzx_view_list.rs"]
pub mod lzx_view_list;

#[path = "lzx_view_detail.rs"]
pub mod lzx_view_detail;

#[path = "lzx_view_create.rs"]
pub mod lzx_view_create;

use crate::GeneratedFile;
use lzx_router_adapter::RouterTarget;

// ---------------------------------------------------------------------------
// Canonical IR stub — replaced by `lazuli_ir` once the `.lzx` parser cell
// lands. The shapes match the spec in the L0 #3 cell-B brief verbatim.
// ---------------------------------------------------------------------------

pub mod ir {
    //! Local stub of the `.lzx` IR. The real producers (parser/analyzer)
    //! emit types of identical shape into `lazuli_ir`; this module is
    //! deleted once those land. Until then, downstream code in this
    //! crate consumes these types via `crate::lzx::ir::*`.

    /// A `.lzx` surface — one file per (feature, target) tuple. Lives at
    /// `features/<feature>/<feature>.web.lzx` or `.mobile.lzx`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Surface {
        /// Feature name (matches `surface <feature> web|mobile` header).
        pub feature: String,
        /// Web or mobile.
        pub target: SurfaceTarget,
        /// Audience blocks declared on the surface.
        pub audiences: Vec<Audience>,
    }

    /// Surface target — selects between web and mobile codegen pipelines.
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

    /// `audience <name>` block — names map verbatim to the dist subpath
    /// (`dist/ts-web/<feat>/views/<audience>/...`). Kebab-case and
    /// snake_case are both legal in source; pascalization happens when
    /// emitting hook identifiers.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Audience {
        pub name: String,
        /// `requires @scope.<name>` atoms — OR semantics per §7.2.
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

    /// Reference to a feature query. The emitter resolves this to the
    /// SDK identifier (`listMineSlugs` / `lookupSlugByKey`) per §6.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct QueryRef {
        pub feature: String,
        pub kind: QueryKind,
        pub name: String,
    }

    /// Query kind — distinguishes list-shaped queries (return arrays)
    /// from lookup-shaped (return single records) and raw SQL.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum QueryKind {
        List,
        Lookup,
        Sql,
    }

    /// Reference to a feature command. The short name (last segment) is
    /// used as the action-map KEY; the SDK identifier (verb +
    /// pascal(feature)) is the action-map VALUE.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CommandRef {
        pub feature: String,
        pub name: String,
    }

    /// `cells <field> @client.<slot>` binding — links a column/field to
    /// a slot component file under `features/<feat>/<target>/cells/`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CellBinding {
        pub field: String,
        pub slot: String,
    }

    /// `route <name>: <Type> from path` — typed path parameter binding.
    /// `type_ref` is the surface type label (`Text`, `Integer`, ...);
    /// the emitter maps it to a TS type via `field_kind_ts` at use
    /// site, but for `.lzx` the only currently-emitted shape is `Text`
    /// (path segments are always strings in the URL).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RouteParam {
        pub name: String,
        pub type_ref: String,
    }

    /// `@<namespace>.<name>` atom — currently always `@scope.<...>`
    /// inside audience `requires` blocks. Kept structured for forward
    /// compatibility with `@policy.X` and `@role.X` namespaces.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PolicyAtom {
        pub namespace: String,
        pub name: String,
    }
}

pub use ir::*;

// ---------------------------------------------------------------------------
// Shared emitter helpers — used by all three view-kind emitters.
// ---------------------------------------------------------------------------

/// Emit the canonical `// Code generated …` banner that every `.gen.ts`
/// file shares.
pub(crate) fn banner() -> &'static str {
    "// Code generated by lazuli; DO NOT EDIT.\n"
}

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

/// lowerCamelCase: same as `pascal_case` but the very first character
/// stays lowercase.
pub(crate) fn lower_camel(s: &str) -> String {
    let pascal = pascal_case(s);
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut out = String::with_capacity(pascal.len());
            for c in first.to_lowercase() {
                out.push(c);
            }
            out.push_str(chars.as_str());
            out
        }
    }
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}

/// Resolve a `CommandRef` to its imported SDK identifier per the
/// convention in §6.1 / §6.3: verb + PascalResource. For example
/// `slug.command.create` → `createSlug`; `slug.command.update_email` →
/// `updateSlug` is wrong (the full `update_email` short name is split
/// on `_` so the suffix words pascalize). Matches `runtime::
/// command_var_name`.
pub(crate) fn command_ident(cmd: &CommandRef) -> String {
    let resource_pascal = pascal_case(&cmd.feature);
    let mut parts = cmd.name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();
    let mut out = verb.to_ascii_lowercase();
    out.push_str(&resource_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out
}

/// The short tail of a command name — used as the KEY in the
/// `actions: { create: createSlug, ... }` map. `slug.command.create` →
/// `create`; `slug.command.update_email` → `updateEmail`.
pub(crate) fn command_action_key(cmd: &CommandRef) -> String {
    lower_camel(&cmd.name)
}

/// Resolve a `QueryRef` to its imported SDK identifier per §6 examples.
/// - List: `list<PascalShort><PascalFeature>s` (e.g. `slug.query.mine`
///   → `listMineSlugs`). If `short` is exactly `list`, the short is
///   dropped (`slug.query.list` → `listSlugs`).
/// - Lookup: `lookup<PascalFeature>By<PascalShort>` — a leading `by_`
///   is stripped from `short` first, so `by_key` → `Key`. If `short`
///   has no `by_` prefix, the entire short pascal-cases.
/// - Sql: same convention as List for now (raw-SQL queries are rare
///   inside `.lzx` and surface a similar shape).
pub(crate) fn query_ident(q: &QueryRef) -> String {
    let resource_pascal = pascal_case(&q.feature);
    match q.kind {
        QueryKind::List | QueryKind::Sql => {
            let short = q.name.as_str();
            if short.eq_ignore_ascii_case("list") {
                format!("list{}s", resource_pascal)
            } else {
                format!("list{}{}s", pascal_case(short), resource_pascal)
            }
        }
        QueryKind::Lookup => {
            let stripped = q.name.strip_prefix("by_").unwrap_or(&q.name);
            format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
        }
    }
}

/// File path for the emitted view hook per L0 #1 §4:
/// `dist/ts-<target>/<feat>/views/<audience>/<view-name>.gen.ts`.
pub(crate) fn view_file_path(
    target: SurfaceTarget,
    feature: &str,
    audience: &str,
    view_name: &str,
) -> String {
    format!(
        "dist/{}/{}/views/{}/{}.gen.ts",
        target.dist_prefix(),
        feature,
        audience,
        view_name
    )
}

/// Spec const name: `<audience><View>View`. E.g. `adminSlugListView`.
pub(crate) fn view_spec_const(audience: &str, view_name: &str) -> String {
    let aud = lower_camel(audience);
    let view = pascal_case(view_name);
    format!("{}{}View", aud, view)
}

/// Hook name: `use<PascalAudience><PascalView>View`. E.g.
/// `useAdminSlugListView`. Note `slug` is not in the hook name — the
/// dist path already scopes by feature.
pub(crate) fn view_hook_name(audience: &str, view_name: &str) -> String {
    format!(
        "use{}{}View",
        pascal_case(audience),
        pascal_case(view_name)
    )
}

/// Pascal name for an `<Audience><View>` prefix (used for slot
/// interfaces and section types). E.g. `AdminSlugList`.
pub(crate) fn audience_view_pascal(audience: &str, view_name: &str) -> String {
    format!("{}{}", pascal_case(audience), pascal_case(view_name))
}

/// Format the `cells` literal for the spec const — a TS object literal
/// like `{ tags: "@client.type_badge" as const }`. Empty input emits an
/// empty object so the spec field is always present.
pub(crate) fn format_cells_literal(cells: &[CellBinding]) -> String {
    if cells.is_empty() {
        return "{}".to_owned();
    }
    let parts: Vec<String> = cells
        .iter()
        .map(|c| format!("{}: \"@client.{}\" as const", c.field, c.slot))
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

/// Format a `["a", "b", "c"]` string literal array with each entry
/// quoted. Used for `columns` / `search` / `filter` / `fields`.
pub(crate) fn format_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_owned();
    }
    let parts: Vec<String> = items.iter().map(|s| format!("\"{}\"", s)).collect();
    format!("[{}]", parts.join(", "))
}

// ---------------------------------------------------------------------------
// Top-level walker — orchestrator wires this into `generate()` post-merge.
// ---------------------------------------------------------------------------

/// Walk a `Surface` and emit one `<view>.gen.ts` per (audience, view)
/// tuple. The `router_target` parameter feeds the per-target
/// `useParams` import for `view detail` hooks (per §6.2 table); list
/// and create views ignore it.
pub fn emit_surface_views(surface: &Surface, router_target: RouterTarget) -> Vec<GeneratedFile> {
    let mut files = Vec::new();
    for audience in &surface.audiences {
        for view in &audience.views {
            let (view_name, contents) = match view {
                View::List(v) => (
                    v.name.clone(),
                    lzx_view_list::emit_view_list(surface, audience, v),
                ),
                View::Detail(v) => (
                    v.name.clone(),
                    lzx_view_detail::emit_view_detail(surface, audience, v, router_target),
                ),
                View::Create(v) => (
                    v.name.clone(),
                    lzx_view_create::emit_view_create(surface, audience, v),
                ),
            };
            files.push(GeneratedFile {
                path: view_file_path(surface.target, &surface.feature, &audience.name, &view_name),
                contents,
            });
        }
    }
    files
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod test_fixtures {
    //! Test fixtures shared between `lzx.rs` and the per-view emitter
    //! test modules. Encodes the L0 #3 §13.1 `slug.web.lzx` example
    //! verbatim.
    use super::ir::*;

    pub fn type_badge_cell() -> CellBinding {
        CellBinding {
            field: "tags".to_owned(),
            slot: "type_badge".to_owned(),
        }
    }

    pub fn slug_list_view() -> ViewList {
        ViewList {
            name: "slug_list".to_owned(),
            route: Some("/slugs".to_owned()),
            source: QueryRef {
                feature: "slug".to_owned(),
                kind: QueryKind::List,
                name: "mine".to_owned(),
            },
            columns: vec![
                "key".to_owned(),
                "title".to_owned(),
                "tags".to_owned(),
                "created_at".to_owned(),
            ],
            search: vec!["key".to_owned(), "title".to_owned()],
            filter: vec!["tags".to_owned()],
            cells: vec![type_badge_cell()],
            actions: vec![
                CommandRef {
                    feature: "slug".to_owned(),
                    name: "create".to_owned(),
                },
                CommandRef {
                    feature: "slug".to_owned(),
                    name: "update".to_owned(),
                },
                CommandRef {
                    feature: "slug".to_owned(),
                    name: "delete".to_owned(),
                },
            ],
        }
    }

    pub fn slug_detail_view() -> ViewDetail {
        ViewDetail {
            name: "slug_detail".to_owned(),
            route: Some("/slugs/:key".to_owned()),
            source: QueryRef {
                feature: "slug".to_owned(),
                kind: QueryKind::Lookup,
                name: "by_key".to_owned(),
            },
            route_params: vec![RouteParam {
                name: "key".to_owned(),
                type_ref: "Text".to_owned(),
            }],
            sections: vec![
                "header".to_owned(),
                "metadata".to_owned(),
                "related_items".to_owned(),
            ],
            cells: vec![type_badge_cell()],
            actions: vec![
                CommandRef {
                    feature: "slug".to_owned(),
                    name: "update".to_owned(),
                },
                CommandRef {
                    feature: "slug".to_owned(),
                    name: "delete".to_owned(),
                },
            ],
        }
    }

    pub fn slug_create_view() -> ViewCreate {
        ViewCreate {
            name: "slug_create".to_owned(),
            route: Some("/slugs/new".to_owned()),
            submit: CommandRef {
                feature: "slug".to_owned(),
                name: "create".to_owned(),
            },
            fields: vec![
                "key".to_owned(),
                "title".to_owned(),
                "description".to_owned(),
                "tags".to_owned(),
            ],
            cells: vec![type_badge_cell()],
        }
    }

    pub fn public_slug_list_view() -> ViewList {
        ViewList {
            name: "public_slug_list".to_owned(),
            route: Some("/browse".to_owned()),
            source: QueryRef {
                feature: "slug".to_owned(),
                kind: QueryKind::List,
                name: "mine".to_owned(),
            },
            columns: vec!["key".to_owned(), "title".to_owned()],
            search: vec!["key".to_owned(), "title".to_owned()],
            filter: vec![],
            cells: vec![],
            actions: vec![],
        }
    }

    pub fn admin_audience() -> Audience {
        Audience {
            name: "admin".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_admin".to_owned(),
            }],
            views: vec![
                View::List(slug_list_view()),
                View::Detail(slug_detail_view()),
                View::Create(slug_create_view()),
            ],
        }
    }

    pub fn public_audience() -> Audience {
        Audience {
            name: "public".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_member".to_owned(),
            }],
            views: vec![View::List(public_slug_list_view())],
        }
    }

    pub fn slug_web_surface() -> Surface {
        Surface {
            feature: "slug".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![admin_audience(), public_audience()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lzx_router_adapter::RouterTarget;

    #[test]
    fn pascal_case_handles_snake_kebab_and_acronyms() {
        assert_eq!(pascal_case("slug_list"), "SlugList");
        assert_eq!(pascal_case("workspace-admin"), "WorkspaceAdmin");
        assert_eq!(pascal_case("workspace_admin"), "WorkspaceAdmin");
        // `id` is in the acronym table; uppercased segments are kept.
        assert_eq!(pascal_case("by_id"), "ByID");
        assert_eq!(pascal_case("user_id"), "UserID");
        assert_eq!(pascal_case(""), "");
    }

    #[test]
    fn lower_camel_first_char_lowercases() {
        assert_eq!(lower_camel("admin"), "admin");
        assert_eq!(lower_camel("workspace-admin"), "workspaceAdmin");
        assert_eq!(lower_camel("Slug"), "slug");
    }

    #[test]
    fn command_ident_handles_simple_and_compound_short_names() {
        let create = CommandRef {
            feature: "slug".to_owned(),
            name: "create".to_owned(),
        };
        assert_eq!(command_ident(&create), "createSlug");

        let update_email = CommandRef {
            feature: "customer".to_owned(),
            name: "update_email".to_owned(),
        };
        assert_eq!(command_ident(&update_email), "updateCustomerEmail");
    }

    #[test]
    fn query_ident_list_includes_short_name() {
        let mine = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "mine".to_owned(),
        };
        assert_eq!(query_ident(&mine), "listMineSlugs");

        let plain_list = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "list".to_owned(),
        };
        assert_eq!(query_ident(&plain_list), "listSlugs");
    }

    #[test]
    fn query_ident_lookup_strips_by_prefix() {
        let by_key = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::Lookup,
            name: "by_key".to_owned(),
        };
        assert_eq!(query_ident(&by_key), "lookupSlugByKey");
    }

    #[test]
    fn view_file_path_matches_l0_1_canon() {
        let p = view_file_path(SurfaceTarget::Web, "slug", "admin", "slug_list");
        assert_eq!(p, "dist/ts-web/slug/views/admin/slug_list.gen.ts");

        let mobile = view_file_path(SurfaceTarget::Mobile, "item", "kiosk", "item_create");
        assert_eq!(mobile, "dist/ts-mobile/item/views/kiosk/item_create.gen.ts");
    }

    #[test]
    fn view_spec_const_and_hook_naming() {
        assert_eq!(view_spec_const("admin", "slug_list"), "adminSlugListView");
        assert_eq!(view_hook_name("admin", "slug_list"), "useAdminSlugListView");
        assert_eq!(
            view_spec_const("workspace-admin", "slug_detail"),
            "workspaceAdminSlugDetailView"
        );
        assert_eq!(
            view_hook_name("workspace-admin", "slug_detail"),
            "useWorkspaceAdminSlugDetailView"
        );
    }

    #[test]
    fn emit_surface_views_matches_proposal_section_13() {
        // Walks the §13.1 `slug.web.lzx` fixture and asserts the four
        // emitted files land at the right paths with the right
        // top-level identifiers.
        let surface = test_fixtures::slug_web_surface();
        let files = emit_surface_views(&surface, RouterTarget::ViteReact);

        // Four (audience, view) tuples: admin × {list, detail, create}
        // plus public × list.
        assert_eq!(files.len(), 4, "expected 4 files, got {:?}",
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>());

        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"dist/ts-web/slug/views/admin/slug_list.gen.ts"));
        assert!(paths.contains(&"dist/ts-web/slug/views/admin/slug_detail.gen.ts"));
        assert!(paths.contains(&"dist/ts-web/slug/views/admin/slug_create.gen.ts"));
        assert!(paths.contains(&"dist/ts-web/slug/views/public/public_slug_list.gen.ts"));

        // Spot-check each emitted file carries its hook + spec const.
        let by_path = |p: &str| files.iter().find(|f| f.path == p).expect("path present");

        let list_file = by_path("dist/ts-web/slug/views/admin/slug_list.gen.ts");
        assert!(list_file.contents.contains("export const adminSlugListView"));
        assert!(list_file.contents.contains("export function useAdminSlugListView"));
        assert!(list_file.contents.contains("listMineSlugs"));

        let detail_file = by_path("dist/ts-web/slug/views/admin/slug_detail.gen.ts");
        assert!(detail_file.contents.contains("export const adminSlugDetailView"));
        assert!(detail_file.contents.contains("export function useAdminSlugDetailView"));
        assert!(detail_file.contents.contains("@tanstack/react-router"));
        assert!(detail_file.contents.contains("AdminSlugDetailSections"));

        let create_file = by_path("dist/ts-web/slug/views/admin/slug_create.gen.ts");
        assert!(create_file.contents.contains("export const adminSlugCreateView"));
        assert!(create_file.contents.contains("export function useAdminSlugCreateView"));
        assert!(create_file.contents.contains("zodResolver"));

        let public_file = by_path("dist/ts-web/slug/views/public/public_slug_list.gen.ts");
        assert!(public_file.contents.contains("export const publicPublicSlugListView"));
        assert!(public_file.contents.contains("export function usePublicPublicSlugListView"));
    }

    #[test]
    fn emit_surface_views_switches_router_target_for_detail() {
        let surface = test_fixtures::slug_web_surface();

        let vite_files = emit_surface_views(&surface, RouterTarget::ViteReact);
        let nextjs_files = emit_surface_views(&surface, RouterTarget::NextJs);

        let vite_detail = vite_files
            .iter()
            .find(|f| f.path.ends_with("slug_detail.gen.ts"))
            .unwrap();
        let nextjs_detail = nextjs_files
            .iter()
            .find(|f| f.path.ends_with("slug_detail.gen.ts"))
            .unwrap();

        assert!(vite_detail.contents.contains("@tanstack/react-router"));
        assert!(!vite_detail.contents.contains("next/navigation"));
        assert!(nextjs_detail.contents.contains("next/navigation"));
        assert!(!nextjs_detail.contents.contains("@tanstack/react-router"));
    }

    #[test]
    fn format_cells_literal_handles_empty_and_populated() {
        assert_eq!(format_cells_literal(&[]), "{}");
        let cells = vec![CellBinding {
            field: "tags".to_owned(),
            slot: "type_badge".to_owned(),
        }];
        assert_eq!(
            format_cells_literal(&cells),
            "{ tags: \"@client.type_badge\" as const }"
        );
    }

    #[test]
    fn format_string_array_handles_empty_and_populated() {
        assert_eq!(format_string_array(&[]), "[]");
        let items = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(format_string_array(&items), "[\"a\", \"b\", \"c\"]");
    }
}
