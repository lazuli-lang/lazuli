use serde::{Deserialize, Serialize};

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
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRequiresLifecycle {
    pub resource: String,
    pub state: String,
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
    pub tests: Vec<String>,
    pub guard: Option<LzxViewGuard>,
    pub span: Span,
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
    pub fields: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
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
    pub span: Span,
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
// Phase L Tier 4c — `resource <Name>` declaration.
//
// Resources live inside `domain` at indent 4. Children at indent 6 are
// fields, `has_many`, `previously`, `soft_delete`, `retention`,
// `validates`. The legacy lowering pipeline already produces an
// `ir::Resource` from the brace MVP; the canonical-indent slice now
// produces one too via `lower_resource_decl`.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `previously migrated <old>` (one entry per `previously` line).
    pub previously: Vec<String>,
    /// `tenancy <axis>` resource-local override.
    pub tenancy: Option<DefaultsTenancy>,
    /// Field declarations (`<name>: <Type> [modifiers...]`).
    pub fields: Vec<ResourceFieldDecl>,
    /// `has_many <name>: <Resource> [inverse <field>]` lines.
    pub has_many: Vec<ResourceHasMany>,
    /// `soft_delete` declared verbatim.
    pub soft_delete: bool,
    /// `timestamps` declared verbatim.
    pub timestamps: bool,
    /// `retention <duration> then <action>` policy.
    pub retention: Option<ResourceRetention>,
    /// `validates @validator.<name>` and `validates resource "./..."`
    /// declarations. Captured as raw text; the analyzer dispatches
    /// between resource-level and field-level validators.
    pub validates: Vec<String>,
    /// Resource-owned state machine over one discriminator field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<crate::parser::LifecycleBlockAst>,
    /// CL.C.4 — standalone `invariant <name>` blocks declared as
    /// resource children. Each block carries a closed-catalog
    /// predicate (`when <expr>`) plus an authored `message`. Shared
    /// shape with `AggregateDecl.invariants`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<InvariantDecl>,
    /// Roadmap §1.5 (CL.C.2) — `lock` decorator. Closed catalog
    /// `optimistic`/`pessimistic`/`row_level`. At most one per resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<ResourceLock>,
    /// Roadmap §1.5 (CL.C.2) — `composite_key` block. Lists fields and
    /// an optional `primary true` flag indicating that the implicit
    /// `id BIGSERIAL PRIMARY KEY` should be replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_key: Option<ResourceCompositeKey>,
    /// `conventions [<name>, ...]` resource-level slot. Closed catalog
    /// of named convention bundles (today: `crud`). Empty when the
    /// resource opts into no conventions. See
    /// `docs/proposals/ir-resource-conventions-crud.md` §4.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conventions: Vec<ResourceConventionAst>,
    /// Resource-authored DDL declarations:
    /// `index on <field>`, `index on (<field>, ...) [using <method>]`,
    /// `unique (<field>, ...)`, and `fts on (<field>, ...)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<ResourceConstraintAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResourceConstraintAst {
    Index(ResourceIndexAst),
    Unique(ResourceUniqueAst),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceIndexAst {
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<ResourceIndexMethodAst>,
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub full_text: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUniqueAst {
    pub fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceIndexMethodAst {
    Btree,
    Gin,
    Gist,
}

/// Closed-catalog identifier inside a resource's `conventions [...]`
/// slot. Adding a variant is an IR/parser change requiring a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceConventionAst {
    Crud,
    Me,
}

/// Roadmap §1.5 (CL.C.2) — `lock` decorator closed catalog. Variant
/// data preserved so the analyzer can lift into `ir::LockSpec` and
/// doctor can cross-check the optimistic `version_field` against
/// `Resource.fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResourceLock {
    Optimistic { version_field: String },
    Pessimistic,
    RowLevel,
}

/// Roadmap §1.5 (CL.C.2) — `composite_key` block AST shape. The parser
/// walks the children of the `composite_key` line (`fields <list>`,
/// `primary true|false`) and produces this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCompositeKey {
    pub fields: Vec<String>,
    #[serde(default)]
    pub primary: bool,
    pub span: Span,
}

/// CL.C.4 — `aggregate <Name>` declaration block. DDD consistency
/// boundary: one `root` resource, a closed `contains` member list,
/// and zero-or-more invariants whose predicates span the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateDecl {
    pub name: String,
    /// `root <Resource>` — the consistency-boundary root.
    pub root: String,
    /// `contains <Resource>, <Resource>, ...` — comma-separated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contains: Vec<String>,
    /// `invariants` sub-block — zero or more `invariant <name>` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<InvariantDecl>,
    pub span: Span,
}

/// CL.C.4 — `invariant <name>` declaration. Shared by `ResourceDecl`
/// and `AggregateDecl` (both surfaces author identical syntax). The
/// `when` text is parsed by the analyzer into `ir::EvalPredicate`; the
/// AST keeps it verbatim so doctor can echo the source on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantDecl {
    pub name: String,
    /// `when <expr>` — verbatim predicate text (analyzer parses).
    pub when: String,
    /// `message "<text>"` — authored message body (empty when absent).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFieldDecl {
    pub name: String,
    /// Raw type text including decorator chain. The analyzer projects
    /// to `TypeRef` via `type_ref_from_text`.
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    pub unique: bool,
    /// CL.C.4 — `@slug` field decorator. When `true` the field is the
    /// resource's URL slug. Doctor enforces implicit uniqueness via
    /// `slug-uniqueness-implicit`. Captured at parse time as a typed
    /// modifier (sibling of `required`/`optional`/`unique`).
    #[serde(default)]
    pub slug: bool,
    /// `= <expr>` default value (verbatim).
    pub default: Option<String>,
    /// `derived from <expr>` computed-field expression (Phase L Tier 4c).
    pub derived_from: Option<String>,
    /// L0 #3 §10 — inline constraints (`min N`, `max N`, `pattern
    /// STRING`, `between A and B`, `length N`, `in [...]`). Parser
    /// captures them; the analyzer validates combination rules and
    /// lifts into `ir::FieldConstraints`.
    #[serde(default, skip_serializing_if = "FieldConstraintsDecl::is_empty")]
    pub constraints: FieldConstraintsDecl,
    /// Roadmap §1.5 (CL.C.2) — `@full_text` decorator marks this field
    /// for Postgres GIN tsvector index emission. Mutually compatible
    /// with `required`/`optional`/`unique` modifiers. The analyzer
    /// rejects `@full_text` on non-text-like types.
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub full_text: bool,
    /// `ir-resource-conventions-owner-scope` §7.1 — `@owner_axis(through:
    /// <ident>)` field annotation. Parser peels the decorator out of
    /// `type_text` and lifts it here so the analyzer projects directly
    /// into `ir::Field.owner_axis`. Absent = field carries no ownership
    /// chain, synth pass uses tenant-scope (today's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_axis: Option<OwnerAxisAst>,
    /// Child `previously migrated <old>` lines beneath the field.
    pub previously: Vec<String>,
    pub span: Span,
}

/// `ir-resource-conventions-owner-scope` §7.1 — AST-level mirror of
/// `ir::OwnerAxis`. `through_column` carries the bare identifier the
/// author wrote between the parens (e.g. `user` for Hostpoint's
/// `Property → Host → User` chain). String-literal arguments are a
/// parse error (per §7.1, the value is a syntactic identifier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerAxisAst {
    pub through_column: String,
}

fn is_false_bool(value: &bool) -> bool {
    !*value
}

/// L0 #3 §10 — parser-side capture of the 6 inline field constraints.
/// Mirrors `ir::FieldConstraints` but stays in the AST layer so the
/// analyzer can apply combination + default-compat checks before
/// projecting into the IR. `r#in` values are stored verbatim (no
/// surrounding quotes for string literals; numerics as their text
/// form) — the analyzer / emitters interpret per type.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FieldConstraintsDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<(i64, i64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub r#in: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitize_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utf8_safe: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recursion: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_pii: Option<String>,
}

impl FieldConstraintsDecl {
    pub fn is_empty(&self) -> bool {
        self.min.is_none()
            && self.max.is_none()
            && self.pattern.is_none()
            && self.between.is_none()
            && self.length.is_none()
            && self.r#in.is_none()
            && self.sanitize_html.is_none()
            && self.utf8_safe.is_none()
            && self.max_recursion.is_none()
            && self.max_size.is_none()
            && self.covers_pii.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHasMany {
    pub name: String,
    /// Resource type reference, e.g. `CustomerNote`.
    pub type_text: String,
    /// `inverse <field>` clause — captured verbatim.
    pub inverse: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRetention {
    /// Duration literal, e.g. `7y`, `30d`. Captured verbatim.
    pub duration: String,
    /// `Anonymize | Delete | Archive` closed catalog.
    pub action: ResourceRetentionAction,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRetentionAction {
    Anonymize,
    Delete,
    Archive,
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
    /// `returns <Type>` declaration (required for SQL queries).
    pub returns: String,
    /// `sql "./queries/<name>.sql"` path literal.
    pub sql_path: String,
    pub span: Span,
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

// -----------------------------------------------------------------------------
// Phase L — `auth` block (canonical-indent slice)
//
// `auth` declares the identity domain of a feature: a single identity
// field plus optional password / mfa / sessions / oauth subcontracts.
// One `auth` block per feature.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub identity: AuthIdentity,
    pub password: Option<AuthPassword>,
    pub sessions: Option<AuthSessions>,
    pub mfa: Option<AuthMfa>,
    pub oauth: Vec<AuthOAuthProvider>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIdentity {
    /// Raw source text `Customer.email`. Lowering splits into
    /// `FieldRef { resource, field }`.
    pub field: String,
    /// Cross-feature contract per `docs/proposals/cross-feature-contracts.md`
    /// §3.5 + §5.3. Authored as `public contract identity as v<N>`
    /// IMMEDIATELY ABOVE the `auth identity <Resource>.<field>` line.
    /// Singleton (one identity per feature) so no per-name binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPassword {
    /// `algorithm argon2id` — required.
    pub algorithm: String,
    /// `hash @fn.<name>` — extension fn reference.
    pub hash: String,
    /// `verify @fn.<name>` — extension fn reference.
    pub verify: String,
    /// `rate_limit "5 per 10 minutes"` — optional declarative throttle.
    /// Env-aware per `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessions {
    /// `resource CustomerSession` — name only; analyzer resolves the
    /// resource against the feature's domain.
    pub resource: String,
    /// `ttl "7 days"` — duration string parsed by the adapter.
    pub ttl: String,
    /// `refresh true|false` — legacy placeholder retained for back-compat.
    /// When omitted, lowering treats it as `false`.
    pub refresh: bool,
    /// `access_ttl "15 minutes"` — optional short-lived access-token TTL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_ttl: Option<AuthDurationClause>,
    /// `rotation` nested block. Presence enables refresh-token rotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<AuthSessionRotation>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthDurationClause {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionRotation {
    /// `refresh_ttl "30 days"` — optional; IR defaults when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ttl: Option<AuthDurationClause>,
    /// `grace "30 seconds"` — optional; IR defaults when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace: Option<AuthDurationClause>,
    /// `theft_detection_action <verb>` — optional closed catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theft_detection_action: Option<AuthTheftDetectionActionClause>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthTheftDetectionActionClause {
    pub action: AuthTheftDetectionAction,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthTheftDetectionAction {
    RevokeSessionFamily,
    RevokeUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMfa {
    /// MFA method id, e.g. `totp`, `sms`, `webauthn`. Adapter-specific
    /// beyond this.
    pub method: String,
    /// `enroll @fn.<name>` — required extension fn reference.
    pub enroll: String,
    /// `verify @validator.<name>` or `@fn.<name>` — required.
    pub verify: String,
    /// `adapter @adapter.<name>` — optional adapter reference.
    pub adapter: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOAuthProvider {
    /// Provider id, e.g. `google`, `github`, `microsoft`.
    pub provider: String,
    /// `adapter @adapter.<provider>_oauth` — required.
    pub adapter: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub input: Vec<AgentInputSlot>,
    pub context: Option<String>,
    pub policy: Option<Vec<String>>,
    /// `rate_limit "<N per period per scope>"` declarations on the
    /// agent. Env-aware per `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    pub output: Option<AgentOutput>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    pub seed: Option<i64>,
    pub prompt: Option<String>,
    pub safety: Vec<String>,
    pub tools: Vec<AgentTool>,
    pub evals: Vec<AgentEvalCase>,
    /// Cut A.7 — `expose http` block. Auto-mounts the agent as an
    /// HTTP endpoint; the agent's policy / rate_limit / output apply
    /// to the exposed surface.
    pub expose: Option<AgentExpose>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExpose {
    pub method: HttpMethod,
    pub path: String,
    pub route_slots: Vec<AgentExposeRouteSlot>,
    pub audience: Option<String>,
    pub rate_limit_override: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExposeRouteSlot {
    pub name: String,
    pub type_text: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// Parse a canonical uppercase method token. Returns `None` on
    /// unknown tokens — callers turn that into a `ParseError`.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInputSlot {
    pub name: String,
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum AgentOutput {
    /// `output stream <Type>` — streaming output of the named type.
    Stream(String),
    /// `output discriminator <Enum>` — single enum-variant output.
    Discriminator(String),
    /// `output <Type>` — bare type reference. Disambiguated at lowering:
    /// records with a `discriminator` marker field become DiscriminatedRecord;
    /// everything else becomes Text (legacy form, soft-warned per Q-impl-5).
    Plain(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTool {
    /// Canonical source text: `customer.query.by_id`, `@tool.web_search`,
    /// `query.by_id` (local shorthand). Lowering qualifies and resolves.
    pub reference: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalCase {
    pub name: String,
    pub assertions: Vec<AgentEvalAssertion>,
    /// Cut A.10 — optional `golden "./path.jsonl" min_score N`
    /// reference. The runtime adapter loads the file and scores the
    /// agent's output against it; `min_score` (0.0–1.0) is the gate
    /// threshold. Language stays out of the scoring algorithm.
    pub golden: Option<AgentEvalGolden>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalGolden {
    /// File path captured verbatim. The runtime resolves it.
    pub path: String,
    /// Optional `min_score N` threshold (0.0..=1.0). The default
    /// when omitted is 0.85 by adapter convention; language pins
    /// only what the author wrote.
    pub min_score: Option<f64>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalAssertion {
    pub kind: AgentEvalKind,
    pub predicate: AgentEvalPredicate,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvalKind {
    Requires,
    Forbids,
}

/// Parser-level eval predicate. Captures the three shapes the EBNF (§14)
/// allows inside `requires` / `forbids`:
///
/// - the closed predicate language (recorded verbatim for lowering),
/// - `<ref> contains <STRING | @semantic.Type>`,
/// - `tools.calls includes|excludes <tool-ref>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AgentEvalPredicate {
    /// Source text passed through to lowering, which re-parses against the
    /// canonical predicate AST. The parser captures the raw form here so
    /// any predicate-language extensions land without churn in this crate.
    Closed {
        text: String,
    },
    Contains {
        lhs: String,
        rhs: ContainsRhs,
    },
    ToolsCalls {
        op: ToolsCallsOp,
        target: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ContainsRhs {
    /// `requires output contains "active"` — substring literal match.
    Literal(String),
    /// `forbids output contains @semantic.Email` — semantic-type membership.
    /// Validation dispatches at `lazuli test --evals`, never at check-time.
    SemanticType(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolsCallsOp {
    Includes,
    Excludes,
}

// =============================================================================
// Phase L Tier 3 — job / webhook / notification / event_group skeletons.
//
// All four constructs are feature children authored at
// AGENT_INDENT_FEATURE_CHILD (2 spaces). Their grandchildren mirror the IR
// shapes (`ir::Job`, `ir::Webhook`, `ir::Notification`, `ir::EventGroup`)
// so lowering is structural.
//
// Route C (`docs/proposals/phase-l-tier-3-job-effect-scope.md:292-348`):
// declarative-body grammar (`target query.by_id(...)`, `let new_score = ...`,
// `updates Customer ... emits ...`) is captured as raw strings until Tier 4
// lifts the shared declarative spine alongside `parse_command`. Handler-backed
// bodies (`handler "./..."`) lower fully.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    /// `queue customer_imports` — execution lane for queued workers.
    pub queue: Option<String>,
    /// `tenant_from payload.<axis>_id` — path captured verbatim.
    pub tenant_from: Option<String>,
    /// `fanout tenants <axis>` — scheduled-job fanout directive.
    pub fanout: Option<JobFanout>,
    /// `idempotency by <path>` — path captured verbatim.
    pub idempotency_by: Option<String>,
    /// `retry <count> backoff <strategy>` — pair captured directly.
    pub retry: Option<JobRetry>,
    /// `policy @policy.<...>` — captured verbatim for lowering.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `timeout "30s"` — adapter-parsed duration literal.
    pub timeout: Option<String>,
    /// `calls <slot>.<op>` blocks lifted as `ExternalCallRef` shapes.
    pub external_calls: Vec<JobExternalCall>,
    /// Body of the job. Handler-backed bodies fully lower; declarative
    /// bodies stay as raw lines until Tier 4.
    pub body: JobBody,
    /// `emits <event>` lines. Each is one event name (qualified or not).
    pub emits: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.activated`.
    Event(String),
    /// `trigger schedule "0 2 * * *"`.
    Schedule(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFanout {
    /// `tenants` — closed scope catalog today.
    pub scope: String,
    pub axis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRetry {
    pub count: u32,
    /// `fixed` or `exponential` — closed strategy catalog today.
    pub backoff: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCall {
    pub slot: String,
    pub op: String,
    /// `arg_name = path.expr` pairs captured verbatim. Tier 4 lifts
    /// the right-hand-side expressions.
    pub args: Vec<JobExternalCallArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCallArg {
    pub name: String,
    /// Right-hand side captured verbatim until Tier 4.
    pub value: String,
    pub span: Span,
}

/// Body of a job. `Handler` is a path reference; `Declarative` is the
/// typed spine (Phase L Tier 4b lifted; previously a raw-line carve-out
/// in `JobDeclarativeRaw`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarativeTyped),
    /// No `handler` and no `target` / `updates` / `creates` / `deletes`
    /// authored. Some fixture jobs ship only `emits` (event reactors
    /// with no declarative body); analyzer treats this as a parse error
    /// only when neither effect nor emits is declared.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHandler {
    /// `"./jobs/process_import.go"` — quotes stripped.
    pub path: String,
    /// Optional `returns <Type>` suffix.
    pub returns: Option<String>,
}

/// Phase L Tier 4b — declarative job body using the typed spine helpers
/// (`TargetExprDecl`, `LetBindingDecl`, `CommandEffectDecl`). Replaces
/// the Tier 3 `JobDeclarativeRaw` carve-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarativeTyped {
    pub target: Option<TargetExprDecl>,
    pub lets: Vec<LetBindingDecl>,
    pub effect: Option<CommandEffectDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub name: String,
    /// `path "/webhooks/..."` — raw HTTP route literal.
    pub route: String,
    /// `verify hmac sha256` + nested `secret`/`header`. Required.
    pub verify: WebhookVerify,
    /// `tenant_from payload.<axis>_id` — path captured verbatim.
    pub tenant_from: Option<String>,
    /// `scope global` declaration — set when the provider doesn't send
    /// a tenant key and the handler reconciles the tenant from another
    /// source (e.g. external_reference lookup). Closes WAR-VOCAB-WEBHOOK-01.
    /// Requires a paired `reason` line so the operator-of-record can
    /// audit why this webhook escapes the standard tenant-from invariant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_global: Option<WebhookScopeGlobal>,
    /// `idempotency by <path>` — captured verbatim.
    pub idempotency_by: Option<String>,
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `handler "./..."` — required for canonical webhooks today.
    pub handler: Option<WebhookHandler>,
    /// `emits <event>` lines — flat list of event names (back-compat
    /// shape). Populated even when per-branch predicates are authored
    /// so the existing doctor / cross-feature pipeline stays oblivious.
    pub emits: Vec<String>,
    /// B5 framework gap 2 — per-branch `emits ... when <predicate>`
    /// bindings. Parallel to `emits`: `emits_predicates[i]` is the
    /// `when` predicate authored on `emits[i]` (or `None` when no
    /// predicate was authored). When every entry is `None` the
    /// surface is unchanged from the flat shape; when any predicate
    /// is present the codegen wires a dispatch table on the
    /// generated webhook contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits_predicates: Vec<Option<String>>,
    /// Webhooks expanded cycle — `payload from webhook_events.<name>`
    /// (verbatim suffix after `webhook_events.`). `None` when the
    /// inbound webhook does not declare a typed envelope yet.
    pub payload_from: Option<String>,
    /// Webhooks expanded cycle — `replay` block (short or long form).
    pub replay: Option<WebhookReplay>,
    /// Webhooks expanded cycle — `dlq` block (three closed variants).
    pub dlq: Option<WebhookDlq>,
    /// Webhooks expanded cycle — `retry <count> backoff <strategy>`
    /// inbound retry policy. Reuses the jobs-side `JobRetry` shape so
    /// codegen and doctor diagnostics stay single-pathed (Atrito #5
    /// of the canonical proposal).
    pub retry: Option<JobRetry>,
    pub span: Span,
}

/// Webhooks expanded cycle — surface form of `replay` on an inbound
/// webhook.
///
/// Short form: `replay allow within "24h"` (single line).
/// Long form: a `replay` header with nested `allow`/`deny` + `within
/// "..."` + optional `dedupe by <path>` children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookReplay {
    /// `allow` or `deny` — closed catalog enforced by the parser.
    pub mode: String,
    /// `within "<duration>"` — quoted duration verbatim.
    pub within: Option<String>,
    /// `dedupe by <path>` — path expression captured verbatim. `None`
    /// reuses the webhook's `idempotency by ...` path.
    pub dedupe_by: Option<String>,
    pub span: Span,
}

/// Webhooks expanded cycle — surface form of `dlq` on an inbound
/// webhook. The parser fails if more than one variant is authored on
/// the same webhook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookDlq {
    /// `dlq emit <event>` — publish a tombstone event after retry
    /// exhaustion.
    Emit { event: String, span: Span },
    /// `dlq handler "./path.go"` — adapter-side handler.
    Handler { path: String, span: Span },
    /// `dlq drop reason "..."` — explicit waiver. Mirrors `verify
    /// none reason "..."`.
    Drop { reason: String, span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookVerify {
    /// `hmac` — closed scheme catalog today.
    pub scheme: String,
    /// `sha256`, etc. — adapter-parsed algorithm token.
    pub algorithm: String,
    /// `secret env.<NAME>` — env binding for the shared secret.
    pub secret_env: Option<String>,
    /// `header "X-..."` — quoted header literal.
    pub header: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookHandler {
    pub path: String,
    pub returns: Option<String>,
}

/// `scope global` declaration on an inbound webhook (WAR-VOCAB-WEBHOOK-01
/// closure). The webhook is intentionally allowed to escape the
/// standard `tenant_from payload.<axis>_id` invariant because the
/// provider doesn't send a tenant key in the payload. The required
/// `reason` is an authored explanation captured for audit + doctor
/// surfaces so the operator-of-record sees why this exception exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookScopeGlobal {
    /// Quoted reason text (parser strips quotes). MUST be non-empty.
    pub reason: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub name: String,
    /// `channel email, in_app` — comma-split list.
    pub channels: Vec<String>,
    /// `recipient target.email` — path captured verbatim.
    pub recipient: String,
    /// `trigger event ...` or `trigger schedule "..."`.
    pub trigger: JobTrigger,
    /// `tenant_from payload.<axis>_id`.
    pub tenant_from: Option<String>,
    /// `idempotency by <path>`.
    pub idempotency_by: Option<String>,
    pub retry: Option<JobRetry>,
    /// `template "./outreach/welcome.mjml"`.
    pub template: String,
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    pub emits: Vec<String>,
    /// Notifications expanded bucket cycle — optional `digest` sub-block.
    /// Captured verbatim from the canonical-indent slice and lowered to
    /// `ir::NotificationDigest` in the analyzer.
    pub digest: Option<NotificationDigest>,
    /// Notifications expanded bucket cycle — optional `throttle` sub-block.
    /// Distinct from scalar `rate_limit`; lowered to
    /// `ir::NotificationThrottle` in the analyzer.
    pub throttle: Option<NotificationThrottle>,
    pub span: Span,
}

/// Notifications expanded bucket cycle — AST sidecar for the `digest`
/// sub-block. Fields are captured verbatim; closed-catalog validation
/// for `template_strategy` and the `every`/`max_size` shape lives in
/// the analyzer/doctor layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDigest {
    /// `every "<duration>"` — required.
    pub every: String,
    /// `group_by <path>` — optional.
    pub group_by: Option<String>,
    /// `max_size <N>` — optional.
    pub max_size: Option<u32>,
    /// `template_strategy <merge|append>` — optional.
    pub template_strategy: Option<String>,
    pub span: Span,
}

/// Realtime bucket cycle MVP — `channel <name>` AST surface.
///
/// Closed three-child body: `tenant_from <axis>`, `policy @policy.<name>`,
/// `payload <RecordType>`. All three are required; missing any one
/// yields a parse error at lowering time so authors get a precise
/// diagnostic. Optional children (audit, rate_limit, broadcast wiring)
/// are deferred per `docs/proposals/bucket-realtime-scope.md` pending
/// pilot evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    /// `tenant_from <axis>` — axis name verbatim.
    pub tenant_from: String,
    /// `policy @policy.<name>` — verbatim atom.
    pub policy: String,
    /// `payload <RecordType>` — verbatim type-name reference.
    /// Doctor `CHANNEL-PAYLOAD-001` resolves it.
    pub payload: String,
    pub span: Span,
}

/// Cache bucket cycle (CL.C.3) — feature-level `cache <name>` profile
/// AST. Required body children: `key <expr>`, `ttl <literal-or-prose>`.
/// Optional: `namespace <label>`, `tags <l1>[, <l2>...]`,
/// `stale_while_revalidate <literal>`, `coalesce <bool>`, `sliding <bool>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheProfileDecl {
    /// Profile identifier (lowercase, dashes allowed).
    pub name: String,
    /// `key <expr>` — opaque template stored verbatim.
    pub key: String,
    /// `ttl <literal>` — raw token (e.g. `5m`, `"5 minutes"`).
    /// Lowering parses it into the typed `CacheTtl` enum.
    pub ttl: String,
    /// `namespace <label>` — single label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `tags <l1>[, <l2>, ...]` — comma-separated labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `stale_while_revalidate <literal>` — raw token; lowered into
    /// `CacheTtl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_while_revalidate: Option<String>,
    /// `coalesce <bool>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce: Option<bool>,
    /// `sliding <bool>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sliding: Option<bool>,
    pub span: Span,
}

/// Notifications expanded bucket cycle — AST sidecar for the
/// `throttle` sub-block. Distinct keyword from scalar `rate_limit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationThrottle {
    /// `max_per "<duration>"` — required.
    pub max_per: String,
    /// `per_recipient` flag — bare child line.
    pub per_recipient: bool,
    /// `per_channel` flag — bare child line.
    pub per_channel: bool,
    /// `burst <N>` — optional.
    pub burst: Option<u32>,
    pub span: Span,
}

// =============================================================================
// MCP bucket (`docs/proposals/bucket-mcp-cycle.md`) — feature-scoped surface
// for declaring an MCP server endpoint. Sibling of `notification` / `channel`
// / `poller` at feature scope.
//
// L0 surface design (closed children): `transport`, `scope`, `auth`,
// `metadata <block>`, `tool <name> <block>`, `resource <name> <block>`,
// `prompt <name> <block>`. The AST captures structured shape so doctor +
// codegen + analyzer can lint and emit against it.
// =============================================================================

/// MCP server endpoint declared inside a feature. Lowered to
/// `ir::MCPServerSpec`. Per the proposal, closed-catalog children:
/// transport / scope / auth / metadata / tool / resource / prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    /// `transport stdio | http_sse | http_streamable` — required, closed catalog.
    pub transport: String,
    /// `scope feature.<name>` — required. Captured as the qualified path.
    pub scope_feature: Option<String>,
    /// `auth bearer env.<NAME>` — optional. Stored verbatim ("bearer env.X").
    pub auth: Option<String>,
    /// `metadata` sub-block contents.
    pub metadata: McpServerMetadata,
    /// `tool <name>` declarations.
    pub tools: Vec<McpTool>,
    /// `resource <name>` declarations.
    pub resources: Vec<McpResource>,
    /// `prompt <name>` declarations.
    pub prompts: Vec<McpPrompt>,
    pub span: Span,
}

/// `metadata` sub-block of an `mcp_server` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct McpServerMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

/// `tool <name>` sub-block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    /// `params` sub-block — list of `<name>: <type> [required|optional]`.
    pub params: Vec<McpParam>,
    /// `returns <KindRef>` — optional reference to a `kind` declared
    /// elsewhere in the app. Stored verbatim; analyzer resolves.
    pub returns: Option<String>,
    /// `handler @fn.<name>` — required.
    pub handler: String,
    /// `policy @policy.<name>` — optional.
    pub policy: Option<String>,
    pub span: Span,
}

/// `resource <name>` sub-block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResource {
    pub name: String,
    /// `uri_template "..."` — required.
    pub uri_template: String,
    /// `mime "..."` — optional; defaults to "application/octet-stream"
    /// at codegen time when absent.
    pub mime: Option<String>,
    /// `handler @fn.<name>` — required.
    pub handler: String,
    /// `policy @policy.<name>` — optional.
    pub policy: Option<String>,
    pub span: Span,
}

/// `prompt <name>` sub-block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: Option<String>,
    /// `params` sub-block (same shape as `McpTool.params`).
    pub params: Vec<McpParam>,
    /// `template "./path/to.tmpl"` — required.
    pub template: String,
    pub span: Span,
}

/// One row inside a `params` sub-block on a tool or prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpParam {
    pub name: String,
    /// Type literal verbatim (`string`, `int`, `enum [...]`, etc.).
    /// The analyzer maps to `ir::ParamType` via `parse_param_type`.
    pub ty: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventGroup {
    /// `customer_*` glob pattern.
    pub pattern: String,
    /// `on Customer` — owning resource type.
    pub on_resource: Option<String>,
    /// `payload` child lines captured verbatim.
    pub payload: Vec<String>,
    /// `audit ...` line captured verbatim.
    pub audit: Option<String>,
    /// Concrete `event <name>` headers under this group, recorded as
    /// name strings. The full event bodies stay in the legacy lowering
    /// pipeline; this slot drives doctor's pattern-prefix rule.
    pub events: Vec<String>,
    /// EVENT-OUTBOX §3.3 — parallel to `events`: `true` at index `i`
    /// when the corresponding `event <name>` block authored
    /// `outbox guaranteed`. Length always matches `events.len()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events_outbox_guaranteed: Vec<bool>,
    /// B5 framework gap 1 — per-event typed payload field bodies.
    /// Parallel to `events`: `event_variants[i]` holds the typed-field
    /// rows authored under `events[i]`. Each entry is an
    /// `EventVariantFieldDecl` (name + type-literal + required/optional).
    /// When an event was authored without a field body, the inner Vec
    /// is empty (preserves back-compat with the `event foo` shorthand).
    /// Lifted into typed `EventVariant` records by the analyzer; see
    /// `docs/proposals/event-group-per-variant-payload.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_variants: Vec<Vec<EventVariantFieldDecl>>,
    /// B5 framework gap 1 — parallel to `events`: closed catalog of
    /// the keyword authored on the event header. Distinguishes
    /// `event <name>` (Committed) from `event.trace <name>` (Trace) so
    /// the analyzer can lower into the correct `EventKind`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_variant_kinds: Vec<EventVariantKindAst>,
    pub span: Span,
}

/// B5 framework gap 1 — per-event variant kind on the AST surface.
/// Mirrors the `ir::EventKind` catalog so the parser stays decoupled
/// from the IR while the analyzer can lift losslessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVariantKindAst {
    /// Authored as `event <name>` — committed bus variant.
    Committed,
    /// Authored as `event.trace <name>` — trace-only variant.
    Trace,
}

/// B5 framework gap 1 — a single typed field row inside an
/// `event_group`'s `event <name>` body. Mirrors the surface shape of
/// `ResourceFieldDecl` but keeps the slot count minimal because event
/// payloads are projection-only (no defaults, no constraints, no
/// `unique`/`slug`/`@full_text`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventVariantFieldDecl {
    /// Field name as authored.
    pub name: String,
    /// Type literal verbatim (`Text`, `@semantic.Money`, `ID`, ...).
    /// Lifted to `ir::TypeRef` via `type_ref_from_syntax` on lowering.
    pub type_text: String,
    /// `required` modifier authored.
    pub required: bool,
    /// `optional` modifier authored.
    pub optional: bool,
    pub span: Span,
}

/// Migrations bucket cycle Route C — `tenant_migration <name>` AST
/// surface. Mirrors `Job`'s spine subset (no body styles, no `emits`,
/// no `policy`): a tenant migration is by design pure schema work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigration {
    pub name: String,
    /// `target query.<name>` / `target command.<name>` — required by the
    /// current surface. The legacy `target tenants <axis>` form leaves this
    /// unset and stores the axis below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    /// `axis <name>` or legacy `target tenants <axis>` — required.
    pub target_axis: String,
    /// `idempotency <path>` / legacy `idempotency by <path>` — mandatory; stored
    /// as `Option<String>` so the parser surfaces the absence as an
    /// IR-level diagnostic rather than a parse error (matches `Job`).
    pub idempotency_by: Option<String>,
    /// `retry <count> backoff <strategy>` — optional.
    pub retry: Option<JobRetry>,
    /// `timeout "<duration>"` — optional adapter-parsed literal.
    pub timeout: Option<String>,
    /// `handler "<path>"` — required path to the Go handler.
    pub handler: String,
    pub span: Span,
}

// =============================================================================
// L0 #2 — `design.lzi` declaration AST.
//
// Parser shape: every value is preserved as raw text (hex strings, rem/px
// literals, font stacks, cubic-bezier strings, weight integers as text)
// so the analyzer can apply lowering-time validation (hex regex check,
// shadow single-layer check, extends rejection, integer parsing for z).
// The IR mirror in `lazuli_ir::Design` is the typed surface.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDeclAst {
    pub name: String,
    /// `extends <name>` — captured if present (lowering rejects in v0).
    pub extends: Option<String>,
    pub colors: Vec<ColorTokenAst>,
    pub typography: TypographyAst,
    pub spaces: Vec<ScaleTokenAst>,
    pub radii: Vec<ScaleTokenAst>,
    pub shadows: Vec<ShadowTokenAst>,
    pub motion: MotionAst,
    pub breakpoints: Vec<ScaleTokenAst>,
    pub z_indices: Vec<ZTokenAst>,
    /// L0 #2 — 9th meta-group `custom` per `docs/proposals/design-tokens-custom.md`.
    /// Flat sub-grammar (no state sub-blocks). Lowering enforces hex validity
    /// + reserved-name + collision diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<CustomTokenAst>,
    pub span: Span,
}

/// L0 #2 — single `custom` entry: `<kebab-name> "<hex>" [dark "<hex>"]`.
/// Verbatim values; lowering validates hex shape + reserved-name policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTokenAst {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorTokenAst {
    pub name: String,
    pub states: Vec<ColorStateAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorStateAst {
    /// One of `base | hover | active | foreground`. The analyzer maps
    /// to `ir::ColorStateKind`; unknown names raise a lowering error.
    pub kind: String,
    /// Hex literal verbatim, e.g. `"#7c3aed"`.
    pub value: String,
    /// Optional `dark <hex>` suffix, verbatim.
    pub dark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypographyAst {
    pub families: Vec<FamilyTokenAst>,
    pub scale: Vec<TextScaleTokenAst>,
    pub weights: Vec<WeightTokenAst>,
    pub tracking: Vec<TrackingTokenAst>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleTokenAst {
    pub name: String,
    pub size: String,
    pub line_height: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightTokenAst {
    pub name: String,
    /// Weight literal as text (lowering parses to u16).
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MotionAst {
    pub durations: Vec<ScaleTokenAst>,
    pub easings: Vec<EasingTokenAst>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EasingTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZTokenAst {
    pub name: String,
    /// Integer literal as text (lowering parses to i32).
    pub value: String,
}

// =============================================================================
// Plan & Gate vocabulary (PG.A — `docs/proposals/plan-and-gate-vocab.md`).
// -----------------------------------------------------------------------------
// `plan <name>` is a top-level package-wide block (sibling of `feature`).
// `subscription resource <feature>.<field>` is a child of `app.lzi` (parsed
// in `crates/lazuli_cli/src/app_manifest.rs`).
// `gate behind plan.feature: ...` / `gate quota plan.limit: ...` are
// captured by the parser as a side-channel `Vec<GateDirectiveAst>` per
// callable kind. The analyzer (PG.B) reads them via a sibling pass so
// existing IR struct literals stay unchanged.
// =============================================================================

/// Top-level `plan <name>` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanBlockAst {
    pub name: String,
    pub features: Vec<PlanFeatureRefAst>,
    pub limits: Vec<PlanLimitRefAst>,
    pub trial: Option<PlanTrialAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanFeatureRefAst {
    Ident(String),
    CrossPlan(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanLimitRefAst {
    Integer { name: String, value: u64 },
    Unlimited { name: String },
    CrossPlan(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanTrialAst {
    pub duration: String,
    pub then_plan: String,
    pub span: Span,
}

/// `gate` directive on a callable. Two closed forms in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum GateDirectiveAst {
    Behind { feature: String, span: Span },
    Quota { limit: String, span: Span },
}

impl GateDirectiveAst {
    pub fn span(&self) -> Span {
        match self {
            GateDirectiveAst::Behind { span, .. } => *span,
            GateDirectiveAst::Quota { span, .. } => *span,
        }
    }
}

/// PG.A — out-of-band map keyed by callable name, holding gate
/// directives lifted from each `command` / `job` / `webhook` / `api` /
/// `query.list` / `query.lookup` / `query.sql` block. Returned by
/// `parse_feature_gates(source)` so analyzers and codegen can read
/// gates without churning the existing surface AST struct literals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureGatesAst {
    /// Per-feature, per-callable gates. Outer key is the feature name;
    /// inner key is `command:<name>` / `job:<name>` / `webhook:<name>` /
    /// `api:<name>` / `query.list:<name>` / `query.lookup:<name>` /
    /// `query.sql:<name>`. The qualified-callable key is what doctor
    /// and codegen consume.
    pub callables: std::collections::BTreeMap<String, Vec<GateDirectiveAst>>,
}

// =============================================================================
// Report vocab — `report <name>` kind AST.
//
// Tabular export contract (CSV / XLSX) declared at compile time. Replaces the
// `api + opaque handler` pattern for static-column exports. See
// `docs/proposals/report-vocab.md` v0.2.
//
// Surface (Surface B only — v0.2 ships this one):
//   report <name>
//     source <qualified_query_ref>
//     columns
//       <col> from row.<field> | @fn.<name>(args) [label "..."] [format "..."]
//     formats csv, xlsx
//     storage <ref>          (optional)
//     visibility signed|public|private
//     signed_ttl <duration>
//     filename "..."
//     policy @policy.<name>
//     rate_limit "..."
//     audit ...
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDecl {
    pub name: String,
    /// `source <qualified_query_ref>` — required. Captured verbatim
    /// (`customer.query.list`, `query.list`). Analyzer resolves.
    pub source: String,
    /// `columns` block. Must contain at least one entry; doctor enforces
    /// via `REPORT-COLUMNS-EMPTY-001`.
    pub columns: Vec<ReportColumnAst>,
    /// `formats csv, xlsx` — closed catalog enforced at lowering /
    /// doctor via `REPORT-FORMAT-UNKNOWN-001`.
    pub formats: Vec<String>,
    /// `storage <capability_ref>` — optional. When omitted and the
    /// package declares exactly one `object_storage` capability, the
    /// analyzer binds implicitly. Otherwise `REPORT-STORAGE-AMBIGUOUS-001`.
    pub storage: Option<String>,
    /// `visibility signed|public|private` — defaults to `signed` at
    /// lowering. Closed catalog.
    pub visibility: Option<String>,
    /// `signed_ttl 1h` — duration literal preserved as text. Required
    /// when `visibility=signed`; rejected otherwise.
    pub signed_ttl: Option<String>,
    /// `filename "..."` — template string. Tokens validated by the
    /// analyzer via `REPORT-FILENAME-TOKEN-UNKNOWN-001`.
    pub filename: Option<String>,
    /// `policy @policy.<name>` — required.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `rate_limit "..."` — required when policy includes `@scope.public`.
    /// Env-aware per `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    /// `audit <subjects>` canonical block (see `CommandAudit`).
    pub audit: Option<CommandAudit>,
    pub span: Span,
}

/// One column in a `report.columns` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportColumnAst {
    pub name: String,
    /// `from row.<field>` or `from @fn.<name>(args)`.
    pub source: ReportColumnSourceAst,
    /// `label "..."` — optional human label.
    pub label: Option<String>,
    /// `format "..."` — optional value format hint (`yyyy-mm-dd`,
    /// `currency:BRL`, etc.). v0 catalog is documented; the parser
    /// captures verbatim.
    pub format: Option<String>,
    pub span: Span,
}

/// Column-source grammar — proposal v0.2 closes this to two variants.
/// `Constant(String)` was rejected (no pilot evidence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ReportColumnSourceAst {
    /// `row.<field>` — project a field from the source query record.
    RowField(String),
    /// `@fn.<name>(arg, arg, ...)` — call a user-defined or capability
    /// function. Args are captured verbatim (comma-split, trimmed).
    FnCall { name: String, args: Vec<String> },
}
