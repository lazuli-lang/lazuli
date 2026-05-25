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
pub use nodes::rbac::{PermissionEntry, RbacCatalog, RoleEntry, RoleGrants};
pub use nodes::realtime::Channel;
pub use nodes::report::{
    FilenameToken, FnInvocation, Report, ReportColumn, ReportColumnSource, ReportFilenamePattern,
    ReportFormat, ReportSource,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    pub variants: Vec<EnumVariant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    /// Authored storage value. `None` means the codegen picks per target;
    /// derived storage values do not enter the IR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_value: Option<StorageValue>,
    /// Optional UI-facing metadata. Keys are opaque app-catalog strings;
    /// IR and codegen do not validate catalog/icon existence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum StorageValue {
    Integer(i64),
    String(String),
}

/// CL.C.4 — `Resource` drops `Eq` because `Invariant` (transitively
/// carrying `EvalPredicate` ⇒ `Expr`) is not `Eq`. Mirrors the existing
/// `Feature` derive note. Consumers needing equality use `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    /// Tenancy axis: `tenancy org`, `tenancy team`, or `tenancy none` (opt-out).
    /// `None` means inherit from feature `defaults`. After lowering's derived
    /// pass this should be resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenancy: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft_delete: bool,
    /// `None` means inherit from feature `defaults`. `Some(true)` = explicit
    /// `timestamps`, `Some(false)` = explicit `no_timestamps` opt-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<bool>,
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    /// Resource-level inline validator: `validates resource "./domain/validate_row.go"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate: Option<PathRef>,
    /// Field-level inline validators: `validates field <field> "./hooks/validate_tier.go"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validates: Vec<FieldValidation>,
    /// Phase L Tier 4c — `retention <duration> then <action>` policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<RetentionSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// Lifecycle vocabulary (L0 #7 / lifecycle-vocab.md). When `Some`,
    /// the resource declares a state machine bound to a discriminator
    /// field; the lowering emits N commands + an auto-generated enum.
    /// `None` when the resource has no lifecycle (the vast majority).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    /// CL.C.4 — resource-level `invariant <name>` declarations. Each
    /// invariant carries a closed-catalog predicate (`when <pred>`)
    /// plus an authored `message`. Lowered from the canonical-indent
    /// slice. Shared shape with `Aggregate.invariants` (DDD aggregate
    /// boundary). Additive: pre-CL.C.4 fixtures deserialize empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<Invariant>,
    /// Roadmap §1.5 (CL.C.2) — `lock` decorator. Declares a
    /// concurrency-control strategy for writes to this resource.
    /// `None` when no lock is declared (the framework default
    /// `id BIGSERIAL` row identity is used as-is, races resolved by
    /// last-write-wins at the SQL layer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<LockSpec>,
    /// Roadmap §1.5 (CL.C.2) — `composite_key` block. When `Some` and
    /// `primary == true`, the migration emitter replaces the implicit
    /// `id BIGSERIAL PRIMARY KEY` with a `PRIMARY KEY (<fields>)`
    /// clause over the listed field columns. Doctor cross-checks each
    /// listed name against `Resource.fields`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_key: Option<CompositeKey>,
    /// Resource-level conventions opt-in: `conventions [crud, ...]`.
    /// Each entry references a closed-catalog convention bundle that
    /// auto-synthesizes commands/queries during lowering. Empty when
    /// the resource opts into no conventions (the default). See
    /// `docs/proposals/ir-resource-conventions-crud.md` §4.2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conventions: Vec<ConventionRef>,
    /// router-w4 — `lifecycle_routes` block on a lifecycle-bearing
    /// resource. Declares a state→URL table that codegen lowers into
    /// a per-resource helper function (`<resource>LifecycleRoute(state)`).
    /// Route `requires_lifecycle X = state` / `on_lifecycle_pending
    /// dispatch_via X.lifecycle_route` consumes the helper to redirect
    /// when the actor's row is at a different lifecycle state.
    /// `None` for resources without a lifecycle routes table; doctor
    /// will eventually warn when a route declares `dispatch_via
    /// X.lifecycle_route` against a resource that doesn't have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_routes: Option<LifecycleRoutes>,
}

/// router-w4 — `lifecycle_routes` block on a Resource. Maps every
/// lifecycle state name (plus `none` for "no row" / `*` for the
/// catch-all wildcard) to a URL string. Codegen emits a per-resource
/// `<resource>LifecycleRoute(state: string | null): string` helper
/// that the routes.gen.tsx beforeLoad closures call when the route's
/// `requires_lifecycle` gate fails.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LifecycleRoutes {
    /// Ordered arms — state name → URL. Insertion order preserved
    /// for deterministic codegen output. The special keys `none`
    /// (missing row / 404) and `*` (wildcard fallback) are accepted
    /// alongside lifecycle state names.
    pub arms: Vec<LifecycleRouteArm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleRouteArm {
    /// Match key — lifecycle state name, `none`, or `*`.
    pub state: String,
    /// URL string the helper returns when the actor's row is in this
    /// lifecycle state.
    pub url: String,
}

/// Closed catalog of resource-level convention bundles. Adding a
/// variant is an IR change requiring a proposal; the parser MUST
/// reject any identifier not in this enum.
///
/// See `docs/proposals/ir-resource-conventions-crud.md` §4.2 and
/// `docs/proposals/ir-resource-conventions-me.md` §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConventionRef {
    /// `crud` — auto-synthesizes 3 commands + 2 queries (5 entries
    /// total) per `ir-resource-conventions-crud.md` §5.1.
    Crud,
    /// `me` — auto-synthesizes one `lookup_my_<resource>` query keyed
    /// by `ctx.User.ID` (or `ctx.User.OrgID` for org-only resources).
    /// See `ir-resource-conventions-me.md` §5.
    Me,
    // Future variants (NOT in this proposal):
    //   Timestamped, PiiAware, SoftDelete, Slugged, Paginated.
}

/// Origin of a synth-eligible entry name in `Feature.synth_origins`.
///
/// Cell C3's synthesis pass marks each name that a convention bundle
/// would have produced. The marker distinguishes the two relevant
/// states the inspect surface (§11) renders:
///
/// * `Synthesized(<bundle>)` — C3 appended this command/query as part
///   of the named bundle's expansion. Inspect annotates with
///   `[conv:<bundle>]`.
/// * `AuthorOverride(<bundle>)` — the name was in the bundle's set but
///   an author-written command/query already existed with the same
///   name. C3 skipped its synthesis. Inspect annotates with
///   `[author override; convention skipped]`.
///
/// Entries C3 does not touch (pure author-written commands not in any
/// convention's set) carry no entry in `synth_origins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "convention", rename_all = "snake_case")]
pub enum ConventionOrigin {
    /// Entry was synthesized by the named bundle.
    Synthesized(ConventionRef),
    /// Author wrote this name; the named bundle would have synthesized it.
    AuthorOverride(ConventionRef),
}

impl ConventionOrigin {
    /// The bundle that produced (or would have produced) this entry.
    pub fn convention(&self) -> ConventionRef {
        match self {
            ConventionOrigin::Synthesized(c) | ConventionOrigin::AuthorOverride(c) => *c,
        }
    }

    /// `true` when an author wrote a command/query with this name and the
    /// convention's synth for that name was skipped.
    pub fn is_author_override(&self) -> bool {
        matches!(self, ConventionOrigin::AuthorOverride(_))
    }
}

/// Roadmap §1.5 (CL.C.2) — `lock` resource-level decorator. Closed
/// catalog of three concurrency-control strategies:
///
/// - `Optimistic { version_field }` — writes carry a `WHERE version = ?`
///   check; the version column is monotonic. Doctor enforces that
///   `version_field` names an existing `Integer` field on the resource.
/// - `Pessimistic` — runtime acquires an advisory or row-level lock
///   for the whole transaction (`SELECT ... FOR UPDATE` style).
/// - `RowLevel` — `FOR UPDATE` per-row at read time. Distinct from
///   `Pessimistic` (which is broader); rendered as a runtime hint
///   today.
///
/// DDL impact: `Optimistic` requires the version column to exist (an
/// authored field; doctor would have already verified the reference).
/// `Pessimistic` and `RowLevel` produce no DDL change — the runtime
/// applies `FOR UPDATE` at execution time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LockSpec {
    Optimistic { version_field: String },
    Pessimistic,
    RowLevel,
}

/// Roadmap §1.5 (CL.C.2) — `composite_key` resource-level decorator.
/// Lists the fields participating in the key and whether the key is
/// the table's `PRIMARY KEY`. When `primary == false`, today this is
/// equivalent to a unique constraint declaration — kept as a separate
/// construct so future evolutions (e.g. clustered index, partitioning)
/// have a place to land without re-purposing `Constraint::Unique`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeKey {
    pub fields: Vec<String>,
    /// `primary true` — replace the implicit `id BIGSERIAL PRIMARY
    /// KEY` with `PRIMARY KEY (<fields>)`. When `false`, the implicit
    /// row identity is kept and the composite is emitted as a UNIQUE
    /// constraint.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
}

/// Phase L Tier 4c — retention policy lifted from
/// `retention <duration> then <action>`. Duration stays verbatim
/// (adapter parses), action is a closed catalog so doctor can pin a
/// finite ruleset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionSpec {
    pub duration: String,
    pub action: RetentionAction,
}

/// Phase L Tier 4d — typed value record. Mirrors `Resource` minus the
/// tenancy / constraint / soft-delete machinery; used for SQL-query
/// return types, agent discriminated-record outputs, and other typed
/// projection bags. Distinct from `Resource` (no persistence axis) and
/// from `ContractRecord` (cross-feature contracts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    pub fields: Vec<Field>,
    /// `discriminator <field>` marker — `Some(field_name)` when the
    /// record carries a tagged-union discriminator (Cut A.6). Lowering
    /// finds the field on the surface; doctor cross-checks the enum
    /// type via `output discriminator <Record>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminator_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    Anonymize,
    Delete,
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_ref: TypeRef,
    pub required: bool,
    pub unique: bool,
    /// CL.C.4 — `@slug` field decorator. When `true` the field is the
    /// resource's URL slug column; codegen emits a unique index +
    /// case-insensitive lookup. Doctor enforces implicit uniqueness
    /// via `slug-uniqueness-implicit`. Additive boolean: pre-CL.C.4
    /// fields deserialize with `slug == false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub slug: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    /// Phase L Tier 4c — `<name>: <Type> derived from <expr>` lifts
    /// the computed-field expression. The analyzer keeps the verbatim
    /// expression text since `Expr` doesn't yet model comparison
    /// operators outside the predicate sublanguage; doctor reads the
    /// text for cross-field resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    /// L0 #3 §10 — inline field constraints emitted to Zod, Go
    /// validator tags, and (in a follow-up) OpenAPI. Six closed
    /// catalog keywords (`min N`, `max N`, `pattern STRING`,
    /// `between A and B`, `length N`, `in [...]`). Combination
    /// rules + default-value compatibility are enforced at lowering
    /// (see `lazuli_analyzer::AnalyzeError::ConstraintConflict` and
    /// `::DefaultViolatesConstraint`).
    #[serde(default, skip_serializing_if = "FieldConstraints::is_empty")]
    pub constraints: FieldConstraints,
    /// Roadmap §1.5 (CL.C.2) — `@full_text` field decorator. When
    /// `true`, the migration emitter adds a Postgres GIN index over
    /// the `to_tsvector('english', <field>)` projection so the runtime
    /// can do `tsvector` full-text search. Doctor enforces that the
    /// field's type is text-like (Text or `@semantic.*` string).
    #[serde(default, skip_serializing_if = "is_false")]
    pub full_text: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// FR-PII-STACK — orthogonal PII annotation. When set, the
    /// observability redactor masks this field's values in log
    /// output AND audit data_subject inference may consume it.
    /// Distinct from `type_ref` being CapabilityRef::PII — that
    /// path is for fields that are ONLY PII (no semantic carrier).
    /// This slot lets `@semantic.BrazilianCPF` + `@cap.PII` stack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pii: Option<PiiCapability>,
    /// `ir-resource-conventions-owner-scope` §7.2 — `@owner_axis(through:
    /// <column>)` field annotation. Marks this FK field as the
    /// ownership-chain hop: the `conventions [crud]` / `[me]` synth
    /// passes (O2) restrict every emitted command to rows where
    /// `<field>.<through_column>` equals `ctx.User.ID`. Absent =
    /// tenant-only scope (today's default). Additive; pre-O1 IR
    /// snapshots deserialize with `owner_axis == None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_axis: Option<OwnerAxis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `ir-resource-conventions-owner-scope` §7.2 — typed payload for the
/// `@owner_axis(through: <column>)` field annotation. `through_column`
/// is the column on the FK target resource that holds the actor key
/// (`user` for Hostpoint's Property → Host chain). Multi-hop chains
/// are deferred per §13; v0 captures one hop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerAxis {
    pub through_column: String,
}

/// `ir-resource-conventions-owner-scope.md` §7.3 + §8.5.A — analyzer
/// synth output for owner-scope mode. Carries the SQL fragment the
/// analyzer composes once at synth time so downstream codegen can
/// emit it verbatim. One field per shape produced by the unified
/// builder: `where_predicate` for DELETE/UPDATE/LOOKUP/LIST tails,
/// `cte_owner_check` for the CREATE-side CTE prefix per §8.5.A.
///
/// **RULE-VOCAB-03**: this is a passive metadata container — codegen
/// pastes the captured string into the lowered SQL. No runtime
/// branching is introduced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerScopeSql {
    /// Field name on the resource that bears `@owner_axis`. Stored for
    /// inspect annotations (O3) and to support multi-axis composition
    /// in future cycles. Example: `"host"` for Hostpoint Property.
    pub field_name: String,
    /// FK target resource name (PascalCase). Codegen lowers this to
    /// the snake-cased table identifier. Example: `"Host"`.
    pub fk_target: String,
    /// The `through:` column on the FK target — typically `"user"`.
    pub through_column: String,
    /// Pre-composed predicate fragment used by DELETE / UPDATE /
    /// LOOKUP / LIST. Example:
    /// `host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)`.
    pub where_predicate: String,
    /// Pre-composed CTE prefix for `create_<resource>` per §8.5.A.
    /// Example: `WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $host AND "user" = ctx.User.ID)`.
    /// `None` when this slot is attached to a Lookup/List/Delete/Update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cte_owner_check: Option<String>,
}

/// L0 #3 §10 — inline field constraints. Each slot is `Option` so an
/// absent constraint serializes off via `is_empty`. Combination rules
/// (§10.2) and default-value compatibility (§10.3) are checked in
/// `lazuli_analyzer` at lowering; this struct is a passive container.
///
/// `r#in` carries values as strings; numeric-typed `in [...]` values
/// are parsed on the consumer side (Go emitter / Zod emitter). This
/// avoids splitting the field per-type and keeps the wire shape
/// stable across numeric / text variants.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FieldConstraints {
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
    /// `validate sanitize_html(<profile>)` — runtime strips dangerous
    /// HTML before persist. `profile` is a closed catalog of named
    /// rule sets (`strict`, `basic`, `markdown_safe`). None means no
    /// sanitization is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitize_html: Option<SanitizeHtmlProfile>,
    /// `validate utf8_safe` — reject control chars + invalid UTF-8
    /// sequences. Cheap guard against subtle injection vectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utf8_safe: Option<bool>,
    /// `validate max_recursion:<n>` — for JSON/JSONB fields, cap
    /// nesting depth. Mitigates OOM via crafted deeply-nested input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recursion: Option<u32>,
    /// `validate max_size:<n>` — cap field byte-length at persist
    /// time (distinct from upload-stream cap on `@cap.File`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    /// `validator covers_pii` — declares the validator function
    /// covers a known PII shape. References the validator catalog
    /// entry by snake_case name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_pii: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SanitizeHtmlProfile {
    /// Strip ALL tags + decode entities. Use for plain-text fields
    /// that briefly accept rich input from rich-text editors.
    Strict,
    /// Allow `<b>`, `<i>`, `<em>`, `<strong>`, `<a href>`, `<br>`,
    /// `<p>`. Strip script/style/iframe/object/embed.
    Basic,
    /// Add markdown-friendly: `<code>`, `<pre>`, `<blockquote>`,
    /// `<ul>`, `<ol>`, `<li>`, `<h1..h6>`. Still strips all
    /// script-bearing tags + on* attributes.
    MarkdownSafe,
}

impl FieldConstraints {
    /// `true` when no constraint is set. Used by serde to skip the
    /// whole struct from JSON output (keeps `Module` byte-for-byte
    /// stable for declarations without inline constraints).
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

    /// Convenience constructor used by tests and call sites that build
    /// the struct from scratch without serde.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Closed catalog of type references. Strings are forbidden; the analyzer
/// decides which variant a syntactic type name resolves to. Unrecognised
/// names become `TypeRef::Unresolved` so downstream consumers can surface a
/// targeted diagnostic without crashing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TypeRef {
    Builtin(BuiltinType),
    UserDefined(QualifiedName),
    EnumRef(QualifiedName),
    Many(Box<TypeRef>),
    Unresolved(String),
    /// Phase L Tier 2 — capability decorators with structured
    /// arguments (`@cap.File(max_size:...,accept:...)`). Today only
    /// `File` is typed; other `@cap.*` decorators (`Hashed`,
    /// `Encrypted`, `Token`) stay as text-pattern in LSP and project
    /// through `Unresolved`/`UserDefined` until the cycle that types
    /// them lands.
    Capability(CapabilityRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinType {
    Id,
    Text,
    Boolean,
    Integer,
    Decimal,
    Date,
    DateTime,
    Json,
    SemanticEmail,
    /// Per `docs/proposals/semantic-types-money-brazilian.md` v0.3 +
    /// MONEY-1 §3.2 of the hostpoint roadmap. Carries the declared ISO
    /// 4217 currency so downstream doctor checks (MONEY-COMPARE-001,
    /// MONEY-ARITHMETIC-001) can reject mixed-currency operations at
    /// analyse time without re-walking surface text. The default
    /// authoring shorthand `Money` lowers to `SemanticMoney { currency:
    /// BRL }` (Hostpoint-pilot reality); explicit
    /// `@semantic.Money(currency: <ISO>)` overrides.
    SemanticMoney {
        currency: CurrencyCode,
    },
    /// Phase L Tier 4 follow-up — `@semantic.Phone`. Closed catalog
    /// addition so auth-identity diagnostics can read the shape
    /// without text-walking.
    SemanticPhone,
    /// Phase L Tier 4 follow-up — `@semantic.Url`.
    SemanticUrl,
    /// Phase L Tier 4 follow-up — `@semantic.Uuid`.
    SemanticUuid,
    /// Currency follow-up — `@semantic.Currency`. ISO 4217 3-letter
    /// uppercase code (`USD`, `BRL`). Pairs with `SemanticMoney` for
    /// typed amount-currency tuples; emitter maps to Go `lazuli.Currency`
    /// alias (already exists in `runtime/go/lazuli/types.go`).
    SemanticCurrency,
    /// GeoPoint follow-up (2026-05-11) — `@semantic.GeoPoint`.
    /// Closed-catalog single semantic carrying `{ lat, lng }`. Required
    /// by `codegen-lazuli-go.md` §6.3/§9.1 to materialise as
    /// `postgis.Point` in generated Go + drive the `GIST` index
    /// emission in DDL migrations.
    SemanticGeoPoint,
    /// B3 — plugin-contributed `@semantic.<Name>` resolved through a
    /// plugin's `manifest.toml`. The IR layer is locale-agnostic: it
    /// knows only the declaring plugin namespace (`@lazuli/plugin-scalars-br`),
    /// the manifest-local alias terminal name (`BrazilianCPF`), the
    /// carrier built-in (currently always `Text`), and the validator
    /// function name from the manifest. Codegen reads the validator
    /// to build the `<plugin-short>.<validator>` go-playground tag
    /// without re-reading the manifest at emission time. The plugin
    /// owns checksum rules, formatting, and any upstream library. See
    /// `docs/proposals/semantic-types-plugin-locales.md`.
    SemanticPluginType {
        plugin: String,
        name: String,
        carrier: Box<BuiltinType>,
        /// Exported Go function on the plugin adapter (e.g.
        /// `ValidateCPF`). Carried so codegen can emit the validate
        /// tag without re-reading `manifest.toml`. Authoritative
        /// source is the plugin's manifest `[[semantic_types]].validator`
        /// — the resolver pass copies it here at lift time.
        validator: String,
        /// W2 (ir-semantic-auto-validate-2026-05-22): effective Go
        /// module path of the plugin (`lazuli.dev/plugin/scalars-br`).
        /// Plugin-level value or convention fallback. Empty when the
        /// IR predates W2 lift.
        #[serde(default)]
        go_module: String,
        /// W2: effective TS/npm package (`@lazuli/plugin-scalars-br`).
        #[serde(default)]
        ts_package: String,
        /// W2: effective error code surfaced on validation_failed
        /// (`cpf_invalid`).
        #[serde(default)]
        error_code: String,
        /// W2: optional i18n message key. Empty when not declared.
        #[serde(default)]
        message_key: String,
        /// W2: TS validator function (`validateCPF`). Empty when not
        /// declared — TS preflight emission is skipped.
        #[serde(default)]
        ts_validator: String,
    },
    CapSecret,
    /// Deprecated: the flat `CapFile` variant never carried arguments.
    /// Phase L Tier 2 introduces `TypeRef::Capability(CapabilityRef::File(...))`
    /// which carries the parsed `max_size`/`accept`/`visibility`/`signed_ttl`
    /// slots. Kept for back-compat with serialized payloads predating the
    /// typed shape.
    CapFile,
}

/// MONEY-1 §3.2 — closed-catalog ISO 4217 codes the language understands
/// at IR time. Expansion is additive: new currencies land here when a
/// pilot demands them. Other ISO codes the user might type fall through
/// to the analyzer's "unknown currency" diagnostic and never reach IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CurrencyCode {
    BRL,
    USD,
    EUR,
    GBP,
    JPY,
    CHF,
}

impl CurrencyCode {
    /// Canonical 3-letter ISO 4217 form (`"BRL"`, `"USD"`...). Used by
    /// codegen to emit the `CHECK (<col> = '<ISO>')` constraint and by
    /// doctor diagnostics when interpolating into messages.
    pub fn as_iso(&self) -> &'static str {
        match self {
            Self::BRL => "BRL",
            Self::USD => "USD",
            Self::EUR => "EUR",
            Self::GBP => "GBP",
            Self::JPY => "JPY",
            Self::CHF => "CHF",
        }
    }

    /// Parse a 3-letter ISO 4217 code into the closed catalog. Returns
    /// `None` for unknown codes; the analyzer surfaces that as a typed
    /// diagnostic rather than silently accepting it.
    pub fn from_iso(raw: &str) -> Option<Self> {
        match raw {
            "BRL" => Some(Self::BRL),
            "USD" => Some(Self::USD),
            "EUR" => Some(Self::EUR),
            "GBP" => Some(Self::GBP),
            "JPY" => Some(Self::JPY),
            "CHF" => Some(Self::CHF),
            _ => None,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    pub kind: CommandKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<RouteSlot>,
    pub input: CommandInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lets: Vec<LetBinding>,
    pub effect: CommandEffect,
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form when the authored
    /// policy contained predicates (`has_role` / `has_permission` /
    /// `authenticated`) or boolean combinators. Coexists with `policy`
    /// (legacy atom ref) for back-compat. `None` when the policy is
    /// a bare atom or absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-command override for the `policy_denied`
    /// error message. Highest-precedence step in the resolution chain
    /// (proposal §2.E step 1). When `Some`, the codegen emits a
    /// per-command `ErrorKeys.PolicyDenied` entry and the runtime
    /// resolver picks it before consulting `PolicyCategory.when_denied`
    /// or `FeatureErrors.messages`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// `rate_limit "<N per period per scope>"` declaration, optionally
    /// env-qualified per `ir-rate-limit-env-aware` (cell 1). The single-
    /// line `rate_limit "X"` source shape lowers to `RateLimitSpec {
    /// default: "X", by_env: [] }`; multi-line shapes populate `by_env`.
    /// Captured verbatim and parsed by adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
    /// Phase L Tier 4b — `audit <subject>, <subject>, ...` with optional
    /// `emit_to <event_group>` child.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
    /// Phase L Tier 4b — Cut A.9 `approval` block. Replaces the
    /// `CommandApprovalFact` text-walker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalSpec>,
    /// Phase L Tier 4b — `invalidates query.<name>(...)` references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates: Vec<InvalidatesSpec>,
    /// Phase L Tier 4b — `calls <slot>.<op>` inside a command body.
    /// Mirrors `Job.external_calls`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_calls: Vec<ExternalCallRef>,
    /// Phase L Tier 4 follow-up — `timeout "<duration>"` literal.
    /// Mirrors `Job.timeout`. Adapter parses; language keeps verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Phase L Tier 4 follow-up — `retry <count> [backoff <strategy>]`.
    /// Mirrors `Job.retry`. Doctor cross-checks against `external_calls`
    /// to enforce `INT-CALL-RETRY-001`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    /// Phase L Tier 4 follow-up — `idempotency by <field>[, ...]`.
    /// Mirrors `Job.idempotency`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    /// `write_window by <path> within <duration_or_ref>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_window: Option<CommandWriteWindow>,
    /// OpenAPI bucket cycle — `deprecated [since "..." replacement <ref>
    /// sunset "..."]`. `None` for live commands; `Some` for those flagged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecation>,
    /// `handler @fn.<name>` or `handler "./path.go"` escape hatch.
    /// `None` for commands fully described by their declarative body
    /// (`creates`/`updates`/`deletes`/`returns` with no user code).
    ///
    /// When set with `effect == None`, the runtime treats the command as
    /// a pure-read invocation routed to the user's Go handler — codegen
    /// emits `Effect: lazuli.Returns(<PascalCase(name)>)` instead of
    /// `Effect: nil` (closes WAR-RUNTIME-COMMAND-01 Effect half).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<HandlerRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// Lifecycle transitions this command fires, in order. Empty = no
    /// lifecycle binding. Multi-element = chain that runs in one tx
    /// (pre-guard = transitions[0].from, post-update = last.to).
    /// See docs/proposals/ir-command-transition-binding.md.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    /// FR-3a — marker set on commands that the analyzer synthesized
    /// from a `@cap.File(...)` field on a per-user resource. `None`
    /// for author-written commands. Codegen reads this to wire the
    /// auto-photo runtime helper instead of expecting a hand-rolled
    /// handler. See docs/proposals/fileref-jsonb-fr3-design.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesized_from_cap_file: Option<SynthesizedFromCapFile>,
    /// `ir-resource-conventions-owner-scope.md` §7.3 + §8.5.A —
    /// owner-scope SQL fragment composed by the analyzer at synth
    /// time. `Some` only when this command was emitted by the crud /
    /// me synth pass for a resource carrying a `@owner_axis(through:
    /// <col>)` field. Codegen passes the fragments through verbatim
    /// — there is no runtime branching. See Cell O2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandWriteWindow {
    pub by: Path,
    pub within: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// OpenAPI bucket cycle — typed deprecation marker for commands (and
/// post-Tier-4 apis). All sub-fields are optional; bare `deprecated`
/// surfaces as `Deprecation::default()`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deprecation {
    /// Authored `since "<version>"`. Free-form (semver, calendar,
    /// git-sha) — emitted verbatim under `x-lazuli-deprecated-since`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Authored `replacement <ref>` resolved at lowering. `None` when
    /// omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<DeprecationReplacement>,
    /// Authored `sunset "<YYYY-MM-DD>"` — ISO-8601 date. Format-checked
    /// at lowering; doctor warns if in the past.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset: Option<String>,
}

/// Closed catalog of replacement reference shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DeprecationReplacement {
    /// `replacement <command_name>` — same-feature short form.
    LocalCommand(String),
    /// `replacement api.<name>` on an api — same-feature short form.
    LocalApi(String),
    /// `replacement <feature>.command.<name>` — cross-feature.
    Qualified(QualifiedName),
    /// `replacement <feature>.api.<name>` — cross-feature api.
    QualifiedApi(QualifiedName),
    /// `replacement "https://..."` — explicit URL escape hatch.
    Url(String),
}

/// Phase L Tier 4b — declarative audit spec captured from a command's
/// `audit <subject>, <subject>, ...` line and optional `emit_to <group>`
/// child. The subject strings stay verbatim (`actor`, `target.id`,
/// `input.owner_id`) because the analyzer resolves them against the
/// command's input slots; doctor cross-checks `emit_to` against the
/// feature's `event_group` declarations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSpec {
    /// `actor`, `target.id`, `input.<field>`, etc. Each entry is a single
    /// subject reference.
    pub subjects: Vec<String>,
    /// `emit_to <event_group>` — optional event-group emission target.
    /// `None` when the command writes audit without emitting to a group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_to: Option<String>,
    /// LGPD/GDPR shape — names the field on the affected resource
    /// that identifies the data subject for right-of-access /
    /// right-to-erasure queries. `None` for non-personal-data
    /// commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_subject: Option<String>,
    /// `audit before` — capture pre-mutation field values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_before: bool,
    /// `audit after` — capture post-mutation field values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_after: bool,
    /// `audit retain <duration>` — retention horizon for audit rows.
    /// `None` = use feature/registry default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_for: Option<String>,
}

/// Phase L Tier 4b — Cut A.9 `approval` block lifted into IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSpec {
    /// `required_when <predicate>` — verbatim predicate text. The
    /// predicate language tracks the command target's resource fields;
    /// the analyzer keeps the raw form so future Cut A.9 evolutions can
    /// land without an IR churn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_when: Option<String>,
    /// `by @role.<name>` or `by @actor.<name>` — single approver atom.
    pub by: String,
    /// `timeout "24h"` — duration literal parsed by the adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// `then deny | allow | escalate` — closed catalog of resolutions.
    pub then: ApprovalThen,
}

/// Phase L Tier 4b — closed catalog of approval timeout resolutions
/// (Cut A.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalThen {
    Deny,
    Allow,
    Escalate,
}

/// Phase L Tier 4b — `invalidates query.<name>(args)` reference.
/// `query` carries the qualified query name and `args` the explicit
/// named-argument bindings (e.g. `id: route.id`). Doctor uses this for
/// cache-invalidation cross-checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidatesSpec {
    pub query: QualifiedName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<NamedArg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    Create,
    Update,
    Delete,
    Returns,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSlot {
    pub name: String,
    pub type_ref: TypeRef,
    /// Phase L Tier 4 follow-up — `route <name>: <Type> from ctx.<expr>`
    /// captures the optional default-binding expression. `Some(text)`
    /// means the slot has a context default; `None` means the caller
    /// must supply it. Doctor's command-route-binding check reads this
    /// to suppress missing-argument diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// `route opaque token: Text` -> `OpaqueToken`.
    /// `route signed_token`       -> `SignedToken`.
    /// Plain `route id: ID`       -> `Plain` (default).
    #[serde(default, skip_serializing_if = "is_plain_route_slot_kind")]
    pub kind: RouteSlotKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSlotKind {
    Plain,
    OpaqueToken,
    SignedToken,
}

impl Default for RouteSlotKind {
    fn default() -> Self {
        RouteSlotKind::Plain
    }
}

fn is_plain_route_slot_kind(kind: &RouteSlotKind) -> bool {
    matches!(kind, RouteSlotKind::Plain)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandInput {
    /// Short list — every entry maps 1:1 to a field on the command's local
    /// `creates`/`updates` resource.
    Short(Vec<String>),
    /// Typed block — explicit name/type pairs.
    Typed(Vec<TypedSlot>),
    /// Empty inputs (`delete` commands often have none).
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedSlot {
    pub name: String,
    pub type_ref: TypeRef,
    pub required: bool,
    /// L0 #3 §10 — inline constraints carried on command input
    /// slots (`input` block). Mirrors `Field::constraints` so Zod
    /// schemas for command inputs, Go validator tags on
    /// `<Cmd>Input` structs, and OpenAPI parameter schemas pick up
    /// the same six-keyword catalog without a parallel field.
    #[serde(default, skip_serializing_if = "FieldConstraints::is_empty")]
    pub constraints: FieldConstraints,
    /// LAZ-SEMANTIC-AUTO-VALIDATE W2 — `@validate.skip` annotation on
    /// the slot. Codegen skips emitting the semantic-scalar runtime
    /// validation pre-pass for this field, even when the
    /// `@semantic.X` type declares a validator. Used for migration /
    /// legacy import flows where authors knowingly accept invalid
    /// scalar values. Doctor SEMANTIC-PLUGIN-002 stays silent when set.
    #[serde(default, skip_serializing_if = "is_false")]
    pub validate_skip: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetExpr {
    pub query: QualifiedName,
    pub args: Vec<NamedArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedArg {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandEffect {
    Creates(CreateEffect),
    Updates(UpdateEffect),
    Deletes(DeleteEffect),
    /// Pure request/response command — declares `returns` instead of an effect.
    Returns(ReturnsEffect),
    /// No effect declared yet (legacy lowering path).
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEffect {
    pub resource: QualifiedName,
    /// True when the command body uses `creates X from input`.
    pub from_input: bool,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateEffect {
    pub resource: QualifiedName,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteEffect {
    pub resource: QualifiedName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnsEffect {
    pub return_type: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub field: String,
    pub value: Expr,
}

/// Policy reference. `Local` = feature-local policy category. `Atom` = closed
/// `@role.*`/`@scope.*`/`@actor.*` namespace. `External` = `<feature>.<name>`.
/// `Unresolved` covers legacy strings until full lowering lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyRef {
    Local(String),
    Atom(String),
    External {
        feature: String,
        name: String,
    },
    Unresolved(String),
    #[default]
    None,
}

impl PolicyRef {
    /// Returns `true` when the policy is unset. Used by serde's
    /// `skip_serializing_if` on query / command IR fields so absent
    /// per-callable policies serialize cleanly and round-trip back to
    /// `PolicyRef::None` (the explicit "feature-default applies" marker).
    pub fn is_none(&self) -> bool {
        matches!(self, PolicyRef::None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Query {
    List(ListQuery),
    Lookup(LookupQuery),
    Sql(SqlQuery),
}

impl Query {
    pub fn name(&self) -> &str {
        match self {
            Query::List(q) => &q.name,
            Query::Lookup(q) => &q.name,
            Query::Sql(q) => &q.name,
        }
    }
}

/// Cache bucket cycle — typed `cache` block on `query.list` / `query.sql`.
/// Adapters parse `key` verbatim; `ttl` is closed-catalog literal or
/// quoted prose; `tags`/`namespace` are author-defined labels used for
/// fan-out invalidation cross-checks.
///
/// Two shapes coexist on a query:
///  * Inline (legacy `cache { key, ttl, tags?, namespace? }`) — every
///    decorator authored at the query site.
///  * Profile reference (`cache <name>`) — `profile_ref: Some(name)` and
///    the resolved `key`/`ttl`/... mirror the feature-level
///    [`CacheProfile`] body. Lowering copies the body into this struct
///    so codegen / runtime never have to redo the lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCache {
    /// `cache key <expr>` — opaque template stored verbatim.
    pub key: String,
    /// `cache ttl <literal>` — typed duration or quoted prose.
    pub ttl: CacheTtl,
    /// `cache tags <label>[, <label>...]` — lowercase identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `cache namespace <label>` — single label; `None` defaults to the
    /// feature name at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `cache <profile_name>` reference form. `None` for the inline
    /// shape; `Some(name)` when the query referenced a feature-level
    /// [`CacheProfile`] by name (`cache product_view`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<String>,
}

/// Cache bucket cycle (CL.C.3) — feature-level `cache <name>` profile.
///
/// First-class declaration sibling to `job`/`webhook`/`notification`.
/// Queries opt into a profile via `cache <profile_name>`; the inline
/// `cache { key, ttl, ... }` shape on a query keeps working for
/// one-off ttl/key pairs that don't need a named profile.
///
/// The four decorators beyond `key/ttl/tags/namespace`
/// (`stale_while_revalidate`, `coalesce`, `sliding`) carry the
/// runtime contract:
///  * `stale_while_revalidate` — serve stale value up to N seconds
///    after expiry while a background refresh runs (RFC 5861-flavoured).
///  * `coalesce` — single-flight populate when multiple readers miss
///    the same key concurrently.
///  * `sliding` — every read extends the TTL window.
///
/// The runtime owns the execution semantics; this struct declares the
/// contract typed end-to-end so doctor/codegen can cross-check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheProfile {
    /// Profile identifier. Lowercase identifier; dash-separated allowed.
    pub name: String,
    /// `key <expr>` — opaque template stored verbatim. Required.
    pub key: String,
    /// `ttl <literal>` — typed duration or quoted prose. Required.
    pub ttl: CacheTtl,
    /// `namespace <label>` — single label; `None` defaults to the
    /// feature name at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `tags <label>[, <label>...]` — lowercase identifiers used by
    /// `invalidates tag:<label>` on commands for fan-out invalidation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `stale_while_revalidate <duration>` — typed duration. `None`
    /// means the runtime refreshes synchronously on TTL miss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_while_revalidate: Option<CacheTtl>,
    /// `coalesce <bool>` — single-flight populate on concurrent miss.
    /// `None` means "runtime default" (today: false). The boolean is
    /// the language-visible knob; per-key window tuning is a runtime
    /// adapter concern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce: Option<bool>,
    /// `sliding <bool>` — sliding TTL semantics (read extends expiry).
    /// `None` means fixed TTL. Requires `ttl` (doctor enforces).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sliding: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Cache bucket cycle — `ttl` literal or quoted prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CacheTtl {
    /// `ttl 5m` — typed literal with closed unit catalog.
    Literal(CacheTtlLiteral),
    /// `ttl "5 minutes"` — adapter-parsed prose, preserved verbatim.
    Quoted(String),
}

/// Cache bucket cycle — typed duration literal. Closed catalog:
/// `s` (seconds), `m` (minutes), `h` (hours), `d` (days).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", content = "amount")]
pub enum CacheTtlLiteral {
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<OrderBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<String>,
    /// Cache bucket cycle — typed `cache` block (key/ttl/tags?/namespace?).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<QueryCache>,
    /// Per-query authored `policy @policy.<name>`. Mirrors
    /// `Command.policy`. `PolicyRef::None` (the default) means "fall
    /// back to the feature-level default policy" — preserving the
    /// pre-existing precedence for fixtures that never authored a
    /// per-query policy.
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form when the authored
    /// policy contained predicates (`has_role` / `has_permission` /
    /// `authenticated`) or boolean combinators. Coexists with `policy`
    /// (legacy atom ref) for back-compat. Mirrors `Command.policy_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-query override for the `policy_denied`
    /// error message. Highest-precedence step in the resolution chain
    /// for queries (proposal §3.3). Codegen wiring tracks the
    /// `Command` slot — queries are end-user-reachable HTTP boundaries
    /// in the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// `ir-resource-conventions-owner-scope.md` §7.3 + §8.4 — owner-
    /// scope WHERE fragment composed at synth time for `list_<r>`
    /// when the resource bears `@owner_axis`. See Cell O2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    pub keys: Vec<KeyClause>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// Per-query authored `policy @policy.<name>`. Mirrors
    /// `Command.policy`. `PolicyRef::None` (the default) means "fall
    /// back to the feature-level default policy".
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form. Mirrors
    /// `Command.policy_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-query override for the `policy_denied`
    /// error message. Same shape as `Command.policy_when_denied`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// `ir-resource-conventions-owner-scope.md` §7.3 + §8.3 — owner-
    /// scope WHERE fragment composed at synth time for `lookup_<r>`
    /// AND `lookup_my_<r>` when the resource bears `@owner_axis`. See
    /// Cell O2. `None` for author-written queries or tenant-only
    /// shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "SqlQueryKind::is_sql")]
    pub sql_kind: SqlQueryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    pub returns: TypeRef,
    pub sql_path: String,
    /// Cache bucket cycle — typed `cache` block. Same shape as
    /// `ListQuery.cache`. `LookupQuery` deliberately does not gain a
    /// cache slot (lookup caching is runtime-implicit; the fixture
    /// only authors cache on list/sql shapes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<QueryCache>,
    /// Per-query authored `policy @policy.<name>`. Mirrors
    /// `Command.policy`. `PolicyRef::None` (the default) means "fall
    /// back to the feature-level default policy".
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form. Mirrors
    /// `Command.policy_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-query override for the `policy_denied`
    /// error message. Same shape as `Command.policy_when_denied`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
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
pub struct Filter {
    pub predicate: Predicate,
    /// `Some(param_name)` for guarded `when params.X` filters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyClause {
    pub path: Path,
    pub equals: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderBy {
    pub field: String,
    pub direction: OrderDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDir {
    Asc,
    Desc,
}

/// Closed predicate sublanguage. See `docs/canonical-semantics.md` "Predicate
/// Expressions" for the ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Predicate {
    Comparison {
        left: Expr,
        op: CompareOp,
        right: Expr,
    },
    Has {
        collection: Expr,
        element: Expr,
    },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Eq,
    Ne,
    /// Cut A — ordered operators are admissible only inside an agent's
    /// `evals` block (proposal §A3). Doctor diagnostic
    /// `eval_ordered_op_invalid_diagnostics` rejects them outside that
    /// scope and when either side resolves to a non-numeric type.
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Expr {
    Path(Path),
    String(String),
    Integer(i64),
    Boolean(bool),
    Enum(EnumLiteral),
    Nil,
    /// `@fn.<name>(<arg>...)` — closes WAR-VOCAB-CREATES-FN-CALL-01.
    /// Declarative `creates`/`updates` field bindings can now invoke
    /// a user-registered extension fn at command time. Codegen lowers
    /// to a `lazuli.FromFn` source; runtime resolves args first, then
    /// invokes the registered fn with the resolved args.
    FnCall(FnCallExpr),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FnCallExpr {
    pub name: QualifiedName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    pub segments: Vec<String>,
}

impl Path {
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }
}

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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn lifecycle_round_trips_through_json() {
        let lc = Lifecycle {
            discriminator_field: "status".to_owned(),
            generated_enum: "PublicationStatus".to_owned(),
            states: vec![
                LifecycleState {
                    name: "scheduled".to_owned(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "published".to_owned(),
                    kind: LifecycleStateKind::Terminal,
                    span_ref: None,
                },
            ],
            transitions: vec![LifecycleTransition {
                name: "publish".to_owned(),
                from: vec!["scheduled".to_owned()],
                to: "published".to_owned(),
                policy: None,
                audit: None,
                timestamps: Some("published_at".to_owned()),
                emits: vec!["publication_published".to_owned()],
                requires: None,
                tests: None,
                previous_names: vec![],
                span_ref: None,
            }],
            invariants: vec![LifecycleInvariant::TerminalImmutable],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let json = serde_json::to_string(&lc).unwrap();
        let back: Lifecycle = serde_json::from_str(&json).unwrap();
        assert_eq!(lc, back);
        assert!(json.contains("\"kind\":\"terminal_immutable\""));
    }

    #[test]
    fn lifecycle_invariant_single_state_per_scope_tagged_correctly() {
        let inv = LifecycleInvariant::SingleStatePerScope {
            state: "gold".to_owned(),
            scope_field: "item_id".to_owned(),
        };
        let json = serde_json::to_string(&inv).unwrap();
        assert!(json.contains("\"kind\":\"single_state_per_scope\""));
        assert!(json.contains("\"state\":\"gold\""));
        assert!(json.contains("\"scope_field\":\"item_id\""));
    }

    #[test]
    fn semantic_plugin_type_round_trips_through_json() {
        // B3 — see `docs/proposals/semantic-types-plugin-locales.md`.
        let plugin = BuiltinType::SemanticPluginType {
            plugin: "@lazuli/plugin-scalars-br".to_owned(),
            name: "BrazilianCPF".to_owned(),
            carrier: Box::new(BuiltinType::Text),
            validator: "ValidateCPF".to_owned(),
            go_module: String::new(),
            ts_package: String::new(),
            error_code: String::new(),
            message_key: String::new(),
            ts_validator: String::new(),
        };
        let json = serde_json::to_string(&plugin).expect("serialize");
        let back: BuiltinType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(plugin, back);
        // Round-trip-stable. The variant name landing in the JSON keeps
        // the inspect serde-default path readable for cold readers.
        assert!(json.contains("SemanticPluginType"));
        assert!(json.contains("BrazilianCPF"));
        assert!(json.contains("ValidateCPF"));
    }

    #[test]
    fn resource_with_no_lifecycle_omits_field_in_json() {
        let r = Resource {
            name: "Publication".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: vec![],
            lifecycle_routes: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("\"lifecycle\""),
            "skip_serializing_if = Option::is_none should drop the field"
        );
    }

    #[test]
    fn convention_ref_crud_serializes_snake_case() {
        // §4.2 requires `#[serde(rename_all = "snake_case")]` so the
        // variant name on the wire matches the .lzi keyword (`crud`).
        let json = serde_json::to_string(&ConventionRef::Crud).unwrap();
        assert_eq!(json, "\"crud\"");

        let back: ConventionRef = serde_json::from_str("\"crud\"").unwrap();
        assert_eq!(back, ConventionRef::Crud);
    }

    #[test]
    fn convention_ref_me_serializes_snake_case() {
        // `ir-resource-conventions-me.md` §4.2 — the `Me` variant
        // serializes as `"me"` on the wire to match the .lzi
        // keyword and the parser catalog identifier.
        let json = serde_json::to_string(&ConventionRef::Me).unwrap();
        assert_eq!(json, "\"me\"");

        let back: ConventionRef = serde_json::from_str("\"me\"").unwrap();
        assert_eq!(back, ConventionRef::Me);
    }

    #[test]
    fn resource_conventions_round_trip_with_crud() {
        // Resource.conventions is the slot Cell C1 adds. Round-trip
        // through serde to lock the JSON shape before Cell C2 (parser)
        // starts producing it.
        let r = Resource {
            name: "Customer".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: vec![ConventionRef::Crud],
            lifecycle_routes: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            json.contains("\"conventions\":[\"crud\"]"),
            "expected the populated conventions list to serialize as snake_case identifiers, got: {json}"
        );
        let back: Resource = serde_json::from_str(&json).unwrap();
        assert_eq!(back.conventions, vec![ConventionRef::Crud]);
    }

    #[test]
    fn resource_conventions_absent_field_deserializes_empty() {
        // `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
        // means existing fixtures (pre-Cell-C1) that lack the field
        // continue to deserialize cleanly into an empty Vec.
        let legacy_json = r#"{
            "name": "Legacy",
            "soft_delete": false,
            "fields": []
        }"#;
        let r: Resource = serde_json::from_str(legacy_json).unwrap();
        assert!(r.conventions.is_empty());

        // Round-trip the empty Vec drops the key entirely.
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("\"conventions\""),
            "skip_serializing_if = Vec::is_empty should drop the field; got: {json}"
        );
    }
}

#[cfg(test)]
mod l0_6_ir_tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn round_trip<T>(value: &T)
    where
        T: Clone + PartialEq + std::fmt::Debug + Serialize + DeserializeOwned,
    {
        let encoded = serde_json::to_string(value).expect("serialize IR value");
        let decoded: T = serde_json::from_str(&encoded).expect("deserialize IR value");
        assert_eq!(*value, decoded);
    }

    fn query_ref(name: &str) -> QueryRef {
        QueryRef {
            feature: "item".to_string(),
            kind: QueryKind::Lookup,
            name: name.to_string(),
        }
    }

    fn command_ref(name: &str) -> CommandRef {
        CommandRef {
            feature: "item".to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn terminal_render_drawer_and_filter_ir_round_trip() {
        round_trip(&ListRender::Table {
            columns: vec!["title".to_string(), "updated".to_string()],
        });
        round_trip(&ListRender::Cells {
            slot: "item_card".to_string(),
        });

        let route_binding = DrawerRouteBinding {
            target: "key".to_string(),
            source: DrawerBindingSource::Selection,
        };
        round_trip(&route_binding);
        round_trip(&DrawerTrigger::ManualOpen);
        round_trip(&DrawerBindingSource::Selection);
        round_trip(&DrawerSubView {
            name: "item_detail".to_string(),
            trigger: DrawerTrigger::Select,
            source: query_ref("by_id"),
            route_binding: Some(route_binding),
            sections: vec!["header".to_string(), "metadata".to_string()],
            cells: vec![CellBinding {
                field: "related".to_string(),
                slot: "related_items".to_string(),
            }],
            actions: vec![command_ref("update"), command_ref("delete")],
            span_ref: Some(SpanRef { start: 10, end: 42 }),
        });

        round_trip(&FilterCardinality::Multi);
        round_trip(&FilterDecl {
            name: "tags".to_string(),
            type_ref: "Text".to_string(),
            cardinality: FilterCardinality::Multi,
            url_sync: true,
            span_ref: Some(SpanRef { start: 44, end: 60 }),
        });
    }

    #[test]
    fn terminal_search_sort_selection_and_setting_ir_round_trip() {
        let filter_binding = BindingRef::Filter {
            name: "slug".to_string(),
        };
        let source_binding = BindingRef::SourceInput {
            name: "q".to_string(),
        };
        round_trip(&filter_binding);
        round_trip(&source_binding);
        round_trip(&BindingRef::SelectionScalar);

        round_trip(&SearchMode::Columns {
            columns: vec!["title".to_string()],
        });
        round_trip(&SearchMode::Segmented);
        round_trip(&SearchField {
            key: "slug".to_string(),
            binds_to: filter_binding.clone(),
            span_ref: Some(SpanRef { start: 70, end: 88 }),
        });
        round_trip(&SearchDecl {
            mode: SearchMode::Segmented,
            fields: vec![SearchField {
                key: "slug".to_string(),
                binds_to: filter_binding,
                span_ref: None,
            }],
            free_text_target: Some(source_binding),
            span_ref: Some(SpanRef { start: 62, end: 96 }),
        });

        round_trip(&SortDir::Desc);
        round_trip(&SortDecl {
            allowed: vec!["title".to_string(), "updated".to_string()],
            default_field: "updated".to_string(),
            default_dir: SortDir::Desc,
            span_ref: Some(SpanRef {
                start: 100,
                end: 120,
            }),
        });

        round_trip(&SelectionMode::Multi);
        round_trip(&SelectionDecl {
            mode: SelectionMode::Multi,
            bulk_actions: vec![command_ref("delete")],
            span_ref: Some(SpanRef {
                start: 130,
                end: 150,
            }),
        });

        round_trip(&SettingValueSpace::Enum {
            values: vec!["sm".to_string(), "md".to_string(), "lg".to_string()],
        });
        round_trip(&SettingValueSpace::Bool);
        round_trip(&SettingValueSpace::Int { min: 1, max: 12 });
        round_trip(&SettingPersistence::Workspace);
        round_trip(&SettingDecl {
            name: "grid_size".to_string(),
            value_space: SettingValueSpace::Enum {
                values: vec!["sm".to_string(), "md".to_string(), "lg".to_string()],
            },
            default: "sm".to_string(),
            persistence: SettingPersistence::Local,
            span_ref: Some(SpanRef {
                start: 160,
                end: 190,
            }),
        });
    }

    #[test]
    fn terminal_tagged_enums_use_kind_discriminators() {
        assert_eq!(
            serde_json::to_value(ListRender::Cells {
                slot: "item_card".to_string()
            })
            .expect("serialize list render"),
            json!({ "kind": "cells", "slot": "item_card" })
        );
        assert_eq!(
            serde_json::to_value(SearchMode::Columns {
                columns: vec!["title".to_string()]
            })
            .expect("serialize search mode"),
            json!({ "kind": "columns", "columns": ["title"] })
        );
        assert_eq!(
            serde_json::to_value(BindingRef::SelectionScalar).expect("serialize binding ref"),
            json!({ "kind": "selection_scalar" })
        );
        assert_eq!(
            serde_json::to_value(SettingValueSpace::Int { min: 1, max: 9 })
                .expect("serialize setting value space"),
            json!({ "kind": "int", "min": 1, "max": 9 })
        );
    }

    #[test]
    fn terminal_optional_and_vec_fields_default_and_skip() {
        let search_json = r#"{"mode":{"kind":"segmented"}}"#;
        let search: SearchDecl = serde_json::from_str(search_json).expect("deserialize search");
        assert_eq!(search.fields, Vec::<SearchField>::new());
        assert_eq!(search.free_text_target, None);
        assert_eq!(search.span_ref, None);
        assert_eq!(
            serde_json::to_value(search).expect("serialize search"),
            json!({ "mode": { "kind": "segmented" } })
        );

        let drawer_json = r#"{
            "name":"item_detail",
            "trigger":"select",
            "source":{"feature":"item","kind":"lookup","name":"by_id"}
        }"#;
        let drawer: DrawerSubView = serde_json::from_str(drawer_json).expect("deserialize drawer");
        assert_eq!(drawer.route_binding, None);
        assert!(drawer.sections.is_empty());
        assert!(drawer.cells.is_empty());
        assert!(drawer.actions.is_empty());
        assert_eq!(drawer.span_ref, None);
        assert_eq!(
            serde_json::to_value(drawer).expect("serialize drawer"),
            json!({
                "name": "item_detail",
                "trigger": "select",
                "source": { "feature": "item", "kind": "lookup", "name": "by_id" }
            })
        );
    }

    #[test]
    fn on_success_spec_round_trips_and_skips_empty_slots() {
        let spec = OnSuccessSpec {
            back: true,
            redirect: Some("/host/property/{result.id}".to_owned()),
            flash: Some(FlashSpec {
                kind: "success".to_owned(),
                message_key: TranslationKeyRef {
                    key: "saved".to_owned(),
                    span_ref: None,
                },
            }),
            invalidates: vec![InvalidatesSpec {
                query: QualifiedName {
                    feature: Some("host".to_owned()),
                    name: "lookup_my_host".to_owned(),
                },
                args: vec![],
            }],
            replace: false,
        };
        round_trip(&spec);
        let value = serde_json::to_value(&spec).expect("serialize on_success");
        assert_eq!(value["back"], json!(true));
        assert_eq!(value["redirect"], json!("/host/property/{result.id}"));
        assert_eq!(value["flash"]["message_key"]["key"], json!("saved"));
        assert_eq!(
            value["invalidates"][0]["query"]["name"],
            json!("lookup_my_host")
        );
        assert!(value.get("replace").is_none());
    }

    // -------------------------------------------------------------------
    // ir-auth-refresh-rotation §3 — TheftAction + RotationConfig + AuthSessions
    // resolver methods. See docs/proposals/ir-auth-refresh-rotation.md.
    // -------------------------------------------------------------------

    fn user_session_qn() -> QualifiedName {
        QualifiedName {
            feature: Some("account".to_string()),
            name: "UserSession".to_string(),
        }
    }

    fn legacy_sessions() -> AuthSessions {
        AuthSessions {
            resource: user_session_qn(),
            ttl: "7 days".to_string(),
            refresh: false,
            extra_columns: Vec::new(),
            access_ttl: None,
            rotation: None,
        }
    }

    fn rotation_sessions(rotation: RotationConfig) -> AuthSessions {
        AuthSessions {
            resource: user_session_qn(),
            ttl: "7 days".to_string(),
            refresh: false,
            extra_columns: Vec::new(),
            access_ttl: None,
            rotation: Some(rotation),
        }
    }

    #[test]
    fn theft_action_round_trip_and_default() {
        round_trip(&TheftAction::RevokeSessionFamily);
        round_trip(&TheftAction::RevokeUser);
        assert_eq!(TheftAction::default(), TheftAction::RevokeSessionFamily);

        // snake_case at the wire — required for parity with .lzi keyword.
        assert_eq!(
            serde_json::to_value(TheftAction::RevokeSessionFamily).unwrap(),
            json!("revoke_session_family")
        );
        assert_eq!(
            serde_json::to_value(TheftAction::RevokeUser).unwrap(),
            json!("revoke_user")
        );
    }

    #[test]
    fn rotation_config_round_trips_with_all_slots() {
        round_trip(&RotationConfig {
            refresh_ttl: Some("30 days".to_string()),
            grace: Some("30 seconds".to_string()),
            theft_detection_action: Some(TheftAction::RevokeSessionFamily),
            span_ref: Some(SpanRef {
                start: 100,
                end: 200,
            }),
        });
    }

    #[test]
    fn rotation_config_round_trips_with_all_slots_absent() {
        // Empty rotation block: presence = enabled, all defaults kick in.
        round_trip(&RotationConfig {
            refresh_ttl: None,
            grace: None,
            theft_detection_action: None,
            span_ref: None,
        });
    }

    #[test]
    fn rotation_config_omits_none_fields_when_serialized() {
        let cfg = RotationConfig {
            refresh_ttl: None,
            grace: None,
            theft_detection_action: None,
            span_ref: None,
        };
        let v = serde_json::to_value(&cfg).unwrap();
        // Author wrote `rotation` block with no inner slots — the JSON
        // must be an empty object, not e.g. {"refresh_ttl": null, ...}.
        assert_eq!(v, json!({}));
    }

    #[test]
    fn auth_sessions_legacy_back_compat_deserializes() {
        // Pre-this-cell fixtures lack access_ttl + rotation. Confirm they
        // still deserialize cleanly with the new fields defaulting to None.
        let legacy_json = json!({
            "resource": { "feature": "account", "name": "UserSession" },
            "ttl": "7 days",
            "refresh": false
        });
        let parsed: AuthSessions =
            serde_json::from_value(legacy_json).expect("legacy fixture must deserialize");
        assert_eq!(parsed.ttl, "7 days");
        assert!(!parsed.refresh);
        assert!(parsed.access_ttl.is_none());
        assert!(parsed.rotation.is_none());
        assert!(!parsed.is_rotation_enabled());
    }

    #[test]
    fn auth_sessions_resolves_legacy_ttl_when_neither_set() {
        let s = legacy_sessions();
        assert_eq!(s.resolved_access_ttl(), "7 days");
        assert_eq!(s.resolved_refresh_ttl(), None);
        assert_eq!(s.resolved_rotation_grace(), None);
        assert_eq!(s.resolved_theft_action(), None);
    }

    #[test]
    fn auth_sessions_resolves_framework_defaults_when_rotation_on() {
        let s = rotation_sessions(RotationConfig {
            refresh_ttl: None,
            grace: None,
            theft_detection_action: None,
            span_ref: None,
        });
        assert!(s.is_rotation_enabled());
        // access_ttl=None + rotation on => "15 minutes" framework default.
        assert_eq!(s.resolved_access_ttl(), "15 minutes");
        assert_eq!(s.resolved_refresh_ttl(), Some("30 days"));
        assert_eq!(s.resolved_rotation_grace(), Some("30 seconds"));
        assert_eq!(
            s.resolved_theft_action(),
            Some(TheftAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn auth_sessions_resolves_explicit_values_when_set() {
        let mut s = rotation_sessions(RotationConfig {
            refresh_ttl: Some("14 days".to_string()),
            grace: Some("60 seconds".to_string()),
            theft_detection_action: Some(TheftAction::RevokeUser),
            span_ref: None,
        });
        s.access_ttl = Some("10 minutes".to_string());

        assert_eq!(s.resolved_access_ttl(), "10 minutes");
        assert_eq!(s.resolved_refresh_ttl(), Some("14 days"));
        assert_eq!(s.resolved_rotation_grace(), Some("60 seconds"));
        assert_eq!(s.resolved_theft_action(), Some(TheftAction::RevokeUser));
    }

    #[test]
    fn auth_sessions_resolves_access_ttl_falls_back_to_legacy_when_rotation_off() {
        let mut s = legacy_sessions();
        s.access_ttl = None;
        // Rotation off, access_ttl not set → legacy ttl.
        assert_eq!(s.resolved_access_ttl(), "7 days");

        s.access_ttl = Some("3 hours".to_string());
        // Rotation off, access_ttl explicit → explicit value wins.
        assert_eq!(s.resolved_access_ttl(), "3 hours");
    }

    #[test]
    fn auth_sessions_round_trips_with_nested_rotation_block() {
        round_trip(&rotation_sessions(RotationConfig {
            refresh_ttl: Some("30 days".to_string()),
            grace: Some("30 seconds".to_string()),
            theft_detection_action: Some(TheftAction::RevokeSessionFamily),
            span_ref: None,
        }));
    }

    // -------------------------------------------------------------------
    // ir-route-guards §3 — ViewGuard + RouteGuardDefaults + view.guard
    // slots on the 4 view kinds. See docs/proposals/ir-route-guards.md.
    // -------------------------------------------------------------------

    #[test]
    fn view_guard_round_trips_with_all_slots() {
        round_trip(&ViewGuard {
            policy: vec!["@policy.host_only".to_string()],
            on_unauthenticated: Some("/sign-in".to_string()),
            on_unauthorized: Some("/explore".to_string()),
            requires_lifecycle: None,
            on_lifecycle_pending: None,
            forbid_when: Vec::new(),
            span_ref: Some(SpanRef { start: 1, end: 50 }),
        });
    }

    #[test]
    fn view_guard_round_trips_with_only_policy() {
        round_trip(&ViewGuard {
            policy: vec!["@policy.authenticated".to_string()],
            on_unauthenticated: None,
            on_unauthorized: None,
            requires_lifecycle: None,
            on_lifecycle_pending: None,
            forbid_when: Vec::new(),
            span_ref: None,
        });
    }

    #[test]
    fn view_guard_round_trips_with_lifecycle_slots() {
        round_trip(&ViewGuard {
            policy: vec!["@policy.host_only".to_string()],
            on_unauthenticated: None,
            on_unauthorized: None,
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Host".to_string(),
                state: "complete".to_string(),
                substep: None,
                span_ref: Some(SpanRef { start: 10, end: 47 }),
            }),
            on_lifecycle_pending: Some("host_onboarding".to_string()),
            forbid_when: Vec::new(),
            span_ref: None,
        });
    }

    #[test]
    fn view_guard_omits_none_redirects_in_serialized_form() {
        let g = ViewGuard {
            policy: vec!["@policy.public".to_string()],
            on_unauthenticated: None,
            on_unauthorized: None,
            requires_lifecycle: None,
            on_lifecycle_pending: None,
            forbid_when: Vec::new(),
            span_ref: None,
        };
        let v = serde_json::to_value(&g).unwrap();
        assert_eq!(v, json!({ "policy": ["@policy.public"] }));
    }

    #[test]
    fn lifecycle_route_gate_ir_round_trips_with_all_slots() {
        let lifecycle_span = SpanRef { start: 10, end: 40 };
        let resume_span = SpanRef {
            start: 50,
            end: 140,
        };
        let requires = RequiresLifecycle {
            resource: "Host".to_string(),
            state: "complete".to_string(),
            substep: Some("phone_verification".to_string()),
            span_ref: Some(lifecycle_span),
        };
        let guard = ViewGuard {
            policy: vec!["@policy.host_only".to_string()],
            on_unauthenticated: Some("/sign-in".to_string()),
            on_unauthorized: Some("/explore".to_string()),
            requires_lifecycle: Some(requires.clone()),
            on_lifecycle_pending: Some("host_onboarding".to_string()),
            forbid_when: Vec::new(),
            span_ref: Some(SpanRef { start: 1, end: 60 }),
        };
        let resolved = ResolvedLifecycleGate {
            resource: "Host".to_string(),
            state: "complete".to_string(),
            substep: Some("phone_verification".to_string()),
            resume_router: "host_onboarding".to_string(),
            source_query_qualified: "host.query.my_host".to_string(),
        };
        let router = ResumeRouter {
            name: "host_onboarding".to_string(),
            source_query: "my_host".to_string(),
            arms: vec![
                ResumeArm {
                    kind: ResumeArmKind::None,
                    substep: None,
                    target_view: "host_onboarding_intermediation".to_string(),
                    span_ref: Some(resume_span),
                },
                ResumeArm {
                    kind: ResumeArmKind::State("complete".to_string()),
                    substep: Some("phone_verification".to_string()),
                    target_view: "host_home".to_string(),
                    span_ref: None,
                },
                ResumeArm {
                    kind: ResumeArmKind::Wildcard,
                    substep: None,
                    target_view: "host_onboarding_intermediation".to_string(),
                    span_ref: None,
                },
            ],
            span_ref: Some(resume_span),
        };

        let module = ExperienceModule {
            app: None,
            routes: vec![AppRoute {
                name: "host_index".to_string(),
                path: Some("/host".to_string()),
                routes: Vec::new(),
                route_params: Vec::new(),
                to: Some("host_home".to_string()),
                surface: Some("host web".to_string()),
                audience: Some("host".to_string()),
                lazy: None,
                prerender: None,
                guard: Some(guard.clone()),
                loaders: Vec::new(),
                pending_view: None,
                error_view: None,
                parent: None,
                span_ref: None,
            }],
            experiences: vec![Experience {
                name: "host".to_string(),
                imports: Vec::new(),
                views: vec![ExperienceView {
                    name: "host_home".to_string(),
                    anchor: Some("host_root".to_string()),
                    routes: vec!["host_index".to_string()],
                    extensible_by: Vec::new(),
                    source: Some("host.query.my_host".to_string()),
                    submit: None,
                    blocks: vec!["host_home_shell".to_string()],
                    actions: Vec::new(),
                    opens: Vec::new(),
                    tests: Vec::<ViewTestAssertion>::new(),
                    guard: Some(guard.clone()),
                    resolved_guard_policy: None,
                    resolved_lifecycle_gate: Some(resolved.clone()),
                    span_ref: None,
                }],
                extensions: Vec::new(),
                resume_routers: vec![router.clone()],
                span_ref: None,
            }],
            surfaces: Vec::new(),
        };

        let json = serde_json::to_string(&module).expect("serialize ExperienceModule");
        let back: ExperienceModule =
            serde_json::from_str(&json).expect("deserialize ExperienceModule");
        assert_eq!(module, back);
        assert!(json.contains("\"requires_lifecycle\""));
        assert!(json.contains("\"on_lifecycle_pending\""));
        assert!(json.contains("\"resolved_lifecycle_gate\""));

        round_trip(&router);
        assert_eq!(
            serde_json::to_value(ResumeArmKind::State("complete".to_string())).unwrap(),
            json!({ "kind": "state", "value": "complete" })
        );

        let feature_json = json!({
            "name": "host",
            "purpose": null,
            "defaults": {},
            "uses": [],
            "enums": [],
            "resources": [],
            "events": [],
            "rules": [],
            "policies": {
                "categories": [],
                "fields": []
            },
            "commands": [],
            "queries": [],
            "resume_routers": [router],
            "workflows": [],
            "jobs": [],
            "webhooks": [],
            "surfaces": [],
            "extensions": [],
            "escape_routes": []
        });
        let feature: Feature =
            serde_json::from_value(feature_json.clone()).expect("deserialize feature");
        assert_eq!(feature.resume_routers.len(), 1);
        assert_eq!(
            serde_json::to_value(feature)
                .expect("serialize feature")
                .get("resume_routers")
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn route_guard_defaults_round_trips_with_all_slots() {
        round_trip(&RouteGuardDefaults {
            default_policy: Some("@policy.public".to_string()),
            on_unauthenticated: Some("/sign-in".to_string()),
            on_unauthorized: Some("/".to_string()),
            skeleton: Some("@client.app_skeleton".to_string()),
            span_ref: None,
        });
    }

    #[test]
    fn route_guard_defaults_round_trips_empty_block() {
        round_trip(&RouteGuardDefaults {
            default_policy: None,
            on_unauthenticated: None,
            on_unauthorized: None,
            skeleton: None,
            span_ref: None,
        });
    }

    #[test]
    fn route_guard_defaults_omits_none_in_serialized_form() {
        let d = RouteGuardDefaults {
            default_policy: None,
            on_unauthenticated: None,
            on_unauthorized: None,
            skeleton: None,
            span_ref: None,
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn when_denied_route_round_trips_with_all_arms() {
        round_trip(&WhenDeniedRoute {
            unauthenticated: Some(RouteRedirectTarget::View("sign_in".to_string())),
            role_mismatch: vec![RoleMismatchArm {
                role: "traveler".to_string(),
                target: RouteRedirectTarget::View("explore".to_string()),
                span_ref: None,
            }],
            default: Some(RouteRedirectTarget::Path("/welcome".to_string())),
            span_ref: Some(SpanRef { start: 20, end: 80 }),
        });
    }

    #[test]
    fn experience_view_back_compat_without_guard() {
        // Pre-this-cell fixtures lack the `guard` field. Confirm they
        // still deserialize cleanly with `guard: None`.
        let legacy_json = json!({
            "name": "host_home",
            "anchor": "host_root"
        });
        let parsed: ExperienceView = serde_json::from_value(legacy_json).unwrap();
        assert_eq!(parsed.name, "host_home");
        assert!(parsed.guard.is_none());
    }

    #[test]
    fn audience_surface_back_compat_without_guard() {
        let legacy_json = json!({ "name": "host" });
        let parsed: AudienceSurface = serde_json::from_value(legacy_json).unwrap();
        assert_eq!(parsed.name, "host");
        assert!(parsed.guard.is_none());
    }

    #[test]
    fn platform_view_back_compat_without_guard() {
        let legacy_json = json!({
            "name": "list",
            "view_type": "list"
        });
        let parsed: PlatformView = serde_json::from_value(legacy_json).unwrap();
        assert_eq!(parsed.name, "list");
        assert!(parsed.guard.is_none());
    }

    #[test]
    fn app_route_back_compat_without_guard() {
        let legacy_json = json!({ "name": "host_index" });
        let parsed: AppRoute = serde_json::from_value(legacy_json).unwrap();
        assert_eq!(parsed.name, "host_index");
        assert!(parsed.guard.is_none());
    }
}

#[cfg(test)]
mod owner_axis_tests {
    //! `ir-resource-conventions-owner-scope` Cell O1 — round-trip the
    //! `Field.owner_axis` slot through serde so the wire shape stays
    //! stable across analyzer / codegen consumers.
    use super::*;

    #[test]
    fn owner_axis_round_trips_through_serde() {
        let axis = OwnerAxis {
            through_column: "user".to_owned(),
        };
        let json = serde_json::to_string(&axis).expect("serialize OwnerAxis");
        assert!(
            json.contains("\"through_column\":\"user\""),
            "serialized payload must carry the through_column verbatim: {json}",
        );
        let back: OwnerAxis = serde_json::from_str(&json).expect("deserialize OwnerAxis");
        assert_eq!(axis, back);
    }

    #[test]
    fn field_owner_axis_omitted_when_none() {
        // §7.2 — `skip_serializing_if = "Option::is_none"` keeps pre-O1
        // IR snapshots byte-for-byte stable. A field with no axis must
        // not surface the slot in the JSON shape.
        let field = Field {
            name: "name".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        };
        let json = serde_json::to_string(&field).expect("serialize Field");
        assert!(
            !json.contains("owner_axis"),
            "absent owner_axis must skip serialization: {json}",
        );
        let back: Field = serde_json::from_str(&json).expect("deserialize Field");
        assert_eq!(field, back);
    }
}
