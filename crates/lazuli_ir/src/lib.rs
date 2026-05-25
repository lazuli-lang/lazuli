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
    ApprovalSpec, ApprovalThen, Assignment, AuditSpec, Command, CommandEffect, CommandInput,
    CommandKind, CommandWriteWindow, CreateEffect, DeleteEffect, Deprecation,
    DeprecationReplacement, InvalidatesSpec, LetBinding, NamedArg, PolicyRef, ReturnsEffect,
    RouteSlot, RouteSlotKind, TargetExpr, TypedSlot, UpdateEffect,
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
    ForbidWhen, Platform, PlatformSurface, PlatformView, RequiresLifecycle, ResolvedLifecycleGate,
    ResumeArm, ResumeArmKind, ResumeRouter, RouteGuardDefaults, RouteLoader, ViewExtension,
    ViewExtensionOrder, ViewExtensionSlot, ViewGuard, ViewTestAssertion,
};
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
pub use nodes::poller::{
    Poller, PollerBackoff, PollerCursor, PollerRetry, PollerRetryQuirk, PollerState,
    PollerStateKind, PollerTick,
};
pub use nodes::query::{
    CacheProfile, CacheTtl, CacheTtlLiteral, CompareOp, Expr, Filter, FnCallExpr, KeyClause,
    ListQuery, LookupQuery, OrderBy, OrderDir, Path, Predicate, Query, QueryCache, SqlQuery,
    SqlQueryKind,
};
pub use nodes::rbac::{PermissionEntry, RbacCatalog, RoleEntry, RoleGrants};
pub use nodes::realtime::Channel;
pub use nodes::report::{
    FilenameToken, FnInvocation, Report, ReportColumn, ReportColumnSource, ReportFilenamePattern,
    ReportFormat, ReportSource,
};
pub use nodes::resource::{
    BuiltinType, CompositeKey, ConventionOrigin, ConventionRef, CurrencyCode, EnumDecl,
    EnumVariant, Field, FieldConstraints, LifecycleRouteArm, LifecycleRoutes, LockSpec,
    OwnerAxis, OwnerScopeSql, Record, Resource, RetentionAction, RetentionSpec,
    SanitizeHtmlProfile, StorageValue, TypeRef,
};
pub use nodes::surface::{
    Audience, BindingRef, CellBinding, CommandRef, DrawerBindingSource, DrawerRouteBinding,
    DrawerSubView, DrawerTrigger, FilterCardinality, FilterDecl, FlashSpec, ListRender,
    OnSuccessSpec, QueryKind, QueryRef, RouteParam, SearchDecl, SearchField, SearchMode,
    SelectionDecl, SelectionMode, SettingDecl, SettingPersistence, SettingValueSpace, SortDecl,
    SortDir, Surface, SurfaceTarget, View, ViewCreate, ViewDetail, ViewList,
};
pub use nodes::test_and_policy::{
    FieldPolicies, FieldPolicy, Policies, PolicyCategory, RoleMismatchArm, RouteRedirectTarget,
    TestAssertion, TestBlock, WhenDeniedRoute,
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

/// ir-rate-limit-env-aware §4.1 — env-qualified rate limit container.
///
/// Backward-compat: the single-line `rate_limit "X"` source shape lowers
/// to `RateLimitSpec { default: "X", by_env: vec![] }`. The runtime helper
/// `ResolveLimit()` (cell 2) reads `LAZULI_ENV`, scans `by_env` in source
/// order, and returns the matching `limit` or falls through to `default`.
///
/// The `"unlimited"` keyword (proposal §4.4) lowers to an empty string —
/// either as the `default` (no throttle by default) or inside a
/// `RateLimitByEnv.limit` (no throttle for the listed envs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitSpec {
    /// Default rate limit applied when no env-qualified entry matches.
    /// Empty string == no rate limit (the `"unlimited"` sentinel lowers
    /// here). The single-line `rate_limit "X"` source shape populates
    /// only this field; `by_env` stays empty.
    pub default: String,
    /// Env-qualified overrides scanned linearly at request time. Each
    /// entry covers one-or-more `EnvName`s sharing a limit string.
    /// Source-order is preserved; the first matching entry wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_env: Vec<RateLimitByEnv>,
    /// Span of the FIRST `rate_limit` line for this spec — points at the
    /// default-declaring line. Per-`by_env` entries carry their own span.
    /// Optional so synth-emitted specs without a source location work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

impl RateLimitSpec {
    /// Backward-compat constructor — lifts the legacy single-string
    /// shape (or the lowered output of a one-line `rate_limit "X"`) into
    /// the new `RateLimitSpec` container. Call-sites that previously
    /// wrote `rate_limit: Some("X".to_owned())` swap to
    /// `rate_limit: Some(RateLimitSpec::from_default("X".to_owned()))`.
    pub fn from_default(s: String) -> Self {
        Self {
            default: s,
            by_env: Vec::new(),
            span_ref: None,
        }
    }
}

/// ir-rate-limit-env-aware §4.1 — single env-qualified override row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitByEnv {
    /// Env names this entry matches. Catalog is closed (`EnvName`);
    /// identifiers outside the catalog land in `unknown_envs` and trigger
    /// the doctor diagnostic `rate_limit_unknown_env` (Cell 3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub envs: Vec<EnvName>,
    /// Forward-compatible bucket for env identifiers outside the closed
    /// catalog. Parses OK at AST level; Cell 3 doctor emits
    /// `rate_limit_unknown_env`. Empty for well-formed source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_envs: Vec<String>,
    /// Limit string (e.g. `"100 per 10 minutes per ip"`) or the empty
    /// string when the source authored `"unlimited"` (proposal §4.4).
    pub limit: String,
    /// Span of this `rate_limit ... in <envs>` line in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// ir-rate-limit-env-aware §4.3 — closed catalog of recognized
/// `LAZULI_ENV` values. Adding a variant is an IR change requiring a
/// proposal. The JSON form is snake_case (matches the runtime
/// `LAZULI_ENV` strings the existing CORS/session code reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvName {
    Production,
    Staging,
    Test,
    Dev,
    Local,
}

impl EnvName {
    /// Parse a lowercase identifier into the closed catalog. Returns
    /// `None` for identifiers outside the catalog; callers (the parser
    /// today, Cell 3 doctor tomorrow) decide whether to surface a
    /// warning.
    pub fn from_ident(ident: &str) -> Option<Self> {
        match ident {
            "production" => Some(EnvName::Production),
            "staging" => Some(EnvName::Staging),
            "test" => Some(EnvName::Test),
            "dev" => Some(EnvName::Dev),
            "local" => Some(EnvName::Local),
            _ => None,
        }
    }

    /// Canonical lowercase identifier (matches `LAZULI_ENV` strings).
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvName::Production => "production",
            EnvName::Staging => "staging",
            EnvName::Test => "test",
            EnvName::Dev => "dev",
            EnvName::Local => "local",
        }
    }
}

/// SourceMap is the IR companion that resolves `SpanRef` byte
/// offsets to (file, line, column). Sidecar to `Module` — passed
/// alongside in codegen, serialized to `<module>.sourcemap.json`
/// when `--with-source` is requested. NOT embedded in `Module`
/// itself per ADR-3 (avoids cascading IR JSON size + snapshot
/// churn across 30+ SpanRef use sites).
///
/// EXPERIMENTAL: shape may grow additive fields before 1.0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub id: FileId,
    /// Canonical relative path, e.g. `features/customer.lzi`.
    pub path: String,
    /// Byte offset of each line start, plus one EOF sentinel.
    pub line_offsets: Vec<u32>,
}

/// Sidecar to `Module`. Resolves cross-feature symbol references.
/// Built by `lazuli_analyzer::build_symbol_origin_index`.
/// EXPERIMENTAL: shape may grow additive fields before 1.0.
///
/// See `docs/proposals/lsp-symbol-origin.md` §6.2.
///
/// `symbols` keys are formatted `<feature>.<name>` (e.g. `account.Gender`)
/// so the index serializes to JSON without custom key adapters. The Rust
/// caller can recover a `QualifiedName` via `QualifiedName::parse_dotted`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolOriginIndex {
    pub symbols: std::collections::BTreeMap<String, SymbolOrigin>,
    pub imports: std::collections::BTreeMap<String, Vec<ImportEdge>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolOrigin {
    pub feature: String,
    pub name: String,
    pub kind: SymbolKind,
    pub defined_at: SourceLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// Cross-feature contract version per
    /// `docs/proposals/cross-feature-contracts.md` §5.1, populated by
    /// `lazuli_analyzer::build_symbol_origin_index` when the origin's
    /// declaration carries `public contract <Symbol> as v<N>`.
    /// `None` when no contract is declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_version: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportEdge {
    pub importer: String,
    pub imported: String,
    pub uses_at: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Enum,
    Resource,
    Record,
    Scalar,   // reserved; populated post-L0 #4 scalar aliases
    Semantic, // closed catalog at docs/canonical-semantics.md (Email/Phone/Url/Uuid/Currency/GeoPoint/Money + plugin BrazilianCPF/CNPJ/CEP)
    Command,
    Query,
    Event,
    Aggregate,
}

/// Where a symbol is defined. Discriminated by `source`:
/// - `{ "source": "file", "file": "...", "line": N, "column": N }` for user-authored symbols
/// - `{ "source": "builtin" }` for compiler-provided types (Money, Email, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SourceLocation {
    File {
        file: String,
        line: u32,
        column: u32,
    },
    Builtin,
}

/// Cross-feature contract version annotation per
/// `docs/proposals/cross-feature-contracts.md` §5.1.
///
/// When a symbol is referenced from another feature under
/// `architecture mode microservices`, the origin feature MUST declare
/// this contract via `public contract <Symbol> as v<N>` adjacent to the
/// symbol's site. Doctor enforces.
///
/// Authored via the parser at `crates/lazuli_syntax/src/parser.rs`;
/// lowered by `lazuli_analyzer` from `PublicContractDeclAst`.
///
/// `version` is monotonic per symbol. `span_ref` anchors the
/// `public contract` source line for diagnostic origin reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicContract {
    pub version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppContract {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<ContractImport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<ContractRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<ContractOperation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ContractEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractImport {
    pub format: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ContractField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractField {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requiredness: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ContractOperationError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperationError {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<ContractField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppWorkspace {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<WorkspaceApp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_registry: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<WorkspaceBoundary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<WorkspaceCommunication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gateways: Vec<WorkspaceGateway>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApp {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBoundary {
    pub app: String,
    pub direction: String,
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceCommunication {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub propagate: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceGateway {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<WorkspaceGatewayRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceGatewayRoute {
    pub path: String,
    pub target_kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
}

/// A feature is the unit of product capability authored in one `.lzi` file.
///
/// `Eq` is intentionally omitted: Cut A's `Agent.temperature` / `top_p`
/// fields are `Option<f64>`, and `f64` has no `Eq` impl. Consumers that
/// need equality use `PartialEq` (`assert_eq!`-style comparisons still
/// work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_goals: Vec<NonGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<String>,
    pub defaults: Defaults,
    pub uses: Vec<String>,
    /// Span anchors for each entry in `uses`. Same length as `uses`. Populated
    /// by the analyzer when lowering from `.lzi` source; empty when `Feature`
    /// is constructed programmatically (tests, manual IR fixtures).
    ///
    /// Used by `lazuli_analyzer::build_symbol_origin_index` to anchor each
    /// `ImportEdge.uses_at`. See `docs/proposals/lsp-symbol-origin.md` §6.5.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses_spans: Vec<SpanRef>,
    /// Optional consumer-side version pin per `uses` entry. Same length as
    /// `uses` when populated; entries are `Some(N)` for `uses <feature> version v<N>`
    /// and `None` for unpinned entries. Empty when the feature has no `uses`
    /// lines or when constructed programmatically.
    ///
    /// Drives `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` per
    /// `docs/proposals/cross-feature-contracts.md` §5.4.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses_versions: Vec<Option<u16>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<FeatureRequirement>,
    pub enums: Vec<EnumDecl>,
    pub resources: Vec<Resource>,
    pub events: Vec<Event>,
    pub rules: Vec<Rule>,
    pub policies: Policies,
    /// IR Error-Vocab — `errors` block lifted into IR. Carries both
    /// exposure rules (legacy LSP surface, now lowered) and typed
    /// per-code message overrides. `None` when the feature uses the
    /// runtime defaults. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<FeatureErrors>,
    pub commands: Vec<Command>,
    /// Phase L Tier 4b — `api <name>` declarations lifted from the
    /// canonical-indent slice. Legacy lowering leaves this empty;
    /// `lower_feature_skeleton` populates it from
    /// `FeatureSkeleton.apis`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apis: Vec<Api>,
    /// Phase L Tier 4d — `record <Name>` declarations lifted from the
    /// canonical-indent slice. Legacy lowering leaves this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<Record>,
    pub queries: Vec<Query>,
    /// `resume <name>` blocks for lifecycle-aware route gates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_routers: Vec<ResumeRouter>,
    pub workflows: Vec<Workflow>,
    pub jobs: Vec<Job>,
    pub webhooks: Vec<Webhook>,
    /// Phase L Tier 3 — `notification <name>` declarations lifted from
    /// the canonical-indent slice. Legacy lowering leaves this empty;
    /// the inspect projection used to harvest notifications via
    /// text-pattern and now reads from IR when populated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<Notification>,
    /// Phase L Tier 3 — `event_group <pattern> on <Resource>`
    /// declarations lifted from the canonical-indent slice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_groups: Vec<EventGroup>,
    /// Migrations bucket cycle Route C — `tenant_migration <name>`
    /// declarations lifted from the canonical-indent slice. Mirrors the
    /// `jobs` slot exactly: one entry per declared tenant migration.
    /// Doctor `TM-*` diagnostics consume this slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tenant_migrations: Vec<TenantMigration>,
    /// i18n bucket cycle — `translation` block lifted from the
    /// canonical-indent slice. `None` when the feature does not author
    /// translation keys. Surfaces declared catalog path + typed keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<Translation>,
    /// L0 #8 — `poller <name>` declarations (docs/proposals/poller-vocab.md).
    /// Sibling slot of `jobs` / `webhooks` / `notifications`. Each
    /// entry models a persistent-cursor resolution loop over a same-
    /// feature resource. Additive: existing fixtures deserialize with
    /// an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pollers: Vec<Poller>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    pub surfaces: Vec<Surface>,
    pub extensions: Vec<Extension>,
    pub escape_routes: Vec<EscapeRoute>,
    /// Cut A: `agent <name>` declarations. The legacy lowering path
    /// produces an empty `Vec`; the canonical-indent slice in
    /// `lazuli_syntax::parse_feature_skeletons` is the producer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<Agent>,
    /// Report vocab — `report <name>` declarations. See
    /// `docs/proposals/report-vocab.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<Report>,
    /// Realtime bucket cycle MVP — `channel <name>` declarations
    /// (see `docs/proposals/bucket-realtime-cycle.md`). Sibling slot
    /// of `events` / `notifications` / `pollers`. Each entry models
    /// a typed, tenant-scoped, policy-gated push stream. Additive:
    /// pre-realtime fixtures deserialize with an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<Channel>,
    /// Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
    /// profiles. Sibling slot of `jobs`/`webhooks`/`notifications`.
    /// Queries reference profiles by name (`cache product_view`); the
    /// inline `cache { key, ttl }` form on a query stays for one-off
    /// ttl/key pairs. Additive: pre-CL.C.3 fixtures deserialize with
    /// an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caches: Vec<CacheProfile>,
    /// CL.C.4 — `aggregate <Name>` declarations (DDD consistency
    /// boundary). Each entry pins a root resource + a closed set of
    /// member resources + invariants spanning the cluster. Sibling
    /// slot of `resources`/`commands`/`policies`. Additive: features
    /// without `aggregate` blocks deserialize with an empty vec.
    /// See roadmap §1.7 + spec wave-c-cl4.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<Aggregate>,
    /// MCP bucket cycle — `mcp_server <name>` declarations. Sibling
    /// slot of `notifications` / `channels` / `pollers`. Each entry
    /// projects a feature's surface over MCP (tools / resources /
    /// prompts) per `docs/proposals/bucket-mcp-cycle.md`. Codegen
    /// emits `*.mcp.gen.go` and wires the Go runtime's
    /// `runtime/go/lazuli/mcp` package against the SDK. Additive:
    /// pre-MCP fixtures deserialize with an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<MCPServerSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// `conventions [crud]` synth origin map — keys are `Command.name` /
    /// `Query.name()` strings the synthesis pass either appended or
    /// would have appended (when the author wrote an override). Values
    /// describe whether the entry was synthesized or skipped because of
    /// an author-side override. Populated by Cell C3's synthesizer;
    /// consumed by Cell C4's `lazuli inspect features` annotation.
    /// Inlined by Cell C4 ahead of Cell C1's IR landing. Additive:
    /// pre-conventions fixtures deserialize empty.
    /// See `docs/proposals/ir-resource-conventions-crud.md` §11.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub synth_origins: std::collections::BTreeMap<String, ConventionOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRequirement {
    pub kind: String,
    pub name: String,
    pub contract: String,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultValue {
    String(String),
    Integer(i64),
    Boolean(bool),
    EnumLiteral(EnumLiteral),
    Nil,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumLiteral {
    /// `None` when the literal is unqualified and the type comes from context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<QualifiedName>,
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

/// Atomic policy reference like `@scope.workspace_admin`, `@role.editor`,
/// `@actor.workspace_owner`. Parser populates `namespace` ("scope" |
/// "role" | "actor") + `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyAtom {
    pub namespace: String,
    pub name: String,
    /// Optional argument literal. Currently only
    /// `@mfa.required(within:<dur>)` populates this —
    /// `args == Some("within:15m")`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

/// RB.S6 — structured `policy <expr>` form used by command / query /
/// job / webhook / api / notification declarations. Coexists with the
/// existing `PolicyRef` field for back-compat; populated only when the
/// authored policy text contained `has_role` / `has_permission` /
/// `authenticated` predicates or boolean combinators.
///
/// See `docs/proposals/rbac-catalog-vocab.md` §"Composition with the
/// existing `policy` block" for the dictionary-vs-predicate split.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyExpr {
    /// `authenticated` — true when the actor is a logged-in user.
    Authenticated,
    /// `has_role <name>` — true when actor's role matches `name` (the
    /// catalog closure subsumes inheritance at codegen time).
    HasRole(String),
    /// `has_permission <resource>:<action>[:...]` — true when actor's
    /// role grants the permission via the catalog closure.
    HasPermission(String),
    /// `@<ns>.<name>` atom embedded in an expression.
    Atom(PolicyAtom),
    /// `<a> and <b>` — boolean conjunction (n-ary; collected from
    /// left-associative parse).
    And(Vec<PolicyExpr>),
    /// `<a> or <b>` — boolean disjunction (n-ary).
    Or(Vec<PolicyExpr>),
    /// `not <a>` — boolean negation.
    Not(Box<PolicyExpr>),
}

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
