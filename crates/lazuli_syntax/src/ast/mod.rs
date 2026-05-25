use serde::{Deserialize, Serialize};

mod agent;
mod auth;
mod design;
mod resource;
mod event;
mod job;
mod mcp;
mod notification;
mod plan;
mod report;
mod tenant_migration;
mod webhook;
pub use agent::*;
pub use auth::*;
pub use design::*;
pub use resource::*;
pub use event::*;
pub use job::*;
pub use mcp::*;
pub use notification::*;
pub use plan::*;
pub use report::*;
pub use tenant_migration::*;
pub use webhook::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// `ir-rate-limit-env-aware` cell 1 — AST analog of `ir::RateLimitSpec`.
///
/// Aggregates one optional unqualified default + any number of
/// env-qualified override entries. The parser builds this from
/// consecutive `rate_limit "..."` / `rate_limit "..." in <envs>` lines
/// (proposal §4.2). The single-line source shape produces a spec with
/// `default = Some("X")` and `by_env = []`; multi-line shapes populate
/// `by_env` in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitSpecAst {
    /// Unqualified `rate_limit "X"` line — at most one per declaration.
    /// `None` when the source authored only env-qualified lines (Cell 3
    /// doctor emits `rate_limit_no_default_with_qualifications`).
    pub default: Option<String>,
    /// `rate_limit "X" in <env_list>` entries in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_env: Vec<RateLimitByEnvAst>,
    pub span: Span,
}

/// AST analog of `ir::RateLimitByEnv`. Envs are kept as raw identifiers
/// here; the analyzer normalises them into the closed `EnvName` catalog
/// at lowering time (`crates/lazuli_analyzer/src/lib.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitByEnvAst {
    /// Limit string verbatim (quotes stripped). The proposal-defined
    /// keyword `"unlimited"` (§4.4) is preserved as-is; the analyzer
    /// lowers it to the empty-string sentinel in `RateLimitByEnv.limit`.
    pub limit: String,
    /// Raw env identifiers as written. Non-empty by construction — the
    /// parser rejects `rate_limit "X" in` (empty tail).
    pub envs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub app: Option<String>,
    pub aggregates: Vec<Aggregate>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aggregate {
    pub name: String,
    pub fields: Vec<Field>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub surfaces: Vec<Surface>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub modifiers: Vec<FieldModifier>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum FieldModifier {
    Required,
    Unique,
    Default(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub input: Vec<String>,
    pub policy: Option<String>,
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub name: String,
    pub search: Vec<String>,
    pub filters: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    pub list_columns: Vec<String>,
    pub form_fields: Vec<String>,
    pub detail_fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxDocument {
    pub app: Option<LzxApp>,
    pub routes: Vec<LzxRoute>,
    pub experiences: Vec<LzxExperience>,
    pub surfaces: Vec<LzxSurface>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxApp {
    pub name: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub targets: Vec<String>,
    pub default_locale: Option<String>,
    pub default_timezone: Option<String>,
    pub auth_failed_redirect: Option<String>,
    pub route_guard: Option<LzxRouteGuardDefaults>,
    pub actor_query: Option<String>,
    pub not_found: Option<String>,
    pub error_pages: Vec<LzxErrorPage>,
    pub uses: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRouteGuardDefaults {
    pub default_policy: Option<String>,
    pub on_unauthenticated: Option<String>,
    pub on_unauthorized: Option<String>,
    pub skeleton: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxViewGuard {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy: Vec<String>,
    pub on_unauthenticated: Option<String>,
    pub on_unauthorized: Option<String>,
    pub requires_lifecycle: Option<LzxRequiresLifecycle>,
    pub on_lifecycle_pending: Option<String>,
    /// router-w3 Tier 3 — `forbid_when <atom> dispatch_to "<url>"`
    /// children. Ordered; codegen emits checks in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbid_when: Vec<LzxForbidWhen>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxForbidWhen {
    pub atom_ref: String,
    pub dispatch_to: String,
    pub span: Span,
}

/// router-w5 — `loader <feature>.<query>` slot under a route block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRouteLoader {
    pub feature: String,
    pub query: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRequiresLifecycle {
    pub resource: String,
    pub state: String,
    pub substep: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxErrorPage {
    pub status: u16,
    pub template: String,
    pub audience: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRoute {
    pub name: String,
    pub path: Option<String>,
    pub routes: Vec<String>,
    pub to: Option<String>,
    pub surface: Option<String>,
    pub audience: Option<String>,
    pub lazy: Option<bool>,
    pub prerender: Option<String>,
    pub guard: Option<LzxViewGuard>,
    /// router-w5 — `loader <feature>.<query>` declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loaders: Vec<LzxRouteLoader>,
    /// router-w6 — `pending_view <component_key>` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_view: Option<String>,
    /// router-w6 — `error_view <component_key>` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_view: Option<String>,
    /// router-w8 — `parent <route_name>` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Wave §2 (2026-05-24) — typed path-param declarations on the
    /// route block. Authored as `route <name>: <Type>` (e.g.
    /// `route id: ID`). Surfaced in `ir::AppRoute.route_params`;
    /// codegen emits a typed `parse<Route>Params` per app-level
    /// route, replacing the manual `Number(params.id)` coercion at
    /// the consumer site.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_params: Vec<RouteParamAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperience {
    pub name: String,
    pub imports: Vec<String>,
    pub views: Vec<LzxExperienceView>,
    pub resume_routers: Vec<LzxResumeRouter>,
    pub extensions: Vec<LzxViewExtension>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxResumeRouter {
    pub name: String,
    pub source_query: String,
    pub arms: Vec<LzxResumeArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxResumeArm {
    pub kind: LzxResumeArmKind,
    pub substep: Option<String>,
    pub target_view: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LzxResumeArmKind {
    State(String),
    None,
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperienceView {
    pub name: String,
    pub anchor: Option<String>,
    pub routes: Vec<String>,
    pub extensible_by: Vec<String>,
    pub source: Option<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub actions: Vec<LzxAction>,
    pub opens: Vec<String>,
    /// Wave 4 — typed view test assertions parsed from the `tests` block.
    /// Only `accepted by <feature>` / `rejected by <feature>` shapes are
    /// admissible; the parser rejects any other line as a `ParseError`.
    pub tests: Vec<LzxViewTestAssertion>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
}

/// Wave 4 — surface-AST mirror of `lazuli_ir::ViewTestAssertion`. The
/// analyzer lowers each variant 1:1 to the IR enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LzxViewTestAssertion {
    AcceptedBy { feature: String, span: Span },
    RejectedBy { feature: String, span: Span },
}

impl LzxViewTestAssertion {
    pub fn feature(&self) -> &str {
        match self {
            LzxViewTestAssertion::AcceptedBy { feature, .. }
            | LzxViewTestAssertion::RejectedBy { feature, .. } => feature,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            LzxViewTestAssertion::AcceptedBy { span, .. }
            | LzxViewTestAssertion::RejectedBy { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAction {
    pub name: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxViewExtension {
    pub anchor: String,
    pub blocks: Vec<String>,
    pub slots: Vec<LzxExtensionSlot>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionSlot {
    pub name: String,
    pub order: Option<LzxExtensionOrder>,
    pub blocks: Vec<String>,
    pub platforms: Vec<String>,
    pub audiences: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionOrder {
    pub relation: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxSurface {
    pub experience: String,
    pub platform: LzxPlatform,
    pub uses_experience: Option<String>,
    pub audiences: Vec<LzxAudience>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LzxPlatform {
    Web,
    Mobile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAudience {
    pub name: String,
    pub qualifiers: Vec<String>,
    pub views: Vec<LzxPlatformView>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxPlatformView {
    pub name: String,
    pub view_type: String,
    pub columns: Vec<String>,
    pub fields: Vec<String>,
    pub sections: Vec<String>,
    pub search: Vec<String>,
    pub filter: Vec<String>,
    pub cells: Vec<String>,
    pub actions: Vec<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
}

// =============================================================================
// L0 #3 — lzx ViewModel surface AST.
// -----------------------------------------------------------------------------
// Hand-written AST mirror for `features/<feat>/<feat>.{web,mobile}.lzx`
// files per `docs/proposals/lzx-integration-codegen.md` §5 (closed
// keyword catalog) + §5.1 (per-view-kind matrix). Field-level type
// references are kept as raw text; the analyzer lifts them to `ir::*`
// in `lower_surface`. Indentation-based parser populates this via
// `parse_surface_decl`.
// =============================================================================

/// A `.lzx` surface declaration — one per `<feat>.<target>.lzx` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceAst {
    /// `surface <feature> web|mobile` — first token after `surface`.
    pub feature: String,
    /// `web` or `mobile`. Validated at parse time.
    pub target: SurfaceTargetAst,
    /// Optional `uses feature <feature>` override. When absent, the
    /// surface's owning feature (derived from the enclosing file path)
    /// is assumed.
    pub uses_feature: Option<String>,
    pub audiences: Vec<AudienceAst>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceTargetAst {
    Web,
    Mobile,
}

/// `audience <name>` block inside a `.lzx` surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceAst {
    pub name: String,
    /// `requires @scope.<name>` (and reserved future `@role.<name>` /
    /// `@actor.<name>`) — one entry per `requires` line.
    pub requires: Vec<PolicyAtomAst>,
    pub views: Vec<ViewAst>,
    pub span: Span,
}

/// Closed view-kind catalog mirroring `ir::View`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewAst {
    List(ViewListAst),
    Detail(ViewDetailAst),
    Create(ViewCreateAst),
}

impl ViewAst {
    pub fn name(&self) -> &str {
        match self {
            ViewAst::List(v) => &v.name,
            ViewAst::Detail(v) => &v.name,
            ViewAst::Create(v) => &v.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewListAst {
    pub name: String,
    pub route: Option<String>,
    /// `source <feature>.query.<name>` — kept as raw `feature.query.name`
    /// text; analyzer splits into a `QueryRef`.
    pub source: String,
    pub columns: Vec<String>,
    pub search: Option<SearchDeclAst>,
    pub filter: Vec<String>,
    /// `filters` block declarations for typed view-local filter state.
    pub filters: Vec<FilterDeclAst>,
    /// `cells @client.<slot>` grid-row slot. `None` means the view either
    /// uses per-column `cells <field> @client.<slot>` bindings or no cells.
    pub cells_slot: Option<String>,
    pub cells: Vec<CellBindingAst>,
    pub drawer: Option<DrawerSubViewAst>,
    pub sort: Option<SortDeclAst>,
    pub selection: Option<SelectionDeclAst>,
    pub settings: Vec<SettingDeclAst>,
    /// `actions <cmd>, <cmd>` — comma-separated short names or qualified
    /// `<feature>.command.<name>` references. Analyzer normalizes.
    pub actions: Vec<String>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDeclAst {
    pub mode: SearchModeAst,
    pub fields: Vec<SearchFieldAst>,
    pub free_text_target: Option<BindingRefAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SearchModeAst {
    Columns(Vec<String>),
    Segmented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFieldAst {
    pub key: String,
    pub binds_to: BindingRefAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BindingRefAst {
    Filter { name: String },
    SourceInput { name: String },
    SelectionScalar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDetailAst {
    pub name: String,
    pub route: Option<String>,
    pub source: String,
    pub route_params: Vec<RouteParamAst>,
    pub sections: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    pub actions: Vec<String>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCreateAst {
    pub name: String,
    pub route: Option<String>,
    /// `submit <feature>.command.<name>` — qualified reference text.
    pub submit: String,
    /// `on_success` post-submit orchestration block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<OnSuccessSpecAst>,
    pub fields: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnSuccessSpecAst {
    pub back: bool,
    pub redirect: Option<String>,
    pub flash: Option<FlashSpecAst>,
    pub invalidates: Vec<InvalidatesDecl>,
    pub replace: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashSpecAst {
    pub kind: String,
    pub message_key: TranslationKeyRefAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerSubViewAst {
    pub name: String,
    pub trigger: DrawerTriggerAst,
    pub source: String,
    pub route_binding: Option<DrawerRouteBindingAst>,
    pub sections: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    pub actions: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerTriggerAst {
    Select,
    ManualOpen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerRouteBindingAst {
    pub target: String,
    pub source: DrawerBindingSourceAst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerBindingSourceAst {
    Selection,
}

/// `cells <field> @client.<slot>` parsed binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellBindingAst {
    pub field: String,
    /// Slot identifier (without the `@client.` prefix).
    pub slot: String,
    pub span: Span,
}

/// `filters` block field declaration inside `view list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterDeclAst {
    pub name: String,
    /// Raw label as authored; resolved during lowering.
    pub type_ref: String,
    pub cardinality: FilterCardinalityAst,
    pub url_sync: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCardinalityAst {
    Single,
    Multi,
}

/// `route <name>: <Type> from path` — typed path parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParamAst {
    pub name: String,
    pub type_ref: String,
    pub span: Span,
}

/// `sort` block inside a `view list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortDeclAst {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDirAst,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirAst {
    Asc,
    Desc,
}

/// `selection single|multi` plus optional `bulk_actions` folded in at
/// view assembly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionDeclAst {
    pub mode: SelectionModeAst,
    pub bulk_actions: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionModeAst {
    None,
    Single,
    Multi,
}

/// One child declaration inside a `settings` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDeclAst {
    pub name: String,
    pub value_space: SettingValueSpaceAst,
    pub default: String,
    pub persistence: SettingPersistenceAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingValueSpaceAst {
    Enum(Vec<String>),
    Bool,
    Int { min: Option<i64>, max: Option<i64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingPersistenceAst {
    None,
    Local,
    Workspace,
}

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

// =============================================================================
// Cut A — canonical-indent slice for `feature` skeletons and `agent` blocks.
//
// Sibling to `Document` (legacy brace MVP). The slice deliberately covers
// only `feature <name>` headers and indented `agent <name>` blocks plus
// their Cut A children (tools / evals / discriminated output). Other
// feature children (resources, commands, queries, workflows, ...) remain
// in the legacy pipeline until later cuts migrate them.
//
// See docs/proposals/ai-primitives-v0-implementation.md §3.2 / §3.4.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureSkeleton {
    pub name: String,
    pub agents: Vec<Agent>,
    /// Phase L — `auth` block. At most one per feature. Lowered into
    /// `ir::Auth` via the analyzer; the surface AST mirrors the IR
    /// shape so the only translation the analyzer performs is field
    /// resolution (`Customer.email` → `FieldRef`).
    pub auth: Option<Auth>,
    /// Phase L Tier 3 — `job <name>` blocks.
    pub jobs: Vec<Job>,
    /// Phase L Tier 3 — `webhook <name>` blocks.
    pub webhooks: Vec<Webhook>,
    /// Phase L Tier 3 — `notification <name>` blocks.
    pub notifications: Vec<Notification>,
    /// Phase L Tier 3 — `event_group <pattern> on <Resource>` blocks.
    pub event_groups: Vec<EventGroup>,
    /// Migrations bucket cycle Route C — `tenant_migration <name>`
    /// blocks. Mirrors `jobs` exactly: zero or more per feature.
    pub tenant_migrations: Vec<TenantMigration>,
    /// Phase L Tier 4a — `defaults` block. Optional; at most one per
    /// feature. Children captured: `tenancy <axis>`, `timestamps`,
    /// `policy_for <kinds>: <atom-list>`.
    pub defaults: Option<FeatureDefaults>,
    /// Phase L Tier 4b — `command <name>` blocks.
    pub commands: Vec<CommandDecl>,
    /// Phase L Tier 4b — `api <name>` blocks.
    pub apis: Vec<ApiDecl>,
    /// Phase L Tier 4c — `resource <Name>` blocks (authored inside
    /// `domain`).
    pub resources: Vec<ResourceDecl>,
    /// Phase L Tier 4d — `query.list` / `query.lookup` / `query.sql`
    /// declarations.
    pub queries: Vec<QueryDecl>,
    /// Phase L Tier 4d — `record <Name>` declarations (typed value
    /// records for projection outputs, distinct from resources).
    pub records: Vec<RecordDecl>,
    /// Phase L Tier 4 follow-up — `policies` block. At most one per
    /// feature. Lowered into `ir::Policies` via the analyzer.
    pub policies: Option<PoliciesDecl>,
    /// IR Error-Vocab (Cell PARSE-1) — `errors` block at indent 2.
    /// Carries the lowered `default hide`/`default expose`,
    /// `expose client 4xx <fields>`, `expose client 5xx <fields>`, and
    /// per-code `<code> message @translation.<key>` lines. At most one
    /// per feature; duplicate is a parse error. Lowered into
    /// `ir::FeatureErrors`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §2.C / §3.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<FeatureErrorsDecl>,
    /// Phase L Tier 4 follow-up — `enum <Name>` declarations
    /// (authored inside `domain`).
    pub enums: Vec<EnumDeclAst>,
    /// i18n bucket cycle — `translation` block. At most one per
    /// feature. Lowered into `ir::Translation` via the analyzer.
    pub translation: Option<TranslationDecl>,
    /// L0 #8 — `poller <name>` blocks (docs/proposals/poller-vocab.md).
    /// Closed catalog feature kind, parallel to `job` / `webhook` /
    /// `notification`. Lowered into `ir::Poller` via the analyzer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pollers: Vec<crate::parser::PollerBlockAst>,
    /// Report vocab — `report <name>` block(s). Static-column export
    /// declarations replacing `api + opaque handler`. See
    /// `docs/proposals/report-vocab.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<ReportDecl>,
    /// Realtime bucket cycle MVP — `channel <name>` block(s).
    /// Sibling slot of `notifications`/`pollers`. See
    /// `docs/proposals/bucket-realtime-cycle.md`. Closed body
    /// (three required children: `tenant_from`, `policy`, `payload`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<Channel>,
    /// Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
    /// profile declarations. Sibling slot of `notifications`/`channels`.
    /// Each entry models a named cache contract (key/ttl + optional
    /// namespace/tags/SWR/coalesce/sliding) that queries opt into via
    /// `cache <profile_name>`. The inline `cache { key, ttl }` shape on
    /// a query stays for one-off ttl/key pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caches: Vec<CacheProfileDecl>,
    /// CL.C.4 — `aggregate <Name>` block(s) (DDD consistency
    /// boundary). Sibling slot of `resources`/`commands`/`policies`.
    /// Each entry carries a root resource + closed contains list +
    /// cluster-spanning invariants. Lowered into `ir::Aggregate`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<AggregateDecl>,
    /// Feature-level `uses <feature>[, <feature>]+ [version v<N>]` lines.
    /// One entry per imported feature. Per
    /// `docs/proposals/cross-feature-contracts.md` §5.4 the optional
    /// `version v<N>` pin is consumer-side and gates the doctor
    /// `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` rule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses_clauses: Vec<UsesClauseAst>,
    /// MCP bucket cycle — feature-scoped `mcp_server <name>` blocks.
    /// Sibling slot of `notifications` / `channels` / `pollers`.
    /// Lowered into `ir::MCPServerSpec` via the analyzer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
    /// Iron-hand context-vocabulary — `purpose "<sentence>"` line.
    /// Single optional string anchoring the feature's intent. Surfaced
    /// by `VOCAB-CONTEXT-PURPOSE-001`. The `tdd-iron-hand` preset
    /// promotes the lint from warn to error. See
    /// `docs/canonical-semantics.md#feature-context-vocabulary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<LziFeaturePurpose>,
    /// Iron-hand context-vocabulary — `non_goals` block with one string
    /// per indented line. Empty list surfaces
    /// `VOCAB-CONTEXT-NONGOALS-001`. See
    /// `docs/canonical-semantics.md#feature-context-vocabulary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_goals: Option<LziFeatureNonGoals>,
    /// Iron-hand context-vocabulary — `attach_ctx "<relative-path>"`
    /// pointing at a markdown sidecar (e.g. `./ctx.md`). Missing,
    /// unreadable, or <100-char content surfaces
    /// `VOCAB-CONTEXT-CTXMD-001`. See
    /// `docs/canonical-semantics.md#feature-context-vocabulary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach_ctx: Option<LziFeatureAttachCtx>,
    pub span: Span,
}

/// Iron-hand `purpose "<sentence>"` line. The string is whatever the
/// author wrote between the quotes; empty / whitespace-only is allowed
/// at parse time so the lint can fire a precise diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeaturePurpose {
    pub text: String,
    pub span: Span,
}

/// Iron-hand `non_goals` block. Children are one-quoted-string-per-line
/// entries (mirrors how `uses` lists work for the wire-thin slice).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureNonGoals {
    pub entries: Vec<String>,
    pub span: Span,
}

/// Iron-hand `attach_ctx "<relative-path>"` line. Path is verbatim;
/// resolution against the project root happens in the doctor lint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureAttachCtx {
    pub path: String,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// RBAC catalog vocab — top-level `permission <ident>` and `role <name>`
// declarations. Package-scoped (sibling of `feature`); see
// `docs/proposals/rbac-catalog-vocab.md`.
// -----------------------------------------------------------------------------

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

/// Package-level skeleton produced by `parse_package_skeleton`. Carries
/// the per-feature skeletons plus any cross-feature top-level decls
/// (RBAC catalog so far). Other top-level kinds (`app`, `workspace`,
/// `contract`) remain on dedicated parsers; this slice is additive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageSkeleton {
    pub features: Vec<FeatureSkeleton>,
    /// Top-level `permission <ident>` decls (RBAC catalog).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<PermissionDeclAst>,
    /// Top-level `role <name>` decls (RBAC catalog).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<RoleDeclAst>,
}

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `policies` block surface AST.
//
// The `policies` block lives at indent 2 under the feature header. Its
// direct children are either:
//
//   * Named category atoms (indent 4): `create: @role.admin, @role.sales`.
//   * Per-resource field overrides (indent 4): `fields <Resource>` with
//     grandchild field names at indent 6 and `read:` / `write:` at indent 8.
//
// The IR shape (`ir::Policies` / `ir::PolicyCategory` / `ir::FieldPolicies`
// / `ir::FieldPolicy`) is mirrored 1:1 so lowering is structural.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoliciesDecl {
    pub categories: Vec<PolicyCategoryDecl>,
    pub fields: Vec<FieldPoliciesDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyCategoryDecl {
    pub name: String,
    /// Verbatim atom literals (`@role.admin`, `@scope.same_org`, ...).
    /// Atoms not prefixed with `@` are dropped silently — matches the
    /// retired `collect_policy_atoms` walker.
    pub atoms: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhenDeniedRouteAst {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unauthenticated: Option<RouteRedirectTargetAst>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_mismatch: Vec<RoleMismatchArmAst>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<RouteRedirectTargetAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleMismatchArmAst {
    pub role: String,
    pub target: RouteRedirectTargetAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RouteRedirectTargetAst {
    View(String),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPoliciesDecl {
    /// `fields <Resource>` — captured verbatim (qualifier-free identifier
    /// in the fixture).
    pub resource: String,
    pub fields: Vec<FieldPolicyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicyDecl {
    pub field: String,
    pub read: Option<Vec<String>>,
    pub write: Option<Vec<String>>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// IR Error-Vocab (Cell PARSE-1) — `errors` block surface AST.
//
// The `errors` block lives at indent 2 under the feature header. Closed
// children at indent 4:
//
//   * `default hide` / `default expose` — at most one.
//   * `expose client 4xx <comma-list>` — at most one.
//   * `expose client 5xx <comma-list>` — at most one.
//   * `<code> message @translation.<key>` — zero or more, one per
//     closed-catalog error code (`policy_denied`, `validation_failed`,
//     `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`,
//     `method_not_allowed`, `integration_error`). Closed-catalog
//     enforcement lives in the analyzer / doctor; the parser keeps the
//     code as a verbatim identifier so unknown codes surface as
//     `ERR-VOCAB-CODE-UNKNOWN` rather than a hard parse error.
//
// See `docs/proposals/ir-error-messages-vocab.md` §2.C / §3.4.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorsDecl {
    /// `default hide` | `default expose`. `None` defers to the runtime
    /// default (currently `Hide`). At most one entry per block.
    pub default: Option<ErrorExposureDefaultAst>,
    /// 4xx envelope-field exposure: `expose client 4xx <comma-list>`.
    /// Closed-catalog enforcement (allowed fields: `message`, `code`,
    /// `data`, `message_key`) lives on the analyzer / doctor side;
    /// parser keeps verbatim tokens. At most one line per block.
    pub exposure_4xx: Vec<String>,
    /// 5xx envelope-field exposure: `expose client 5xx <comma-list>`.
    /// Closed catalog (`code`, `data`) — `message` is intentionally
    /// excluded so 5xx stays framework-internal. At most one line.
    pub exposure_5xx: Vec<String>,
    /// `expose to @audience <name> <comma-list>` rows.
    pub audience_exposure: Vec<FeatureErrorExposeRuleDecl>,
    /// `error_redact <pattern>` rows. Pattern text is preserved
    /// verbatim except for surrounding quotes.
    pub redact_patterns: Vec<String>,
    /// `<code> message @translation.<key>` rows in source order.
    pub messages: Vec<FeatureErrorMessageDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorExposeRuleDecl {
    pub audience: Option<String>,
    pub fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorExposureDefaultAst {
    Hide,
    Expose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorMessageDecl {
    /// Verbatim error code identifier (e.g. `policy_denied`). Closed-
    /// catalog validation runs analyzer-side.
    pub code: String,
    /// The `@translation.<key>` reference.
    pub message: TranslationKeyRefAst,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `enum <Name>` declaration surface AST.
//
// Authored inside `domain` at indent 4. Variants at indent 6 are either
// bare identifiers (`free`) or `<name> = <value>` (storage value is `i64`
// or quoted string).
// -----------------------------------------------------------------------------

/// Cross-feature contract annotation per
/// `docs/proposals/cross-feature-contracts.md` §5.1. Appears as the
/// line `public contract <Symbol> as v<N>` IMMEDIATELY ABOVE the
/// declaration of `<Symbol>`. Captured during parse; the analyzer
/// resolves the version into the IR `PublicContract`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicContractDeclAst {
    /// Version number from `as v<N>`. Monotonic per symbol.
    pub version: u16,
    pub span: Span,
}

/// One `uses` clause: a cross-feature import with an optional version pin.
/// Authored at feature scope as `uses account` or `uses account version v1`.
/// Multiple comma-separated entries on one `uses` line yield multiple
/// `UsesClause` instances, each carrying its own optional pin.
///
/// Consumer-side pin per
/// `docs/proposals/cross-feature-contracts.md` §5.4 + the consumer-side-pin
/// follow-up. When `version` is `Some(N)`, the doctor
/// `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` rule checks each referenced
/// symbol's origin `public_contract.version` against `N`. When `None`,
/// the consumer floats with the origin's current version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsesClauseAst {
    pub feature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u16>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDeclAst {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub variants: Vec<EnumVariantDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariantDecl {
    pub name: String,
    /// `None` when no `= <value>` is authored. `Some(Integer(_))` for
    /// `<name> = <number>`; `Some(String(_))` for `<name> = "<text>"`.
    pub storage: Option<EnumStorageValueDecl>,
    /// Optional enum metadata parsed from
    /// `<variant>: label @translation.<key>, hint @translation.<key>, icon "<name>"`.
    /// Stored as opaque strings; validation against translation/icon catalogs
    /// belongs to app tooling/doctor, not the parser.
    pub label_key: Option<String>,
    pub hint_key: Option<String>,
    pub icon_key: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EnumStorageValueDecl {
    Integer(i64),
    String(String),
}

/// i18n bucket cycle — surface AST for a `translation` block. The
/// analyzer copies this into `ir::Translation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationDecl {
    /// `catalog "<path>"` — required. Carries `<locale>` placeholder.
    pub catalog: String,
    pub keys: Vec<TranslationKeyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKeyDecl {
    pub name: String,
    pub variants: Vec<TranslationVariantDecl>,
    pub plurals: Vec<TranslationPluralArmDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariantDecl {
    /// BCP-47 tag, e.g. `pt-BR`.
    pub locale: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArmDecl {
    /// CLDR plural category: `zero`, `one`, `two`, `few`, `many`, `other`.
    pub arm: String,
    pub variants: Vec<TranslationVariantDecl>,
}

/// i18n bucket cycle — surface AST for a `locale_negotiate` block.
/// Sits inside `api <name>` (per-endpoint override). The `app.runtime
/// unit api locale_negotiate` form is parsed by `app_manifest.rs` since
/// it lives on `app.lzi`. Both forms lower to `ir::LocaleNegotiate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleNegotiateDecl {
    pub source: Option<String>,
    pub strategy: Option<String>,
    pub fallback: Option<String>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// Phase L Tier 4a — feature-level `defaults` block.
//
// The `defaults` block declares feature-level inheritance for tenancy,
// timestamps, and policy. Resource-local declarations override these.
// The IR already carries `ir::Defaults`; this AST mirrors that shape so
// lowering is structural.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDefaults {
    /// `tenancy org`, `tenancy team`, `tenancy none`, or a custom axis.
    pub tenancy: Option<DefaultsTenancy>,
    /// `timestamps` declared verbatim. Absent when not authored.
    pub timestamps: bool,
    /// `policy_for jobs, webhooks: @actor.system` style entries. Each
    /// entry binds a list of construct kinds (`jobs`, `webhooks`,
    /// `commands`, ...) to a single policy atom.
    pub policy_for: Vec<DefaultsPolicyFor>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultsTenancy {
    /// `tenancy org`.
    Org,
    /// `tenancy team`.
    Team,
    /// `tenancy none` — explicit opt-out.
    None,
    /// `tenancy workspace` and similar custom identifiers.
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultsPolicyFor {
    /// Construct kinds the policy applies to (`jobs`, `webhooks`,
    /// `commands`, `apis`, etc.). Comma-separated in source.
    pub kinds: Vec<String>,
    /// The policy atom literal, e.g. `@actor.system`. Captured verbatim
    /// so the analyzer can decide between `PolicyRef::Atom` and other
    /// variants without re-parsing surface text.
    pub atom: String,
    pub span: Span,
}

// =============================================================================
// Phase L Tier 4b — `command` / `api` declarations and the shared
// declarative spine (`target`, `let`, `creates`/`updates`/`deletes`).
//
// The AST mirrors the IR shape as closely as practical so lowering is
// structural. Expressions inside `target.<query>(args)` and
// assignment RHS are captured as raw text and re-parsed by the analyzer
// — the parser's job is to determine block boundaries and the indent
// contract, not to type-check expressions.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `previously migrated <old>` (one entry per `previously` line).
    pub previously: Vec<String>,
    /// `route <name>: <Type>` slots.
    pub route: Vec<CommandRouteSlot>,
    /// `input` block — `Empty` when absent, `Short` for `input <ref>`
    /// shorthand, `Typed` for typed-name lists.
    pub input: CommandInputDecl,
    /// `policy @policy.<name>` atom. Captured verbatim.
    pub policy: Option<String>,
    /// RB.S6 — structured form of `policy <expr>` when the policy
    /// string includes `has_role` / `has_permission` / `authenticated`
    /// predicates or boolean combinators. Coexists with `policy` (raw)
    /// for back-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// IR Error-Vocab (Cell PARSE-1) — optional `when_denied
    /// @translation.<key>` child at indent 6 under the `policy` line.
    /// Highest-precedence step of the resolution chain (§2.A). Lowered
    /// into `ir::Command.policy_when_denied`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRefAst>,
    /// `rate_limit "<N per period per scope>"` declarations on the
    /// command. Per `ir-rate-limit-env-aware` (cell 1) the parser
    /// accepts one default line + any number of `in <env_list>`
    /// qualified lines and folds them into a single
    /// `RateLimitSpecAst`. The single-line source shape stays
    /// 100% backward-compatible.
    pub rate_limit: Option<RateLimitSpecAst>,
    /// `audit <subject>, <subject>, ...` line + optional `emit_to <group>` child.
    pub audit: Option<CommandAudit>,
    /// Cut A.9 `approval` block.
    pub approval: Option<CommandApproval>,
    /// `target query.<name>(args)` — at most one per command.
    pub target: Option<TargetExprDecl>,
    /// `let <name> = <expr>` bindings — order preserved.
    pub lets: Vec<LetBindingDecl>,
    /// `validate @validator.<name>(args)` lines. Doctor-only today; the
    /// surface keeps the verbatim invocation.
    pub validate: Vec<String>,
    /// `creates`/`updates`/`deletes` body. `None` for `returns`-only
    /// commands or commands with `handler` opt-outs (none in the
    /// fixture today).
    pub effect: Option<CommandEffectDecl>,
    /// `returns <TypeRef>` for pure request/response commands. Mutually
    /// exclusive with `effect`.
    pub returns: Option<String>,
    /// `handler "./..."` escape hatch — verbatim path literal. Mutually
    /// exclusive with the declarative body.
    pub handler: Option<JobHandler>,
    /// `emits <event>` lines with optional `from creates`/`from updates`
    /// suffix or assignment child block.
    pub emits: Vec<CommandEmit>,
    /// `triggers transition <name>[, <name>]` transition names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    /// `invalidates query.<name>(args?)` references.
    pub invalidates: Vec<InvalidatesDecl>,
    /// `calls <slot>.<op>` references inside the command body.
    pub external_calls: Vec<JobExternalCall>,
    /// Phase L Tier 4 follow-up — `timeout "<duration>"` literal.
    /// Mirrors `Job.timeout`. Adapter parses; surface keeps verbatim.
    pub timeout: Option<String>,
    /// Phase L Tier 4 follow-up — `retry <count> [backoff <strategy>]`.
    /// Mirrors `Job.retry`.
    pub retry: Option<JobRetry>,
    /// Phase L Tier 4 follow-up — `idempotency by <field>[, ...]`.
    /// Mirrors `Job.idempotency_by`.
    pub idempotency_by: Option<String>,
    /// `write_window by <path> within <duration_or_ref>`.
    pub write_window: Option<CommandWriteWindow>,
    /// `tests` block — captured as raw lines until a typed test grammar
    /// lands. The body is the indented child list, trimmed.
    pub tests: Vec<String>,
    /// OpenAPI bucket cycle — `deprecated [since ".." replacement <ref> sunset ".."]`.
    pub deprecated: Option<CommandDeprecatedDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandWriteWindow {
    pub by: String,
    pub within: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDeprecatedDecl {
    /// Authored `since "<version>"` — verbatim (semver, calendar, git-sha).
    pub since: Option<String>,
    /// Authored `replacement <ref>` — verbatim dotted ref or quoted URL.
    pub replacement: Option<String>,
    /// Authored `sunset "<YYYY-MM-DD>"` — verbatim ISO-8601 string.
    pub sunset: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRouteSlot {
    pub name: String,
    pub type_text: String,
    /// `from ctx.customer.id` default expression — verbatim text.
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "is_plain_command_route_slot_kind")]
    pub kind: CommandRouteSlotKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRouteSlotKind {
    Plain,
    OpaqueToken,
    SignedToken,
}

impl Default for CommandRouteSlotKind {
    fn default() -> Self {
        CommandRouteSlotKind::Plain
    }
}

fn is_plain_command_route_slot_kind(kind: &CommandRouteSlotKind) -> bool {
    matches!(kind, CommandRouteSlotKind::Plain)
}

/// `input` block: empty, short reference, or typed slot list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandInputDecl {
    /// `input` block absent.
    Empty,
    /// `input <name>` short form — one inline name. The analyzer resolves
    /// it against the command's local resource fields.
    Short(String),
    /// `input` block with typed children:
    ///
    ///   input
    ///     name: Text required
    ///     email: @semantic.Email @pii.contact required
    Typed(Vec<CommandInputSlot>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInputSlot {
    pub name: String,
    /// Raw type text including decorator chain — `@semantic.Email
    /// @pii.contact required` is parsed by the analyzer into `TypeRef`
    /// + modifiers.
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    /// L0 #3 §10 — inline constraints (`min N`, `max N`, `pattern
    /// STRING`, `between A and B`, `length N`, `in [...]`). Parser
    /// captures them; the analyzer validates combination rules and
    /// lifts into `ir::FieldConstraints`.
    #[serde(default, skip_serializing_if = "FieldConstraintsDecl::is_empty")]
    pub constraints: FieldConstraintsDecl,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAudit {
    /// `actor`, `target.id`, `input.<field>`, etc.
    pub subjects: Vec<String>,
    /// `emit_to <event_group>` — optional child.
    pub emit_to: Option<String>,
    /// `data_subject <field>` — optional child naming the affected
    /// resource field that identifies the data subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_subject: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_before: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_after: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_for: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandApproval {
    /// `required_when <predicate>` — verbatim predicate text.
    pub required_when: Option<String>,
    /// `by @role.<name>` or `by @actor.<name>` — single approver atom.
    pub by: String,
    /// `timeout "24h"` — duration literal (quotes stripped).
    pub timeout: Option<String>,
    /// `then deny | allow | escalate`.
    pub then: ApprovalThenDecl,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalThenDecl {
    Deny,
    Allow,
    Escalate,
}

/// `target query.<name>(args)` — at most one per command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetExprDecl {
    /// Qualified query reference. The parser keeps the dotted form
    /// (`customer.query.by_id` or `query.by_id`); the analyzer splits.
    pub query: String,
    /// `name: <expr>` pairs. The right-hand side is captured verbatim.
    pub args: Vec<TargetArgDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetArgDecl {
    pub name: String,
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetBindingDecl {
    pub name: String,
    /// Verbatim expression text. The analyzer re-parses against the
    /// canonical `Expr` AST.
    pub value: String,
    pub span: Span,
}

/// `creates X` / `updates X` / `deletes X` body. Carries the qualified
/// resource name + child assignments. `from_input` is true when the
/// command author writes `creates X from input` shorthand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEffectDecl {
    pub kind: CommandEffectKindDecl,
    /// Resource name. May be qualified (`customer.Customer`); the
    /// analyzer resolves against the local feature first.
    pub resource: String,
    pub from_input: bool,
    pub assignments: Vec<AssignmentDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandEffectKindDecl {
    Creates,
    Updates,
    Deletes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentDecl {
    pub field: String,
    /// Verbatim RHS text. The analyzer re-parses against `Expr`.
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEmit {
    /// `customer_created`, `customer_reassigned`, etc.
    pub name: String,
    /// `from creates` / `from updates` / `from deletes` suffix.
    pub from: Option<CommandEffectKindDecl>,
    /// Optional child `<key> = <expr>` lines (e.g.
    /// `emits customer_reassigned\n  to_owner_id = input.owner_id`).
    pub fields: Vec<AssignmentDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidatesDecl {
    /// Qualified query reference, e.g. `query.list` or
    /// `customer.query.by_id`.
    pub query: String,
    /// Named args, e.g. `id: route.id`.
    pub args: Vec<TargetArgDecl>,
    pub span: Span,
}


// =============================================================================
// Phase L Tier 4d — `query.list`, `query.lookup`, `query.sql`, and
// `record` declarations.
//
// Three query shapes with overlapping (but not identical) children.
// Records are simple typed-field bags (no constraints, no tenancy);
// they live under `domain` alongside resources and queries.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum QueryDecl {
    List(ListQueryDecl),
    Lookup(LookupQueryDecl),
    Sql(SqlQueryDecl),
}

impl QueryDecl {
    pub fn name(&self) -> &str {
        match self {
            QueryDecl::List(q) => &q.name,
            QueryDecl::Lookup(q) => &q.name,
            QueryDecl::Sql(q) => &q.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured form of `policy <expr>` when predicates
    /// (`has_role` / `has_permission` / `authenticated`) are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `modifier @query_modifier.<name>` reference.
    pub modifier: Option<String>,
    /// `params` block (typed slots).
    pub params: Vec<CommandInputSlot>,
    /// `scope override` flag — when set, the query opts out of feature
    /// default tenancy.
    pub scope_override: bool,
    /// `scope override\n  reason "..."` text.
    pub scope_reason: Option<String>,
    /// `scope override\n  deleted_at = nil` raw assignments captured
    /// for cross-check; not yet lowered to typed predicate.
    pub scope_assignments: Vec<String>,
    /// `scope` block (without `override`) — verbatim lines for now;
    /// the legacy lowering produces typed predicates.
    pub scope_lines: Vec<String>,
    /// `filters` block lines (`field when params.field`).
    pub filters: Vec<String>,
    /// `search params.<key> over <fields>` line with optional `mode contains`.
    pub search: Option<QuerySearch>,
    /// `cache` block — verbatim lines (inline shape).
    pub cache: Vec<String>,
    /// Cache bucket cycle (CL.C.3) — `cache <profile_name>` reference
    /// form. Single-line shape pointing at a feature-level `cache
    /// <name>` profile. Mutually exclusive with the inline `cache`
    /// block at parse time; the parser rejects the combination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_profile_ref: Option<String>,
    /// `paginate <N>` page size.
    pub paginate: Option<u32>,
    /// `order <field> <asc|desc>` declarations.
    pub order: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `by <field>: <Type>` keys. Authored on the same line as the
    /// header in the fixture (`query.lookup by_id by id: ID`).
    pub keys: Vec<LookupKey>,
    /// `filters` block — verbatim lines (`field = ctx.actor.X` form),
    /// same shape as ListQueryDecl.filters. Lowered into
    /// `ir::LookupQuery.filters` by the analyzer; codegen merges them
    /// with `keys` into `LookupBy` so a ctx-keyed lookup (e.g.
    /// `my_host` filtered by `user_id = ctx.actor.user_id`) round-trips
    /// through the runtime's RunLookup mechanism.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupKey {
    pub name: String,
    pub type_text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "SqlQueryKind::is_sql")]
    pub kind: SqlQueryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `params` block.
    pub params: Vec<CommandInputSlot>,
    /// `scope` block — verbatim lines.
    pub scope_lines: Vec<String>,
    /// `returns <Type>` declaration (required for SQL-backed queries).
    pub returns: String,
    /// `sql "./queries/<name>.sql"` path literal or `source @file.<name>.sql`.
    pub sql_path: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqlQueryKind {
    #[default]
    Sql,
    View,
}

impl SqlQueryKind {
    pub fn is_sql(&self) -> bool {
        matches!(self, Self::Sql)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuerySearch {
    /// `params.search` source path.
    pub source: String,
    /// `over name, email` list.
    pub fields: Vec<String>,
    /// `mode contains` (closed catalog — `contains` only today).
    pub mode: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub fields: Vec<ResourceFieldDecl>,
    /// `discriminator` field marker name when authored. Cut A.6 used
    /// `record` types with a discriminator field for tagged-union
    /// agent outputs.
    pub discriminator_field: Option<String>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// `api <name>` declaration.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiDecl {
    pub name: String,
    /// `method GET|POST|PUT|PATCH|DELETE`. Captured as a typed enum.
    pub method: HttpMethod,
    /// `path "/api/customers/export"` — verbatim path literal.
    pub path: String,
    /// `output <TypeRef>` — captured as raw type text. The analyzer
    /// projects to `TypeRef`.
    pub output: String,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `rate_limit "<N per period per scope>"` declarations on the
    /// api block. See `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    /// `handler "./api/<name>.go"`.
    pub handler: Option<String>,
    /// i18n bucket cycle — per-api `locale_negotiate` block override.
    pub locale_negotiate: Option<LocaleNegotiateDecl>,
    /// `route <name>: <Type>` slots — path placeholders bound to typed
    /// values. Captured verbatim; codegen currently materializes them
    /// as args inferred from the path string.
    #[serde(default)]
    pub route: Vec<CommandRouteSlot>,
    /// `input` block — typed body fields. Captured verbatim; codegen
    /// does not lower these yet (handler @fn.<name> reads the request
    /// body itself).
    #[serde(default)]
    pub input: Option<CommandInputDecl>,
    /// `deprecated` child block shared with commands.
    pub deprecated: Option<CommandDeprecatedDecl>,
    pub span: Span,
}







