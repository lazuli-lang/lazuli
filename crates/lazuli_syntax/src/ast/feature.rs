//! `feature <name>` skeleton AST + everything that lives at feature
//! scope but isn't a domain construct of its own (policy block, errors
//! block, defaults, translations, enums, RBAC catalog, context vocab).
//!
//! `FeatureSkeleton` is the **container struct** for a single
//! `feature <name>` block: it owns vectors of agents, commands, queries,
//! resources, jobs, webhooks, notifications, channels, pollers, mcp
//! servers, etc. Per-construct surface ASTs live in their own
//! sub-files; this file holds the container plus the feature-scoped
//! cross-cutting blocks (policies / errors / defaults / translations /
//! enums / RBAC catalog / iron-hand context vocab / cross-feature
//! contracts).
//!
//! `PolicyAtomAst` and `PolicyExprAst` (RB.S6 structured policy form)
//! also live here because they're consumed by *every* callable surface
//! and authoring them in `feature.rs` keeps the policy vocabulary co-
//! located with its consumers (`PoliciesDecl` / `PolicyCategoryDecl` /
//! `FieldPolicyDecl`).
//!
//! `PackageSkeleton` aggregates feature skeletons plus the package-
//! level RBAC catalog (`permission` + `role` decls). It's the top-level
//! output of `parse_package_skeleton`.

use serde::{Deserialize, Serialize};

use super::{
    Agent, AggregateDecl, ApiDecl, Auth, CacheProfileDecl, Channel, CommandDecl, EventGroup, Job,
    McpServer, Notification, QueryDecl, RecordDecl, ReportDecl, ResourceDecl, Span, TenantMigration,
    Webhook,
};

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
// Cross-feature contract annotation per
// `docs/proposals/cross-feature-contracts.md` §5.1.
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

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `enum <Name>` declaration surface AST.
//
// Authored inside `domain` at indent 4. Variants at indent 6 are either
// bare identifiers (`free`) or `<name> = <value>` (storage value is `i64`
// or quoted string).
// -----------------------------------------------------------------------------

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
