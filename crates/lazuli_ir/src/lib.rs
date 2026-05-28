//! Lazuli intermediate representation.
//!
//! Shape governance lives in `docs/ir-abi.md`. This crate exposes types only;
//! it has no public mutator. Producers live in `lazuli_analyzer` lowering
//! parsed `.lzi` and `.lzx` source.
//! All consumers (codegens, planner, LSP, MCP, CLI) read this data and never
//! write back. Re-authoring means rewriting source.
//!
//! Phase 1a foundation: `Module` / `Feature` / `Resource` / `Field` (with
//! `TypeRef` enum), `EnumDecl`, `Command` (with `Effect`), `Query` (List /
//! Lookup / Sql), and a minimal `Predicate` AST. Workflows, rules, events,
//! surfaces, jobs, webhooks, auth, escape routes, and extension contracts are
//! reserved for later phases.

use serde::{Deserialize, Serialize};

pub mod encryption;
pub mod nodes;
pub mod security_duration;
pub use encryption::{
    E2eeCapability, EncryptionAlgorithm, EncryptionBinding, EncryptionKeyScope, EncryptionRotation,
    EncryptionSource, EncryptionTemplate, EncryptionTemplateAxis,
};
pub use nodes::aggregate::{Aggregate, Invariant};
pub use nodes::ai_primitives::{
    Agent, AgentOutputKind, Api, DiscriminatorRef, EvalAssertion, EvalAssertionKind, EvalCase,
    EvalContainsRhs, EvalPredicate, GoldenSpec, HttpExposure, HttpMethod, QualifiedToolRef,
    ToolBinding, ToolEffect, ToolKind, ToolsCallsOp,
};
pub use nodes::app_contract::{
    AppContract, ContractEvent, ContractField, ContractImport, ContractOperation,
    ContractOperationError, ContractRecord,
};
pub use nodes::app_manifest::{
    AppArchitecture, AppBinding, AppCapability, AppCommunication, AppCookie, AppCors,
    AppCorsOriginRule, AppDeploy, AppEnvVar, AppHeaders, AppHsts, AppIntegration,
    AppIntegrationCredentialBinding, AppIntegrationCredentials, AppLimits, AppLocale, AppLogging,
    AppManifest, AppObservability, AppPack, AppPackProvide, AppPackUse, AppProfile,
    AppProfileDeploy, AppProfileIntegration, AppProfileUrl, AppProxy, AppRegistry, AppRuntimeUnit,
    AppService, AppServiceExposure, AppTracing, AppUrl, CookieProfile, DeployCheckpoint,
    ERROR_PAGE_STATUS_CATALOG, ErrorPage, LocaleFallback, LocaleNegotiate, RegistryToolEntry,
    SecretRotation, Translation, TranslationKey, TranslationPluralArm, TranslationVariant,
    WebhookEvent, WebhookEventField, WebhookEventRegistry,
};
pub use nodes::async_work::{
    BackoffStrategy, DigestStrategy, DlqSpec, ExternalCallRef, FanoutScope, FanoutSpec,
    IdempotencyKey, Job, JobBody, JobDeclarative, JobHandler, JobOperationalKind, JobTrigger,
    Notification, NotificationDigest, NotificationThrottle, ReplayMode, ReplaySpec, RetryPolicy,
    TenantFromSpec, VerifyScheme, VerifySpec, Webhook, WebhookEventRef, WebhookScopeGlobalSpec,
};
pub use nodes::auth::{
    Auth, AuthIdentity, AuthMfa, AuthOAuthProvider, AuthPassword, AuthSessions, RotationConfig,
    SessionExtraColumn, TheftAction,
};
pub use nodes::capability::{
    CapabilityRef, EncryptedCapability, FileCapability, FileSize, FileSizeLiteral, FileVisibility,
    HashAlgorithm, HashedCapability, MimeType, PiiCapability, TokenCapability, TokenStore,
};
pub use nodes::command::{
    ApprovalSpec, ApprovalThen, Assignment, AuditMaterialize, AuditSpec, Command, CommandEffect,
    CommandInput, CommandKind, CommandWriteWindow, CreateEffect, DeleteEffect, Deprecation,
    DeprecationReplacement, DerivedFrom, InvalidatesSpec, LetBinding, NamedArg, PolicyRef,
    ReorderEffect, ReturnsEffect, RouteSlot, RouteSlotKind, TargetExpr, TypedSlot, UpdateEffect,
};
pub use nodes::design::{
    ColorState, ColorStateKind, ColorToken, CustomToken, Design, EasingToken, FamilyToken, Motion,
    ScaleToken, ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};
pub use nodes::error_vocab::{
    ErrorExposeRule, ErrorExposureDefault, FeatureErrorMessage, FeatureErrors, FeatureFieldError,
    TranslationKeyRef,
};
pub use nodes::event::{
    BuiltInTraceEvent, BuiltInTraceRecord, EmitPredicate, EmitPredicateKind, Event, EventField,
    EventGroup, EventKind, EventVariant, EventVariantKind, OperationKind, OperationRef, OutboxMode,
    Rule, TraceFiresPer, built_in_trace_event, built_in_trace_event_records, built_in_trace_events,
    is_reserved_trace_event_name,
};
pub use nodes::experience::{
    AppRoute, AudienceSurface, Experience, ExperienceAction, ExperienceModule, ExperienceView,
    ForbidWhen, Platform, PlatformSurface, PlatformView, RequiresField, RequiresLifecycle,
    RequiresLifecycleIn, ResolvedLifecycleGate, ResumeArm, ResumeArmKind, ResumeRouter,
    RouteGuardDefaults, RouteLoader, ViewExtension, ViewExtensionOrder, ViewExtensionSlot,
    ViewGuard, ViewTestAssertion,
};
pub use nodes::feature::{Feature, FeatureRequirement};
pub use nodes::feature_defaults::{
    Constraint, Defaults, EscapeRoute, Extension, ExtensionContract, FieldValidation,
    IndexConstraint, IndexMethod, NonGoal, PathRef, PathSource, Tenancy, UniqueConstraint,
};
pub use nodes::lifecycle::{
    FieldRef, HandlerRef, Lifecycle, LifecycleInvariant, LifecycleState, LifecycleStateKind,
    LifecycleTransition, Transition, Workflow,
};
pub use nodes::mcp::{
    MCPAuth, MCPParam, MCPPrompt, MCPResource, MCPServerMetadata, MCPServerSpec, MCPTool,
    MCPTransport,
};
pub use nodes::migrations::{
    TenantMigration, TenantMigrationTarget, TenantMigrationTargetOperation,
};
pub use nodes::plan_and_gate::{
    AutoPhotoCommandRole, Gate, Plan, PlanCatalog, PlanLimit, PlanLimitValue, SubscriptionAnchor,
    SynthesizedFromCapFile, TrialPolicy,
};
pub use nodes::policy_expr::{PolicyAtom, PolicyExpr};
pub use nodes::poller::{
    Poller, PollerBackoff, PollerCursor, PollerRetry, PollerRetryQuirk, PollerState,
    PollerStateKind, PollerTick,
};
pub use nodes::query::{
    CacheProfile, CacheTtl, CacheTtlLiteral, CompareOp, Expr, Filter, FnCallExpr, KeyClause,
    ListQuery, LookupQuery, OrderBy, OrderDir, Path, Predicate, Query, QueryCache, SqlQuery,
    SqlQueryKind,
};
pub use nodes::rate_limit::{EnvName, RateLimitByEnv, RateLimitSpec};
pub use nodes::rbac::{PermissionEntry, RbacCatalog, RoleEntry, RoleGrants};
pub use nodes::realtime::Channel;
pub use nodes::report::{
    FilenameToken, FnInvocation, Report, ReportColumn, ReportColumnSource, ReportFilenamePattern,
    ReportFormat, ReportSource,
};
pub use nodes::resource::{
    BuiltinType, CompositeKey, ComputedDate, ComputedDateBase, ComputedDateOffset,
    ConventionOrigin, ConventionRef, CrossFeatureTarget, CurrencyCode, EnumDecl, EnumVariant,
    Field, FieldConstraints, LifecycleRouteArm, LifecycleRoutes, LockSpec, OwnerAxis,
    OwnerScopeSql, PolymorphicRef, Record, Resource, RetentionAction, RetentionSpec,
    SanitizeHtmlProfile, StorageValue, TypeRef,
};
pub use nodes::source_map::{
    ImportEdge, PublicContract, SourceFile, SourceLocation, SourceMap, SymbolKind, SymbolOrigin,
    SymbolOriginIndex,
};
pub use nodes::surface::{
    Audience, AudienceUx, BindingRef, CellBinding, CommandRef, DrawerBindingSource,
    DrawerRouteBinding, DrawerSubView, DrawerTrigger, FilterCardinality, FilterDecl, FlashSpec,
    InlineTable, ListRender, OnSuccessSpec, QueryKind, QueryRef, RenderMode, RouteParam,
    SearchDecl, SearchField, SearchMode, SelectionDecl, SelectionMode, SettingDecl,
    SettingPersistence, SettingValueSpace, SortDecl, SortDir, Surface, SurfaceTarget, TabEntry,
    TabGroup, TabGroupCase, Tabs, View, ViewCreate, ViewDetail, ViewList, ViewUx, Wizard,
    WizardStep, WizardSteps,
};
pub use nodes::test_and_policy::{
    FieldPolicies, FieldPolicy, Policies, PolicyCategory, RoleMismatchArm, RouteRedirectTarget,
    TestAssertion, TestBlock, WhenDeniedRoute,
};
pub use nodes::workspace::{
    AppWorkspace, WorkspaceApp, WorkspaceBoundary, WorkspaceCommunication, WorkspaceGateway,
    WorkspaceGatewayRoute,
};

/// LZIR_SCHEMA — version of the IR JSON ABI. Bumped to 0.16.0 by
/// `ir-rate-limit-env-aware` cell 1 (proposal §4.1 + §8): the
/// `Command.rate_limit` / `Api.rate_limit` / `Agent.rate_limit` /
/// `Report.rate_limit` / `AuthPassword.rate_limit` fields move from
/// `Option<String>` to `Option<RateLimitSpec>`, gaining a structured
/// `default` + `by_env` shape with a closed `EnvName` catalog. The
/// single-line `rate_limit "X"` source shape lowers to
/// `RateLimitSpec { default: "X", by_env: vec![] }`, preserving 100%
/// of source-level back-compat. The JSON ABI changes shape on these
/// slots — consumers reading IR JSON must read the object form (string
/// fixtures no longer deserialize without lifting via
/// `RateLimitSpec::from_default`).
///
/// Previously bumped to 0.15.0 by Phase L Tier 4b — additive
/// `Command.rate_limit`, `Command.audit`, `Command.approval`,
/// `Command.invalidates`, `Command.external_calls`, plus the new
/// `Api`, `AuditSpec`, `ApprovalSpec`, `ApprovalThen`,
/// `InvalidatesSpec`, and `Feature.apis` types. The `JobDeclarative`
/// spine moves from `raw_target`/`raw_lets`/`raw_effect` strings to
/// typed `target: Option<TargetExpr>`, `lets: Vec<LetBinding>`,
/// `effect: CommandEffect` (the canonical Phase 1a shape). All field
/// additions carry `#[serde(default, skip_serializing_if = "…")]` so
/// 0.14.0 fixtures deserialize unchanged.
pub const LZIR_SCHEMA: &str = "0.16.0";

pub type FileId = u16;

/// Span back-reference into the source AST. Debug-only; not part of the
/// published JSON ABI. Consumers must opt in via `--with-spans`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanRef {
    pub start: usize,
    pub end: usize,
}

/// Alias used by newer IR cells that name source anchors as `span`.
pub type Span = SpanRef;

// =============================================================================
// Rate limit family (RateLimitSpec, RateLimitByEnv, EnvName) lives in
// `nodes::rate_limit` after the W4.1 rails-style split. Re-exported at the
// crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Source-map + symbol-origin sidecars (SourceMap, SourceFile,
// SymbolOriginIndex, SymbolOrigin, ImportEdge, SymbolKind, SourceLocation,
// PublicContract) live in `nodes::source_map` after the W4.1 rails-style
// split. Re-exported at the crate root above to preserve the ABI surface.
// =============================================================================

/// A module is the IR root. It groups the optional app operational manifest
/// and one or more features that flowed through the same compilation.
///
/// `Eq` is omitted because `Feature` no longer implements it (see the note
/// on `Feature`'s derive).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<AppWorkspace>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts: Vec<AppContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<AppRegistry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<AppProfile>,
    /// L0 #2 — top-level `design.lzi` declaring the closed token catalog
    /// for the product (colors, typography, spacing, etc.). Optional;
    /// emitters skip the design pipeline when `None`. See `docs/proposals/
    /// design-tokens.md` and `Design` below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<Design>,
    /// RBAC catalog — package-scoped `permission` / `role` declarations.
    /// `None` when the package has no catalog (legacy `@role.*` text-walk
    /// stays in effect for back-compat per
    /// `docs/proposals/rbac-catalog-vocab.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rbac: Option<RbacCatalog>,
    pub features: Vec<Feature>,
}

// =============================================================================
// RBAC catalog IR.
// RBAC family (RbacCatalog, PermissionEntry, RoleEntry, RoleGrants) lives in
// nodes::rbac after the W4.1 rails-style split. Re-exported at the crate root
// above to preserve the ABI surface. Produced by
// `lazuli_analyzer::analyze_rbac_catalog`; closure is analyzer-derived and
// baked into the IR so downstream consumers (codegen, doctor, inspect) never
// recompute.
// =============================================================================

// =============================================================================
// Cross-feature contract IR (AppContract + ContractImport / ContractRecord /
// ContractField / ContractOperation / ContractOperationError / ContractEvent)
// lives in `nodes::app_contract` after the W4.1 rails-style split. Re-exported
// at the crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Workspace IR (AppWorkspace + WorkspaceApp / WorkspaceBoundary /
// WorkspaceCommunication / WorkspaceGateway / WorkspaceGatewayRoute) lives in
// `nodes::workspace` after the W4.1 rails-style split. Re-exported at the
// crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Feature + FeatureRequirement live in `nodes::feature` after the W4.1
// rails-style split. Re-exported at the crate root above to preserve the
// ABI surface.
// =============================================================================

// =============================================================================
// Resource family — `resource <Name>`, `record <Name>`, `enum <Name>`, type refs.
// EnumDecl, EnumVariant, StorageValue, Resource, LifecycleRoutes, LifecycleRouteArm,
// ConventionRef, ConventionOrigin, LockSpec, CompositeKey, RetentionSpec, Record,
// RetentionAction, Field, OwnerAxis, OwnerScopeSql, FieldConstraints,
// SanitizeHtmlProfile, TypeRef, BuiltinType, CurrencyCode now live in
// `nodes::resource` after the Wave R3-F Stage 4 rails-style split. Re-exported
// at the crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Phase L Tier 2 — typed `@cap.*` capabilities.
// Family (CapabilityRef + File/PII/Hashed/Encrypted/Token sub-shapes,
// FileSize, FileSizeLiteral, MimeType, FileVisibility, HashAlgorithm,
// TokenStore) lives in `nodes::capability` after the W4.1 rails-style
// split. Re-exported at the crate root above to preserve the ABI surface.
// =============================================================================

/// Qualified name for a feature-scoped or local symbol. `feature` is `None`
/// for local references; cross-feature references carry the feature id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QualifiedName {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    pub name: String,
}

/// Literal default value attached to a field declaration. Covers the four
/// primitive shapes the lowering accepts (`String`, `Integer`, `Boolean`,
/// `EnumLiteral`) plus `Nil` for explicit `nil` defaults. Serialized as a
/// tagged union (`kind` / `value`) so consumers can match without losing the
/// type discriminator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultValue {
    /// String literal default (`"hello"`).
    String(String),
    /// Integer literal default (`42`).
    Integer(i64),
    /// Boolean literal default (`true` / `false`).
    Boolean(bool),
    /// Enum literal default (`Color.Red`).
    EnumLiteral(EnumLiteral),
    /// Explicit `nil` — distinct from a missing default.
    Nil,
}

/// One named enum variant reference used as a default value. The
/// `type_name` is only populated when the literal is fully qualified
/// (`Color.Red`); unqualified literals (`Red`) leave it `None` and rely on
/// surrounding context to resolve the type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumLiteral {
    /// `None` when the literal is unqualified and the type comes from context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<QualifiedName>,
    /// The variant identifier (e.g. `"Red"`).
    pub variant: String,
}

// =============================================================================
// Command + Query families — `command <name>` and `query.list/lookup/sql <name>`.
// Command/CommandKind/CommandInput/CommandEffect/Create-/Update-/Delete-/Returns-
// Effect/Assignment/RouteSlot/RouteSlotKind/TypedSlot/TargetExpr/NamedArg/LetBinding/
// PolicyRef/Deprecation/DeprecationReplacement/AuditSpec/ApprovalSpec/ApprovalThen/
// InvalidatesSpec/CommandWriteWindow now live in `nodes::command`.
// Query/ListQuery/LookupQuery/SqlQuery/SqlQueryKind/QueryCache/CacheProfile/
// CacheTtl/CacheTtlLiteral/Filter/KeyClause/OrderBy/OrderDir/Predicate/CompareOp/
// Expr/FnCallExpr/Path now live in `nodes::query`. Re-exported at the crate root
// above to preserve the ABI surface. Wave R3-F Stage 5 rails-style split.
// =============================================================================

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

// =============================================================================
// Phase 1b — events, rules, workflows, surfaces.
// Event family (Event, EventKind, EventField, OutboxMode, BuiltInTraceEvent,
// BuiltInTraceRecord, TraceFiresPer, Rule, OperationRef, OperationKind,
// EventGroup, EventVariant, EventVariantKind, EmitPredicate, EmitPredicateKind)
// plus built_in_trace_events / built_in_trace_event_records /
// is_reserved_trace_event_name / built_in_trace_event helpers live in
// `nodes::event` after the W4.1 rails-style split. Re-exported at the
// crate root above to preserve the ABI surface. The Workflow + Lifecycle
// sub-family lives in `nodes::lifecycle`; the surface sub-family lives
// in `nodes::surface`.
// =============================================================================

// =============================================================================
// CL.C.4 — `aggregate <Name>` + standalone `invariant <name>` vocabulary.
// Aggregate family (Aggregate, Invariant) lives in `nodes::aggregate` after
// the W4.1 rails-style split. Re-exported at the crate root above to
// preserve the ABI surface.
// =============================================================================

// =============================================================================
// L0 #8 — `poller` vocabulary (docs/proposals/poller-vocab.md §4).
// Poller family (Poller, PollerCursor, PollerRetry, PollerBackoff, PollerState,
// PollerStateKind, PollerTick, PollerRetryQuirk) lives in `nodes::poller`
// after the W4.1 rails-style split. Re-exported at the crate root above to
// preserve the ABI surface.
// =============================================================================

// =============================================================================
// Lzx ViewModel surface IR — L0 #3 (lzx-integration-codegen).
// Surface family (Surface, SurfaceTarget, Audience, View, ViewList, ViewDetail,
// ViewCreate, OnSuccessSpec, FlashSpec, QueryRef, QueryKind, CommandRef,
// CellBinding, RouteParam, ListRender, DrawerSubView, DrawerTrigger,
// DrawerRouteBinding, DrawerBindingSource, FilterDecl, FilterCardinality,
// SearchDecl, SearchMode, SearchField, BindingRef, SortDecl, SortDir,
// SelectionDecl, SelectionMode, SettingDecl, SettingValueSpace,
// SettingPersistence) lives in `nodes::surface` after the W4.1 rails-style
// split. Re-exported at the crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// PolicyAtom + PolicyExpr live in `nodes::policy_expr` after the W4.1
// rails-style split. Re-exported at the crate root above to preserve the
// ABI surface.
// =============================================================================

// =============================================================================
// Experience IR — `.lzx`
// AppManifest family lives in `nodes::app_manifest` after the Wave R3-F
// rails-style split (security, registry, runtime, observability, locale).
// ExperienceModule + Experience + AppRoute + ViewGuard families live in
// `nodes::experience` after the same wave. Re-exported at the crate root
// above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Phase 1c — feature defaults, resource enrichment, extensions, escape routes.
// Family (NonGoal, Defaults, Tenancy, Constraint, UniqueConstraint,
// IndexConstraint, IndexMethod, FieldValidation, Extension, ExtensionContract,
// PathRef, PathSource, EscapeRoute) lives in nodes::feature_defaults after the
// W4.1 rails-style split. Re-exported at the crate root above to preserve the
// ABI surface.
// =============================================================================

// =============================================================================
// Phase 1d — async work: jobs and webhooks.
// Job + Webhook + Notification families live in nodes::async_work after the
// W4.1 rails-style split. Re-exported at the crate root above to preserve
// the ABI surface. See nodes/async_work.rs for shape + design notes.
// =============================================================================

// =============================================================================
// Realtime bucket cycle MVP — `channel <name>` kind.
// Channel family (Channel) lives in `nodes::realtime` after the W4.1
// rails-style split. Re-exported at the crate root above to preserve the
// ABI surface. See `docs/proposals/bucket-realtime-cycle.md` and
// `docs/proposals/bucket-realtime-scope.md`.
// =============================================================================

// =============================================================================
// Migrations bucket cycle (Route C) — tenant_migration kind.
// TenantMigration family (TenantMigration, TenantMigrationTarget,
// TenantMigrationTargetOperation) lives in `nodes::migrations` after the
// W4.1 rails-style split. Re-exported at the crate root above to preserve
// the ABI surface.
// =============================================================================

// =============================================================================
// Phase 1e — auth block
// Auth family (Auth, AuthIdentity, AuthPassword, AuthSessions, AuthMfa,
// AuthOAuthProvider, RotationConfig, SessionExtraColumn, TheftAction) lives
// in `nodes::auth` after the W4.1 rails-style split. Re-exported at the
// crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Cut A — AI primitives: agent + tools + evals + discriminated output.
// AI primitives family (Agent, HttpExposure, HttpMethod, Api, AgentOutputKind,
// DiscriminatorRef, ToolBinding, QualifiedToolRef, ToolKind, ToolEffect,
// EvalCase, GoldenSpec, EvalAssertion, EvalAssertionKind, EvalPredicate,
// EvalContainsRhs, ToolsCallsOp) lives in `nodes::ai_primitives` after the
// W4.1 rails-style split. Re-exported at the crate root above to preserve
// the ABI surface. See `docs/proposals/ai-primitives-v0.md` for design.
// =============================================================================

// =============================================================================
// Report vocab — `report <name>` IR.
// Report family (Report, ReportSource, ReportColumn, ReportColumnSource,
// FnInvocation, ReportFormat, ReportFilenamePattern, FilenameToken) lives in
// `nodes::report` after the W4.1 rails-style split. Re-exported at the crate
// root above to preserve the ABI surface. See `docs/proposals/report-vocab.md`
// v0.2 for the design and `runtime/go/lazuli/report` for the byte-level
// adapter the IR routes into.
// =============================================================================

// =============================================================================
// Phase 1f — inline tests + policy registry with field-level policies.
// Family (TestBlock, TestAssertion, Policies, PolicyCategory, WhenDeniedRoute,
// RoleMismatchArm, RouteRedirectTarget, FieldPolicies, FieldPolicy) lives in
// nodes::test_and_policy after the W4.1 rails-style split. Re-exported at the
// crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// L0 #2 — Design Tokens.
// Design family (Design, CustomToken, ColorToken, ColorState, ColorStateKind,
// Typography, FamilyToken, TextScaleToken, WeightToken, TrackingToken,
// ScaleToken, ShadowToken, Motion, EasingToken, ZToken) lives in
// `nodes::design` after the W4.1 rails-style split. Re-exported at the
// crate root above to preserve the ABI surface. See
// `docs/proposals/design-tokens.md` §3 and `docs/proposals/design-tokens-custom.md`.
// =============================================================================

// =============================================================================
// Plan & Gate vocabulary (PG.B — `docs/proposals/plan-and-gate-vocab.md`).
// Plan family (PlanCatalog, Plan, PlanLimit, PlanLimitValue, TrialPolicy,
// SubscriptionAnchor, Gate, SynthesizedFromCapFile, AutoPhotoCommandRole)
// lives in `nodes::plan_and_gate` after the W4.1 rails-style split.
// Re-exported at the crate root above to preserve the ABI surface.
// =============================================================================

// =============================================================================
// Wave R3-F Stage 3 — the three legacy `#[cfg(test)]` modules that used
// to live here (`lifecycle_tests`, `l0_6_ir_tests`, `owner_axis_tests`)
// have been promoted to integration tests under `crates/lazuli_ir/tests/`:
//
//   - tests/lifecycle_round_trip.rs
//   - tests/l0_6_round_trip.rs
//   - tests/owner_axis_round_trip.rs
//
// They only touched the public ABI surface, so moving them out of the lib
// root shrinks the crate root toward the gold-standard ≤ 1000 LOC target
// without changing what is tested.
// =============================================================================
