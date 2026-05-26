//! `feature <name>` skeleton AST + the feature-scoped cross-cutting
//! blocks (policy, errors, defaults, translations, enums, RBAC catalog,
//! context vocab, cross-feature contracts).
//!
//! `FeatureSkeleton` is the **container struct** for a single
//! `feature <name>` block: it owns vectors of agents, commands, queries,
//! resources, jobs, webhooks, notifications, channels, pollers, mcp
//! servers, etc. Per-construct surface ASTs live in their own
//! sub-files; this module holds the container plus the feature-scoped
//! cross-cutting blocks split across siblings:
//!
//! - [`policy`] — `PolicyAtomAst`, `PolicyExprAst`, `PoliciesDecl`,
//!   `PolicyCategoryDecl`, `FieldPoliciesDecl`, `FieldPolicyDecl`,
//!   `WhenDeniedRouteAst`, `RoleMismatchArmAst`, `RouteRedirectTargetAst`,
//!   `TranslationKeyRefAst`.
//! - [`rbac`] — package-level `permission` / `role` catalog.
//! - [`errors`] — `errors` block (IR Error-Vocab Cell PARSE-1).
//! - [`enums`] — `enum <Name>` declarations.
//! - [`translation`] — i18n bucket cycle (`translation` block +
//!   `locale_negotiate`).
//! - [`defaults`] — feature-level `defaults` block.
//! - [`contracts`] — cross-feature contracts + `uses` clauses.
//! - [`context`] — iron-hand `purpose` / `non_goals` / `attach_ctx`.

use serde::{Deserialize, Serialize};

use super::{
    Agent, AggregateDecl, ApiDecl, Auth, CacheProfileDecl, Channel, CommandDecl, EventGroup, Job,
    McpServer, Notification, QueryDecl, RecordDecl, ReportDecl, ResourceDecl, Span,
    TenantMigration, Webhook,
};

pub mod context;
pub mod contracts;
pub mod defaults;
pub mod enums;
pub mod errors;
pub mod policy;
pub mod rbac;
pub mod translation;

pub use context::{LziFeatureAttachCtx, LziFeatureNonGoals, LziFeaturePurpose};
pub use contracts::{PublicContractDeclAst, UsesClauseAst};
pub use defaults::{DefaultsPolicyFor, DefaultsTenancy, FeatureDefaults};
pub use enums::{EnumDeclAst, EnumStorageValueDecl, EnumVariantDecl};
pub use errors::{
    ErrorExposureDefaultAst, FeatureErrorExposeRuleDecl, FeatureErrorMessageDecl,
    FeatureErrorsDecl,
};
pub use policy::{
    FieldPoliciesDecl, FieldPolicyDecl, PoliciesDecl, PolicyAtomAst, PolicyCategoryDecl,
    PolicyExprAst, RoleMismatchArmAst, RouteRedirectTargetAst, TranslationKeyRefAst,
    WhenDeniedRouteAst,
};
pub use rbac::{PermissionDeclAst, RoleDeclAst, RoleGrantsAst};
pub use translation::{
    LocaleNegotiateDecl, TranslationDecl, TranslationKeyDecl, TranslationPluralArmDecl,
    TranslationVariantDecl,
};

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
