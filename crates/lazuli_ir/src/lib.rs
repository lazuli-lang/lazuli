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

/// LZIR_SCHEMA — version of the IR JSON ABI. Bumped to 0.12.0 by D1
/// (SourceMap companion). Companion is opt-in sidecar emission, so
/// `Module` shape itself is unchanged; bump signals the companion
/// exists for downstream tooling.
pub const LZIR_SCHEMA: &str = "0.12.0";

pub type FileId = u16;

/// Span back-reference into the source AST. Debug-only; not part of the
/// published JSON ABI. Consumers must opt in via `--with-spans`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanRef {
    pub start: usize,
    pub end: usize,
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
    pub features: Vec<Feature>,
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<FeatureRequirement>,
    pub enums: Vec<EnumDecl>,
    pub resources: Vec<Resource>,
    pub events: Vec<Event>,
    pub rules: Vec<Rule>,
    pub policies: Policies,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum StorageValue {
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    /// Phase L Tier 4c — `<name>: <Type> derived from <expr>` lifts
    /// the computed-field expression. The analyzer keeps the verbatim
    /// expression text since `Expr` doesn't yet model comparison
    /// operators outside the predicate sublanguage; doctor reads the
    /// text for cross-field resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    SemanticMoney,
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
    CapSecret,
    /// Deprecated: the flat `CapFile` variant never carried arguments.
    /// Phase L Tier 2 introduces `TypeRef::Capability(CapabilityRef::File(...))`
    /// which carries the parsed `max_size`/`accept`/`visibility`/`signed_ttl`
    /// slots. Kept for back-compat with serialized payloads predating the
    /// typed shape.
    CapFile,
}

// =============================================================================
// Phase L Tier 2 — typed `@cap.File` capability
//
// `@cap.File(max_size:<size>,accept:<mime>,visibility:<mode>,signed_ttl:<dur>)`
// becomes a `TypeRef::Capability(CapabilityRef::File(FileCapability { ... }))`.
//
// Surface → IR mapping lives in `lazuli_analyzer::type_ref_from_syntax`.
// Doctor cross-checks against `object_storage` capability + input/output
// symmetry remain in the existing text-pattern doctor pipeline until the
// storage bucket cycle migrates them.
// =============================================================================

/// Discriminated union for typed capability decorators. New variants land
/// as the bucket cycles type each `@cap.*` family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CapabilityRef {
    File(FileCapability),
    /// Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:<X>)`.
    /// Closed catalog: `argon2id` canonical, `bcrypt` for legacy
    /// migration only.
    Hashed(HashedCapability),
    /// Phase L Tier 4 follow-up — `@cap.Encrypted(key:@key.<scope>)`.
    Encrypted(EncryptedCapability),
    /// Phase L Tier 4 follow-up — `@cap.Token(ttl:<duration>,
    /// single_use:<bool>,store:<storage>)`. `store` is `hashed` in v0.
    Token(TokenCapability),
}

/// Phase L Tier 4 follow-up — typed `@cap.Hashed(algorithm:<X>)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashedCapability {
    pub algorithm: HashAlgorithm,
}

/// Phase L Tier 4 follow-up — closed catalog of `@cap.Hashed` algorithms.
/// `Argon2id` is canonical; `Bcrypt` is kept only for legacy migration
/// (doctor warns on new uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    Argon2id,
    Bcrypt,
}

/// Phase L Tier 4 follow-up — typed `@cap.Encrypted(key:@key.<scope>)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedCapability {
    /// `@key.<scope>` reference, stored verbatim with the `@key.`
    /// prefix preserved so cold-readers see the namespace.
    pub key: String,
}

/// Phase L Tier 4 follow-up — typed `@cap.Token(...)`. All three
/// dimensions (ttl/single_use/store) are mandatory in canonical v0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCapability {
    /// `ttl:<integer><s|m|h|d>` — duration literal preserved verbatim
    /// (adapter parses).
    pub ttl: String,
    /// `single_use:true|false`.
    pub single_use: bool,
    /// `store:hashed` — closed catalog `{Hashed}` in v0.
    pub store: TokenStore,
}

/// Phase L Tier 4 follow-up — closed catalog of token storage modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenStore {
    Hashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCapability {
    pub max_size: FileSize,
    /// At least one MIME entry. The `|`-separated source form is
    /// normalised into a flat vector; a `*/*` wildcard authoring is
    /// represented as `family: "*", subtype: "*"`.
    pub accept: Vec<MimeType>,
    /// `None` is the parse-time default (`private` on a resource
    /// field, **required** on an api output — doctor warns on
    /// omission). The bucket-storage cycle proposal carries the
    /// closed catalog `{public, private, signed}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<FileVisibility>,
    /// `Some` only when `visibility == Signed`. Mutually exclusive
    /// with `visibility == Private` (doctor enforces); language
    /// records what the author wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_ttl: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSize {
    pub bytes: u64,
    /// Source-text literal preserved for inspect round-trip.
    pub literal: FileSizeLiteral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", content = "amount")]
pub enum FileSizeLiteral {
    Kb(u32),
    Mb(u32),
    Gb(u32),
}

impl FileSizeLiteral {
    /// Convert the literal into a byte count. `kb` = 1024, `mb` = 1024*1024,
    /// `gb` = 1024*1024*1024 (binary prefixes, matching the LSP literal
    /// catalog `is_file_size_literal`).
    pub fn bytes(self) -> u64 {
        match self {
            Self::Kb(n) => n as u64 * 1024,
            Self::Mb(n) => n as u64 * 1024 * 1024,
            Self::Gb(n) => n as u64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MimeType {
    /// IANA top-level family (`text`, `image`, `application`, `audio`,
    /// `video`, `font`) or wildcard `*`.
    pub family: String,
    /// Subtype, e.g. `csv`, `vnd.ms-excel`, `*`.
    pub subtype: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileVisibility {
    Public,
    Private,
    Signed,
}

/// Qualified name for a feature-scoped or local symbol. `feature` is `None`
/// for local references; cross-feature references carry the feature id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// Phase L Tier 4b — `rate_limit "<N per period per scope>"` literal.
    /// Captured verbatim and parsed by adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
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
    /// OpenAPI bucket cycle — `deprecated [since "..." replacement <ref>
    /// sunset "..."]`. `None` for live commands; `Some` for those flagged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
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
    /// `replacement <feature>.command.<name>` — cross-feature.
    Qualified(QualifiedName),
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyRef {
    Local(String),
    Atom(String),
    External { feature: String, name: String },
    Unresolved(String),
    None,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    pub keys: Vec<KeyClause>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQuery {
    pub name: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
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

fn is_false(value: &bool) -> bool {
    !*value
}

// =============================================================================
// Phase 1b — events, rules, workflows, surfaces
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub kind: EventKind,
    pub payload: Vec<EventField>,
    /// Observability bucket cycle row 37 — optional severity hint
    /// authored on `event.trace <name>`. Closed catalog:
    /// `debug`, `info`, `warn`, `error`. None defaults to `info` at
    /// the adapter. Rejected on `EventKind::Domain` by doctor
    /// (`event_trace_level_on_domain_event_diagnostics`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// Standard domain event published into the feature reaction graph.
    Domain,
    /// `event.trace` — intentionally not part of the reaction graph; for logs,
    /// audit streams, and external observers.
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventField {
    pub name: String,
    pub type_ref: TypeRef,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
}

// -----------------------------------------------------------------------------
// Cut A.8 — built-in trace events
//
// Trace events emitted by the runtime, not by author source. The
// language registers the names and the canonical payload schema so
// subscribers can rely on a stable contract; doctor rejects authored
// `event.trace <reserved>` redeclarations and validates subscriber
// payload references against the canonical fields.
//
// `agent_run` is the foundational built-in: emitted per agent
// dispatch (or per `flow` step in Cut B). The runtime instruments;
// adapters export to OpenTelemetry / file / stdout.
//
// See `docs/proposals/ai-primitives-cut-a-8.md`.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltInTraceEvent {
    pub name: String,
    pub payload: Vec<EventField>,
    pub fires_per: TraceFiresPer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceFiresPer {
    /// One emission per `agent <name>` dispatch.
    AgentDispatch,
    /// One emission per `command <name>` dispatch (observability
    /// bucket cycle row 35). Bound by `command_run`.
    CommandDispatch,
    /// One emission per `flow <name>.step <name>` (Cut B; reserved).
    FlowStep,
    /// One emission per `job <name>` invocation (observability
    /// bucket cycle row 35). Bound by `job_run`.
    JobInvocation,
    /// One emission per `webhook <name>` delivery (observability
    /// bucket cycle row 35). Bound by `webhook_run`.
    WebhookDelivery,
}

/// The canonical list of built-in trace events. The language reserves
/// these names; authoring `event.trace <name>` for any entry here is
/// rejected. The list is `const`-shaped (returns a fresh `Vec` per
/// call) so consumers don't worry about static-lifetime gymnastics.
///
/// Observability bucket cycle row 35 extends the registry from one
/// entry (`agent_run`, Cut A.8) to four. Each new entry follows the
/// A.8 pattern mechanically: a flat payload, no nested objects beyond
/// `agent_run.tools[]`, and a stable `fires_per` discriminant. See
/// `docs/proposals/bucket-observability-cycle.md` §3.5.
pub fn built_in_trace_events() -> Vec<BuiltInTraceEvent> {
    vec![
        BuiltInTraceEvent {
            name: "agent_run".to_owned(),
            fires_per: TraceFiresPer::AgentDispatch,
            payload: agent_run_payload(),
        },
        BuiltInTraceEvent {
            name: "command_run".to_owned(),
            fires_per: TraceFiresPer::CommandDispatch,
            payload: command_run_payload(),
        },
        BuiltInTraceEvent {
            name: "job_run".to_owned(),
            fires_per: TraceFiresPer::JobInvocation,
            payload: job_run_payload(),
        },
        BuiltInTraceEvent {
            name: "webhook_run".to_owned(),
            fires_per: TraceFiresPer::WebhookDelivery,
            payload: webhook_run_payload(),
        },
    ]
}

fn agent_run_payload() -> Vec<EventField> {
    use BuiltinType::*;
    let required = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: false,
    };
    let optional = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: true,
    };

    vec![
        required("agent", Text),
        optional("flow", Text),
        optional("flow_step", Text),
        required("model", Text),
        required("finish_reason", Text),
        required("tokens_input", Integer),
        required("tokens_output", Integer),
        required("tokens_total", Integer),
        optional("cost_usd", Decimal),
        required("duration_ms", Integer),
        optional("prompt_version", Text),
        // `tools` is a structured list. We surface it as a
        // single field with a forward-resolved record type; the
        // record itself (ToolCall) is registered alongside (see
        // built_in_trace_event_records).
        EventField {
            name: "tools".to_owned(),
            type_ref: TypeRef::Many(Box::new(TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "ToolCall".to_owned(),
            }))),
            optional: true,
        },
        optional("safety_decision", Text),
        optional("tenant", Text),
        optional("request_id", Text),
        optional("trace_id", Text),
    ]
}

/// Observability bucket cycle row 35 — canonical payload for
/// `command_run`. Emitted once per command dispatch (the moment the
/// runtime invokes the command handler, regardless of HTTP/RPC/event
/// trigger). Flat shape mirrors `agent_run_payload` discipline.
fn command_run_payload() -> Vec<EventField> {
    use BuiltinType::*;
    let required = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: false,
    };
    let optional = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: true,
    };
    vec![
        required("command", Text),
        required("actor", Text),
        optional("tenant", Text),
        required("status", Text),
        optional("error_code", Text),
        required("duration_ms", Integer),
        optional("request_id", Text),
        optional("trace_id", Text),
    ]
}

/// Observability bucket cycle row 35 — canonical payload for
/// `job_run`. One emission per job invocation: scheduled, manual, or
/// event-triggered. `attempt` lets billing/retry observers reconstruct
/// the retry chain without a separate join.
fn job_run_payload() -> Vec<EventField> {
    use BuiltinType::*;
    let required = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: false,
    };
    let optional = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: true,
    };
    vec![
        required("job", Text),
        required("trigger", Text),
        optional("tenant", Text),
        required("status", Text),
        required("attempt", Integer),
        required("duration_ms", Integer),
        optional("idempotency_key", Text),
        optional("error_code", Text),
        optional("request_id", Text),
        optional("trace_id", Text),
    ]
}

/// Observability bucket cycle row 35 — canonical payload for
/// `webhook_run`. One emission per webhook delivery (inbound). The
/// `signature_valid` field surfaces HMAC verification status so
/// fraud-detection adapters don't reparse the body.
fn webhook_run_payload() -> Vec<EventField> {
    use BuiltinType::*;
    let required = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: false,
    };
    let optional = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: true,
    };
    vec![
        required("webhook", Text),
        optional("tenant", Text),
        required("status", Text),
        required("signature_valid", Boolean),
        required("duration_ms", Integer),
        optional("idempotency_key", Text),
        optional("error_code", Text),
        optional("request_id", Text),
        optional("trace_id", Text),
    ]
}

/// Canonical inner records used by built-in trace events. Today only
/// `ToolCall` exists (referenced by `agent_run.tools[]`). The records
/// are surfaced via inspect alongside the events themselves so
/// subscribers know the full schema without spelunking source.
pub fn built_in_trace_event_records() -> Vec<BuiltInTraceRecord> {
    use BuiltinType::*;
    let required = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: false,
    };
    let optional = |name: &str, ty: BuiltinType| EventField {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        optional: true,
    };
    vec![BuiltInTraceRecord {
        name: "ToolCall".to_owned(),
        fields: vec![
            required("name", Text),
            required("effect", Text),
            required("duration_ms", Integer),
            required("status", Text),
            optional("error_kind", Text),
        ],
    }]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltInTraceRecord {
    pub name: String,
    pub fields: Vec<EventField>,
}

/// Whether `name` is reserved by a built-in trace event. Doctor calls
/// this when validating author-side `event.trace <name>` and job-side
/// `trigger event.trace <name>` references.
pub fn is_reserved_trace_event_name(name: &str) -> bool {
    built_in_trace_events()
        .iter()
        .any(|event| event.name == name)
}

/// Lookup a built-in trace event by name. Returns `None` for authored
/// trace events (which live under each feature's `events` instead).
pub fn built_in_trace_event(name: &str) -> Option<BuiltInTraceEvent> {
    built_in_trace_events()
        .into_iter()
        .find(|event| event.name == name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Author's prose title: `rule "archived customers cannot be reassigned"`.
    pub title: String,
    pub denies: OperationRef,
    pub when: Predicate,
    pub message: String,
    /// i18n bucket cycle — `message @translation.<key>` form. When set,
    /// `message` is the empty string; the runtime resolves the typed
    /// key at render time using `ctx.locale`. Doctor cross-checks the
    /// reference against the surrounding feature's `Translation.keys`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRef {
    pub resource: QualifiedName,
    pub op_name: String,
    pub kind: OperationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    Command,
    Transition,
    /// Resolution deferred to the analyzer; default for legacy lowering.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub on: FieldRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_emits: Vec<String>,
    pub transitions: Vec<Transition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef {
    pub resource: QualifiedName,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub from: String,
    pub to: String,
    /// `requires @policy.<category>` raises the policy bar for this transition
    /// above the workflow default (e.g. `requires @policy.delete`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    /// Surface words joined by space: `surface web admin` -> `name = "web admin"`.
    pub name: String,
    pub views: Vec<View>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed view kinds. New kinds enter via minor bump (see `docs/ir-abi.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum View {
    Table(TableView),
    SidePanel(SidePanelView),
    Form(FormView),
    Custom(CustomView),
}

impl View {
    pub fn name(&self) -> &str {
        match self {
            View::Table(v) => &v.name,
            View::SidePanel(v) => &v.name,
            View::Form(v) => &v.name,
            View::Custom(v) => &v.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableView {
    pub name: String,
    pub source: SourceRef,
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensible_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidePanelView {
    pub name: String,
    pub source: SourceRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensible_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormView {
    pub name: String,
    pub submit: QualifiedName,
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomView {
    pub name: String,
    /// Authored type label: `SidePanel`, `KanbanBoard`, etc. Lazuli does not
    /// generate a renderer for these; they reach extension contracts.
    pub view_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub query: QualifiedName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<NamedArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellBinding {
    pub field: String,
    /// `@client.<name>` reference resolved against the feature's `extensions`.
    pub renderer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockBinding {
    pub renderer: String,
}

// =============================================================================
// Experience IR — `.lzx`
// =============================================================================

// Observability bucket cycle row 36 — `Eq` dropped to match the
// downstream `AppManifest` change (which now carries `Option<f64>`
// sample-rate fields). `PartialEq` is preserved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperienceModule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<AppRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub experiences: Vec<Experience>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<PlatformSurface>,
}

// Observability bucket cycle row 36 — `AppManifest` no longer
// derives `Eq` because the new `logging.sample_rate` /
// `tracing.sample_rate` fields are `Option<f64>`. `f64` is
// intentionally non-`Eq` due to NaN; `PartialEq` is sufficient for
// the snapshot / fixture-equality assertions that depend on this
// struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_failed_redirect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_found: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<AppPackUse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<AppArchitecture>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<AppService>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<AppCommunication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<AppUrl>,
    /// Cut A.11 — CORS allowlist per environment. The runtime
    /// materialises browser-side middleware from this declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cors: Option<AppCors>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<AppEnvVar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<AppIntegration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<AppCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime: Vec<AppRuntimeUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<AppDeploy>,
    /// Observability bucket cycle row 36 — `app.logging` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<AppLogging>,
    /// Observability bucket cycle row 36 — `app.tracing` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracing: Option<AppTracing>,
    /// i18n bucket cycle — typed `locale` block. Supersedes the bare
    /// scalar `default_locale` when both are present; the analyzer
    /// copies `locale.default` into `default_locale` for back-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<AppLocale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// i18n bucket cycle — `app.locale` block. Declares supported BCP-47
/// tags and the fallback graph the runtime walks when a translation
/// is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppLocale {
    /// BCP-47 tag, e.g. "pt-BR".
    pub default: String,
    /// BCP-47 tags the app is willing to negotiate against. Must
    /// include `default`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported: Vec<String>,
    /// Fallback edges: when `from` is requested but no translation
    /// resolves, the runtime walks the chain to `to` before defaulting
    /// to `default`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<LocaleFallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleFallback {
    pub from: String,
    pub to: String,
}

/// i18n bucket cycle — `locale_negotiate` decorator. Sits on
/// `AppRuntimeUnit` (global default) and `Api` (per-endpoint override).
/// Declares the request axis the runtime reads to populate
/// `ctx.locale` and the matching strategy. All slots optional; the
/// runtime defaults to `accept_language` + `best_match` when omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleNegotiate {
    /// `source <axis>` — closed catalog: `accept_language`,
    /// `query_param`, `cookie`, `user_profile`, `subdomain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `strategy <name>` — closed catalog: `best_match`, `prefix_match`,
    /// `exact_match`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// `fallback <tag>` — BCP-47 tag in `app.locale.supported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

/// i18n bucket cycle — `translation` block lifted onto `Feature`.
/// Declares a per-locale catalog path and typed translation keys with
/// optional CLDR plural arms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    /// Catalog path with `<locale>` placeholder, e.g.
    /// `./i18n/customer.<locale>.json`.
    pub catalog: String,
    pub keys: Vec<TranslationKey>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKey {
    pub name: String,
    /// One entry per BCP-47 tag; must cover `app.locale.supported`.
    pub variants: Vec<TranslationVariant>,
    /// CLDR plural arms (`zero`/`one`/`two`/`few`/`many`/`other`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plurals: Vec<TranslationPluralArm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariant {
    pub locale: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArm {
    /// `zero` | `one` | `two` | `few` | `many` | `other` per CLDR.
    pub arm: String,
    pub variants: Vec<TranslationVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppBinding {
    pub target_feature: String,
    pub target_slot: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackUse {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRegistry {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<AppEnvVar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<AppIntegration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<AppCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<AppPack>,
    /// Cut A — `@tool.<name>` adapter declarations. Each entry pins the
    /// tool's `effect: read | write` (required) and optional
    /// `pii_classes`. Doctor diagnostic
    /// `tool_registry_effect_required_diagnostics` rejects entries that
    /// omit `effect`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<RegistryToolEntry>,
    /// Webhooks expanded cycle — catalog of expected inbound envelope
    /// shapes (per provider, provider-neutral surface). Referenced by
    /// `webhook ... payload from webhook_events.<name>`. Treated as
    /// external-origin: Lazuli does not assume the source is
    /// trustworthy, only that the contract matches what the provider
    /// documents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub webhook_events: Vec<WebhookEvent>,
}

/// Webhooks expanded cycle — single entry in the
/// `registry.webhook_events` catalog. Mirrors the lightweight `record`
/// shape (typed field decls + capability annotations) but keeps the
/// type token verbatim because the envelope is external and Lazuli
/// does not own its evolution. Doctor uses the field list to
/// cross-check `tenant_from`, `idempotency by`, and `dlq emit` payload
/// references; the runtime decodes against the same shape via
/// generated Go types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<WebhookEventField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Webhooks expanded cycle — one declared field inside a
/// `webhook_events.<name>` envelope. The `type_text` is kept verbatim
/// because the envelope is provider-side; `capabilities` capture any
/// `@semantic.*` / `@pii.*` decorators authored on the line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEventField {
    pub name: String,
    /// `Text`, `ID`, `Timestamp`, `Money`, ... — captured verbatim.
    pub type_text: String,
    pub required: bool,
    /// `@semantic.Email`, `@pii.contact`, ... — kept as authored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryToolEntry {
    /// Dotted path under `@tool.`, e.g. `web_search`, `calendar.create_event`.
    pub name: String,
    pub effect: ToolEffect,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pii_classes: Vec<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPack {
    pub name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<AppPackProvide>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<FeatureRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackProvide {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfile {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<AppProfileUrl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<AppProfileIntegration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<AppProfileDeploy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfileUrl {
    pub target: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfileIntegration {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_provenance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppProfileDeploy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_lock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppArchitecture {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_ready: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforce_service_boundaries: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppService {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposes: Vec<AppServiceExposure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServiceExposure {
    pub kind: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppCommunication {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asynchronous: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub propagate: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppUrl {
    pub target: String,
    pub environment: String,
    pub url: String,
}

/// Cut A.11 — CORS declaration. Lives in `app.lzi` alongside `urls`;
/// the runtime materialises browser-side CORS middleware from this
/// shape. Doctor cross-checks origins against `environments` and
/// declared `urls`; LSP catches shape errors at typing time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCors {
    /// One entry per `allow_origins <env> "<origin>"...` line.
    /// Multiple entries per environment merge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_origins: Vec<AppCorsOriginRule>,
    /// `allow_credentials true | false`. Defaults to `false` (CORS
    /// spec safe default).
    #[serde(default)]
    pub allow_credentials: bool,
    /// Quoted duration string (e.g. `"1h"`, `"10 minutes"`). Adapter
    /// parses to seconds. `None` lets the adapter pick its default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCorsOriginRule {
    pub environment: String,
    pub origins: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEnvVar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub scope: String,
    pub name: String,
    pub type_name: String,
    pub requiredness: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegration {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_provenance: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<AppIntegrationCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_classification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegrationCredentials {
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<AppIntegrationCredentialBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIntegrationCredentialBinding {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCapability {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRuntimeUnit {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serves: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<String>,
    /// i18n bucket cycle — `locale_negotiate` decorator on the runtime
    /// unit. Declares the global default request-locale negotiation
    /// strategy. Per-api overrides live on `Api.locale_negotiate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_negotiate: Option<LocaleNegotiate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppDeploy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_lock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_migrations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<String>,
    /// Migrations bucket cycle Route C — `strategy <rolling|blue_green|canary>`.
    /// Closed catalog enforced by `DEPLOY-STRATEGY-001`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// Migrations bucket cycle Route C — `lock_timeout "<duration>"`.
    /// Adapter-parsed duration literal; the language keeps the literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_timeout: Option<String>,
    /// Migrations bucket cycle Route C — `pre_migration_hook "<path>"`.
    /// Optional shell hook the runtime invokes before applying migrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_migration_hook: Option<String>,
    /// Migrations bucket cycle Route C — `post_migration_hook "<path>"`.
    /// Optional shell hook the runtime invokes after applying migrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_migration_hook: Option<String>,
    /// Migrations bucket cycle Route C — `checkpoint <name> "<path>"`.
    /// Pins an IR JSON snapshot the runtime can diff against. `lazuli plan
    /// --check <name>` validates the snapshot's integrity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<DeployCheckpoint>,
}

/// Migrations bucket cycle Route C — declarative checkpoint pinning under
/// `app.deploy.checkpoint <name> "<path>"`. The path is captured verbatim;
/// `DEPLOY-CHECKPOINT-001` verifies the file resolves relative to
/// `app.lzi`, and `DEPLOY-CHECKPOINT-002` warns when the loaded snapshot's
/// `lazuli_version` lags the analyzer's expected version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployCheckpoint {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Observability bucket cycle row 36 — declarative logging contract.
/// Lives directly under `app <Name>` alongside `urls`/`runtime`/`deploy`.
/// The language fixes intent (level, format, redact strategy); the
/// runtime materialises the slog handler stack. Adapter selection
/// (slog/zap/zerolog) lives in `registry.capabilities`.
///
/// All slots are optional; `None` means "adapter default". Authors
/// only need to declare the values they intend to override.
///
/// Closed catalogs:
///   - level:  debug, info, warn, error
///   - format: json, text
///   - redact: pii, none
///
/// Doctor:
///   - `app_logging_level_invalid_diagnostics`
///   - `app_logging_format_invalid_diagnostics`
///   - `app_logging_redact_unknown_diagnostics`
///   - `app_logging_sample_rate_range_diagnostics`
///
/// See `docs/proposals/bucket-observability-cycle.md` §3.1.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppLogging {
    /// One of the level catalog tokens. `None` means adapter default
    /// (typically `info` for production, `debug` for local).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// One of `json` (production-friendly, machine-parseable) or
    /// `text` (dev-friendly, human-readable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// PII redaction policy. `pii` auto-strips fields tagged with any
    /// `@pii.*` namespace; `none` disables auto-redaction (adapter
    /// may still redact). `None` defers to adapter default (`pii`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redact: Option<String>,
    /// Optional sampling rate in `[0.0, 1.0]`. `None` means "log
    /// every record". The runtime turns this into a slog `LevelVar`
    /// or sampling handler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Observability bucket cycle row 36 — declarative tracing contract.
/// Sibling to `AppLogging`. Declares whether trace spans are
/// propagated and at what sampling rate. The exporter wiring lives
/// in `registry.capabilities` (`tracing: @adapter.tracing`); this
/// block only declares the intent.
///
/// All slots are optional; `None` means adapter default.
///
/// Doctor:
///   - `app_tracing_sample_rate_range_diagnostics`
///   - `app_tracing_exporter_unbound_diagnostics`
///
/// See `docs/proposals/bucket-observability-cycle.md` §3.2.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppTracing {
    /// Whether the runtime propagates trace context across the
    /// request graph. `None` is treated as `true` by the runtime
    /// (matches W3C default expectations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propagate: Option<bool>,
    /// Head sampling rate in `[0.0, 1.0]`. `1.0` captures every
    /// span; `0.0` disables capture (still propagates context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f64>,
    /// Optional adapter slot name. Resolves to a
    /// `registry.capabilities <slot>: tracing` entry. `None` lets
    /// the runtime pick the default (no-op or stdout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exporter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRoute {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lazy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Experience {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ExperienceView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ViewExtension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperienceView {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensible_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ExperienceAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opens: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperienceAction {
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewExtension {
    pub anchor: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<ViewExtensionSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewExtensionSlot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<ViewExtensionOrder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audiences: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewExtensionOrder {
    pub relation: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformSurface {
    pub experience: String,
    pub platform: Platform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses_experience: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audiences: Vec<AudienceSurface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Web,
    Mobile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceSurface {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<PlatformView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformView {
    pub name: String,
    pub view_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

// =============================================================================
// Phase 1c — feature defaults, resource enrichment, extensions, escape routes
// =============================================================================

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Constraint {
    Unique(UniqueConstraint),
    Index(IndexConstraint),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueConstraint {
    pub fields: Vec<String>,
    /// `unique email per org` -> `qualifier = Some("org")`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConstraint {
    pub fields: Vec<String>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSource {
    Convention,
    Authored,
}

impl PathRef {
    pub fn convention(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: PathSource::Convention,
        }
    }

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

// =============================================================================
// Phase 1d — async work: jobs and webhooks
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    /// Execution lane for queued workers. `None` runs the reactor inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    /// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor.
    /// Lowered from the canonical-indent slice; doctor cross-checks
    /// the path against the resource tenancy axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    /// Phase L Tier 3 — `fanout tenants <axis>` scheduled-job
    /// declaration. `None` for non-scheduled or single-tenant jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fanout: Option<FanoutSpec>,
    /// Phase L Tier 3 — `timeout "<duration>"`. Adapter-parsed string;
    /// language keeps the literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Phase L Tier 3 — `calls <slot>.<op>` external-call references
    /// surfaced from the job body. Doctor uses these for cross-feature
    /// integration coverage (`INT-CALL-*`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_calls: Vec<ExternalCallRef>,
    pub body: JobBody,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.customer_archived` — feature-qualified or local.
    Event { event: QualifiedName },
    /// `trigger schedule "0 2 * * *"` — cron expression.
    Schedule { cron: String },
}

/// Derived operational kind for inspect output. Authoring never sets this;
/// the analyzer resolves `Schedule` -> Scheduled, event without queue ->
/// Reactor, event with queue -> QueuedWorker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobOperationalKind {
    Scheduled,
    Reactor,
    QueuedWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    /// Path expression: `envelope.id`, `payload.batch_id`, `payload.external_id`.
    pub by: Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub count: u32,
    pub backoff: BackoffStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    Fixed,
    Exponential,
}

/// A job has exactly one body style. Handler-backed jobs may still declare
/// `emits`; declarative bodies bind a target and apply one write effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarative),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHandler {
    pub path: PathRef,
    /// `handler "./..." returns Customer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarative {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lets: Vec<LetBinding>,
    pub effect: CommandEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub name: String,
    /// Inbound HTTP path: `"/webhooks/stripe/invoice-paid"`.
    pub route: String,
    pub verify: PathRef,
    /// Phase L Tier 3 — structured `verify hmac <alg>` declaration.
    /// `None` for legacy text-pattern webhooks; `Some` when the
    /// canonical-indent parser lifted the structured form. Coexists
    /// with `verify: PathRef` because the legacy path uses a file
    /// reference for verifier bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_verify: Option<VerifySpec>,
    /// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    pub handler: PathRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// Webhooks expanded cycle — `payload from webhook_events.<name>`
    /// typed envelope reference resolved against
    /// `AppRegistry.webhook_events`. Carried as a structured ref so
    /// doctor and inspect consumers do not have to re-parse the
    /// dotted form. Atrito #2 of the canonical proposal: this is a
    /// typed `WebhookEventRef`, not an opaque string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_from: Option<WebhookEventRef>,
    /// Webhooks expanded cycle — `replay` child declaring an inbound
    /// replay contract. `None` defers to the runtime default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplaySpec>,
    /// Webhooks expanded cycle — `dlq <variant>` child declaring how
    /// the runtime routes deliveries after retry exhaustion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dlq: Option<DlqSpec>,
    /// Webhooks expanded cycle — Atrito #5 of the canonical proposal:
    /// optional retry policy on inbound webhooks, reusing the jobs-side
    /// `RetryPolicy` verbatim. Surface form: `retry <n> backoff
    /// <strategy>`. The shared shape keeps the parser, doctor, and
    /// codegen single-pathed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Webhooks expanded cycle — typed reference to a
/// `registry.webhook_events.<name>` envelope. The `webhook_events.`
/// prefix is implicit (registry path); the language keeps only the
/// final identifier on disk so renames are local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEventRef {
    /// Catalog entry name within `AppRegistry.webhook_events`.
    pub name: String,
}

/// Webhooks expanded cycle — declarative replay contract on an inbound
/// webhook. `Allow` requires `within "<duration>"`; `Deny` rejects any
/// re-delivery whose dedupe key was seen before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaySpec {
    pub mode: ReplayMode,
    /// `within "<duration>"` — verbatim duration literal. The runtime
    /// parses it; the language never normalises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
    /// `dedupe by <path>` — optional override for the dedupe key path.
    /// `None` reuses the webhook's `idempotency by ...` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_by: Option<Path>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// `replay allow within "<duration>"` — re-delivery accepted in
    /// the window; runtime returns 200 without re-running the handler.
    Allow,
    /// `replay deny` — re-delivery always rejected; runtime returns a
    /// 409 with `ErrWebhookReplayDenied`.
    Deny,
}

/// Webhooks expanded cycle — dead-letter routing after retry
/// exhaustion. Closed three-variant catalog; mutual exclusion is baked
/// into the discriminator so the parser fails on duplicate children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DlqSpec {
    /// `dlq emit <event>` — publish a tombstone event onto the bus.
    /// Doctor resolves the event name against the feature's declared
    /// events / `event.trace` set.
    Emit { event: String },
    /// `dlq handler "./path.go"` — adapter-side custom handler.
    Handler { path: PathRef },
    /// `dlq drop reason "..."` — explicit waiver. Mirrors
    /// `verify none reason "..."` for the silent-drop edge.
    Drop { reason: String },
}

/// Phase L Tier 3 — `tenant_from payload.<axis>_id` extractor used by
/// jobs, webhooks, and notifications. Captures the path verbatim;
/// doctor splits and validates against tenancy axes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantFromSpec {
    /// `payload.org_id`, `envelope.tenant_id`, etc.
    pub path: Path,
}

/// Phase L Tier 3 — `fanout tenants <axis>` scheduled-job fanout
/// directive. `scope` is closed (`Tenants` today); the `axis` carries
/// the partition key the doctor cross-checks against the feature's
/// tenancy axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanoutSpec {
    pub scope: FanoutScope,
    pub axis: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanoutScope {
    /// `fanout tenants <axis>` — one execution per tenant per fire.
    Tenants,
}

/// Phase L Tier 3 — structured webhook verification spec. Replaces the
/// legacy `verify: PathRef` for canonical-indent webhooks: the
/// algorithm is closed, the secret is an env binding, and the header is
/// a literal string. Bare `PathRef` `verify` stays in place for the
/// legacy text-pattern path until Tier 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifySpec {
    pub scheme: VerifyScheme,
    pub algorithm: String,
    pub secret_env: String,
    pub header: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyScheme {
    /// `verify hmac <alg>` — the canonical inbound verifier today.
    Hmac,
}

/// Phase L Tier 3 — `calls <slot>.<op>` reference surfaced from the
/// job body. The slot is a registry integration name and the op is the
/// adapter method; doctor pairs these against the feature's
/// `integrations` block. `args` carries the named-argument bindings
/// declared on the call site.
///
/// Phase L Tier 4 follow-up — `span_ref` carries the call site's AST
/// span so doctor anchors `INT-CALL-*` diagnostics on the `calls`
/// line directly instead of text-walking the job body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalCallRef {
    pub slot: String,
    pub op: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<NamedArg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Phase L Tier 3 — `notification <name>` declarative contract.
///
/// `channel`, `recipient`, `template`, and `trigger` are the
/// notification-specific axes; `tenant_from`, `idempotency`, `retry`,
/// `emits`, and `policy` reuse the same shapes as jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub name: String,
    /// `trigger event <feature>.<event>` or `trigger schedule "<cron>"`.
    pub trigger: JobTrigger,
    /// `channel email, in_app`. Closed catalog enforced by
    /// `NOTIF-CHANNEL-001`: `email`, `in_app`, `sms`, `push`, `slack`,
    /// `discord`, `webhook`.
    pub channels: Vec<String>,
    /// `recipient target.email` — a path captured verbatim. Lowering
    /// keeps the literal so the adapter resolves against the live
    /// payload.
    pub recipient: String,
    /// `template "./outreach/welcome.mjml"`.
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// Notifications expanded bucket cycle — optional `digest` block.
    /// Aggregates triggers into a single dispatch per window per
    /// `group_by` key. Distinct from `rate_limit` (scalar, per-call) —
    /// digest is per-recipient/per-group structured batching. Doctor:
    /// `NOTIF-DIGEST-001/002/003`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<NotificationDigest>,
    /// Notifications expanded bucket cycle — optional `throttle` block.
    /// Per-recipient / per-channel structured rate-limit with burst.
    /// Distinct from scalar `rate_limit "N per <window>"` used on
    /// `agent` / `auth password` / `command` / `expose http`; throttle
    /// keys on the notification's recipient/channel axes, not on the
    /// caller. Doctor: `NOTIF-THROTTLE-001/002/003`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle: Option<NotificationThrottle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Notifications expanded bucket cycle — `digest` sub-block on
/// `notification`. Aggregates N triggers within `every` into one
/// dispatch per `group_by` value, capped at `max_size`. The
/// `template_strategy` closed catalog (`merge` | `append`) describes
/// how the adapter combines the per-trigger payloads when rendering
/// the digest template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDigest {
    /// `every "15 minutes"` / `every "1 hour"` / `every "1 day"`.
    /// Captured verbatim; doctor `NOTIF-DIGEST-002` rejects shapes
    /// outside `<N> (seconds|minutes|hours|days)`.
    pub every: String,
    /// `group_by <payload-path>` — typically the recipient axis
    /// (`customer_id`, `target.email`). Optional: when absent, the
    /// digest groups globally per notification kind. Doctor
    /// `NOTIF-DIGEST-001` cross-checks the segment against the trigger
    /// event's payload schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// `max_size <N>` — hard cap on items per digest. Doctor
    /// `NOTIF-DIGEST-003` rejects `<= 0` or `> 10000`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u32>,
    /// `template_strategy merge|append` — closed catalog. None defaults
    /// to `merge` at the adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_strategy: Option<DigestStrategy>,
}

/// Notifications expanded bucket cycle — closed catalog for
/// `digest template_strategy`. `merge` collapses per-trigger payloads
/// into a single object (last-write-wins per key); `append` emits a
/// list the template iterates over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestStrategy {
    Merge,
    Append,
}

/// Notifications expanded bucket cycle — `throttle` sub-block on
/// `notification`. Distinct from scalar `rate_limit "N per <window>"`:
/// throttle keys on recipient and/or channel and supports an
/// `immediate burst` before the bucket starts rejecting. The shape is
/// per-recipient / per-channel / per-burst, not per-caller — that is
/// why it does not reuse the `rate_limit` keyword.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationThrottle {
    /// `max_per "1 hour"` / `max_per "1 day"`. Window over which the
    /// bucket refills. Doctor `NOTIF-THROTTLE-001` rejects shapes
    /// outside `<N> (seconds|minutes|hours|days)`.
    pub max_per: String,
    /// `per_recipient` — when set, the throttle bucket is keyed on the
    /// notification's `recipient <path>` value. Required when `burst`
    /// is set (doctor `NOTIF-THROTTLE-003`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_recipient: bool,
    /// `per_channel` — when set, each channel of a multi-channel
    /// notification gets its own bucket. Email and `in_app` are then
    /// throttled independently.
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_channel: bool,
    /// `burst <N>` — number of immediate dispatches the bucket allows
    /// before throttling starts. Useful for OTP/login flows where the
    /// first 1-3 sends must go through without delay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst: Option<u32>,
}

/// Phase L Tier 3 — `event_group <pattern> on <Resource>` declaration.
///
/// The pattern is a glob (`customer_*`) the doctor uses to bind
/// concrete events authored under the group. The lifted IR records the
/// pattern + the owning resource verbatim; the payload block is
/// captured as a raw string list (Tier 4 lifts to typed event-field
/// projection).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventGroup {
    /// `customer_*` — glob pattern matched against event names.
    pub pattern: String,
    /// `on Customer` — owning resource type. `None` for resource-free
    /// groups (none in the fixture today; the field stays optional for
    /// forward compatibility).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_resource: Option<String>,
    /// `payload` child lines captured verbatim. Tier 4 lifts into typed
    /// `EventField`/`Expr` shapes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_payload: Vec<String>,
    /// `audit ...` line captured verbatim. None when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_audit: Option<String>,
    /// Concrete events authored directly under this group, identified
    /// by name only. The actual event records remain attached to
    /// `Feature.events`; this slot records the inheritance link so
    /// doctor can run `EVENTGROUP-NESTING-001` and the pattern-prefix
    /// rule (`event_group_can_own_short_event_declarations`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

// =============================================================================
// Migrations bucket cycle (Route C) — tenant_migration kind
// =============================================================================

/// Migrations bucket cycle Route C — `tenant_migration <name>` declaration.
///
/// Mirrors `Job`'s spine (idempotency / retry / timeout / handler) but is
/// scoped to per-tenant schema migrations. The closed body shape is:
/// `target tenants <axis>`, `idempotency by <path>` (mandatory),
/// `retry <count> backoff <strategy>`, `timeout "<duration>"`, and
/// `handler "<path>"`. No `emits`, no `target query.*`, no `policy`:
/// schema migrations are by design free of business effects.
///
/// Doctor cross-checks:
///   - `TM-AXIS-001` — `target` axis matches a `defaults.tenancy` axis
///     in the same feature.
///   - `TM-IDEMP-001` — `idempotency by` is mandatory; absence is an
///     error because schema migrations are not safely re-runnable
///     without an idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigration {
    pub name: String,
    pub target: TenantMigrationTarget,
    /// Mandatory; absence triggers `TM-IDEMP-001`.
    pub idempotency: IdempotencyKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    pub handler: PathRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Migrations bucket cycle Route C — `target tenants <axis>` fanout
/// directive for tenant migrations. Captures the axis verbatim; doctor
/// cross-checks against `defaults.tenancy` declared in the same feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigrationTarget {
    pub axis: String,
}

// =============================================================================
// Phase 1e — auth block
// =============================================================================

/// Authentication is a family of related subcontracts. Modeling them in one
/// optional block keeps identity, password, sessions, MFA, and OAuth visible
/// together. A feature should not declare more than one identity domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub identity: AuthIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<AuthPassword>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions: Option<AuthSessions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa: Option<AuthMfa>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth: Vec<AuthOAuthProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `auth identity Customer.email` — one identity field on one resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIdentity {
    pub field: FieldRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPassword {
    /// `algorithm argon2id` — required by Phase L. The runtime adapter
    /// pins the KDF; the language records the author's choice verbatim
    /// so doctor can spot legacy/banned algorithms (`md5`, `sha1`, etc.).
    /// Existing JSON payloads that omit this field default to the empty
    /// string for backward compatibility; doctor warns once the slot is
    /// known to be empty.
    #[serde(default)]
    pub algorithm: String,
    /// `hash @fn.hash_customer_password` — extension fn reference.
    pub hash: String,
    pub verify: String,
    /// `rate_limit "5 per 10 minutes"` — declarative throttle string parsed by
    /// the auth adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessions {
    /// `resource CustomerSession`
    pub resource: QualifiedName,
    /// `ttl "7 days"` — duration string parsed by the adapter.
    pub ttl: String,
    /// `refresh false` — whether refresh tokens are issued.
    pub refresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMfa {
    /// MFA method id: `totp`, `sms`, `webauthn`. Adapter-specific beyond this.
    pub method: String,
    /// `enroll @fn.<name>` — enrollment extension fn reference. Required
    /// by Phase L; legacy payloads default to the empty string.
    #[serde(default)]
    pub enroll: String,
    /// `verify @validator.<name>` or `@fn.<name>` — verification reference.
    /// Required by Phase L; legacy payloads default to the empty string.
    #[serde(default)]
    pub verify: String,
    /// Optional adapter reference, e.g. `@adapter.totp_provider`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOAuthProvider {
    /// Provider id: `google`, `github`, `microsoft`, etc.
    pub provider: String,
    /// `@adapter.<provider>_oauth` reference.
    pub adapter: String,
}

// =============================================================================
// Cut A — AI primitives: agent + tools + evals + discriminated output.
//
// Lowered from `lazuli_syntax::Agent` (the canonical-indent slice). The
// IR shape mirrors proposal `docs/proposals/ai-primitives-v0.md` and plan
// `docs/proposals/ai-primitives-v0-implementation.md` §4.1.
//
// Doctor cross-feature checks (Phase 3) read `Agent.tools[].resolved_*`
// fields populated by the expand pass — lowering produces `None` /
// empty vectors and the workspace resolution happens under
// `lazuli inspect --expand=tools` or `--expand=security`.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    /// Feature this agent lives in (canonical lower-snake name).
    pub feature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
    pub output_kind: AgentOutputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<TypeRef>,
    /// Resolved discriminator target. `None` for `Text` / `Stream` outputs;
    /// `Some(Enum)` for `output discriminator <Enum>`; `Some(RecordField)`
    /// for `output <Record>` after lowering disambiguates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_discriminator: Option<DiscriminatorRef>,
    /// `@llm.<name>` reference. The closed-namespace catalog enforces the
    /// prefix; doctor checks the name resolves to a known LLM adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_path: Option<String>,
    /// `@validator.*` references. Cut A allows 0 or 1; Cut A.5 widens to
    /// many (the `Vec` shape is already correct, so A.5 lands by adding
    /// the coverage diagnostic without an IR shape change).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety: Vec<QualifiedName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evals: Vec<EvalCase>,
    /// Cut A.7 — `expose http` block. Auto-mounts the agent as an
    /// HTTP endpoint with the agent's policy / rate_limit / output
    /// applied at the gateway. Doctor cross-checks path conflicts +
    /// audience reachability; LSP catches local shape issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expose_http: Option<HttpExposure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpExposure {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_slots: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of HTTP methods. Mirrors the existing `api.method`
/// text-pattern catalog (`GET | POST | PUT | PATCH | DELETE`) but now
/// typed in IR. JSON form is uppercase ASCII for wire-stability with
/// HTTP standard conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Phase L Tier 4b — `api <name>` declaration lifted from the
/// canonical-indent slice. Sibling of `Command` but with HTTP transport
/// bound. Replaces the `collect_api_paths` text-pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Api {
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub policy: PolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
    /// `output <TypeRef>` — required for canonical APIs today. Captured
    /// as a `TypeRef` so `@cap.File(...)` outputs project the same way
    /// as command outputs.
    pub output: TypeRef,
    /// `handler "./api/..."` — required for legacy text-pattern APIs;
    /// canonical APIs may opt out in a future cut. Captured as a path.
    pub handler: PathRef,
    /// i18n bucket cycle — per-api `locale_negotiate` override. When
    /// `Some`, supersedes the runtime unit's default for this endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_negotiate: Option<LocaleNegotiate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputKind {
    /// `output <Type>` — bare type reference; the agent returns plain text
    /// (or, for a record with a `discriminator` field, a discriminated
    /// record — see `output_discriminator`).
    Text,
    /// `output stream <Type>` — streaming response.
    Stream,
    /// `output discriminator <Enum>` — single enum-variant response.
    DiscriminatedEnum,
    /// `output <Record>` where the record carries a `discriminator` field.
    DiscriminatedRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DiscriminatorRef {
    /// `output discriminator <Enum>` — payload is the enum.
    Enum(QualifiedName),
    /// `output <Record>` — payload is the record; one of its fields
    /// carries the `discriminator` marker. The analyzer resolves the
    /// field + its enum type at lowering.
    RecordField {
        record: QualifiedName,
        field: String,
        enum_type: QualifiedName,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBinding {
    pub reference: QualifiedToolRef,
    /// Populated by the expand pass when the workspace IR is loaded;
    /// `None` after pure lowering. Proposal §A1 / plan §4.3 mandate this
    /// derivation runs only under `--expand=tools` / `--expand=security`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_effect: Option<ToolEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_pii_classes: Vec<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// JSON shape: `{"target": "Local", "kind": "...", "name": "..."}` for
/// `Local`/`CrossFeature` variants (the inner `kind` is the tool kind);
/// `{"target": "Adapter", "dotted": [...]}` for adapter tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target")]
pub enum QualifiedToolRef {
    /// `query.by_id`, `command.create`, `api.export` — same-feature
    /// shorthand. The analyzer rewrites to `CrossFeature` at expand time.
    Local { kind: ToolKind, name: String },
    /// `customer.query.by_id` — explicit cross-feature reference.
    CrossFeature {
        feature: String,
        kind: ToolKind,
        name: String,
    },
    /// `@tool.web_search`, `@tool.calendar.create_event` — adapter tool.
    /// The dotted tail joins the segments under `@tool.`.
    Adapter { dotted: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// `query.list` — collection read.
    QueryList,
    /// `query.lookup` — single-record read.
    QueryLookup,
    /// `query.sql` — opaque SQL read.
    QuerySql,
    /// `command` — write.
    Command,
    /// `api` — custom HTTP endpoint; effect derived from `method`.
    Api,
    /// `query` — unspecified subkind; the analyzer narrows to
    /// `QueryList`/`QueryLookup`/`QuerySql` once the target is known.
    QueryUnspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub assertions: Vec<EvalAssertion>,
    /// Cut A.10 — optional `golden "./path.jsonl" min_score N`
    /// reference. The runtime adapter loads the file and scores the
    /// agent's output against it; the language stays out of the
    /// scoring algorithm itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden: Option<GoldenSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenSpec {
    /// File path captured verbatim. The runtime resolves it.
    pub path: String,
    /// Optional `min_score N` gate threshold (0.0..=1.0). `None`
    /// means the adapter's default (0.85 by convention) applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalAssertion {
    pub kind: EvalAssertionKind,
    pub predicate: EvalPredicate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalAssertionKind {
    Requires,
    Forbids,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EvalPredicate {
    /// The closed predicate sublanguage. Lowering parses simple `<path>
    /// <op> <literal>` forms; richer shapes hit `Unparsed` until a future
    /// cut extends the predicate parser.
    Closed(Predicate),
    /// `<ref> contains <token-literal>` / `<ref> contains <@semantic.Type>`.
    /// Semantic-type validators dispatch at `lazuli test --evals` only —
    /// `lazuli check` validates predicate shape, never dispatches.
    Contains { lhs: Path, rhs: EvalContainsRhs },
    /// `tools.calls includes|excludes <tool-ref>`.
    ToolsCalls {
        op: ToolsCallsOp,
        target: QualifiedToolRef,
    },
    /// Source text the lowering could not yet structure. Doctor surfaces
    /// these as warnings; later predicate-parser extensions promote them
    /// to `Closed`.
    Unparsed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EvalContainsRhs {
    /// `"active"` — substring literal.
    Literal(String),
    /// `@semantic.Email` — membership matched by the type's auto validator.
    SemanticType(QualifiedName),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolsCallsOp {
    Includes,
    Excludes,
}

// =============================================================================
// Phase 1f — inline tests + policy registry with field-level policies
// =============================================================================

/// Inline declarative assertions about IR shape. A `TestBlock` is the last
/// child of a command, workflow transition, rule, or extensible view. See
/// `docs/canonical-semantics.md` "Tests" for the verb catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestBlock {
    pub assertions: Vec<TestAssertion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of test verbs. The analyzer rejects assertions that do not
/// belong to the parent construct (e.g. `accepted by` on a command).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", content = "value")]
pub enum TestAssertion {
    /// Generated command policy matrix row: `permits @role.admin, @role.sales`.
    PolicyAllow { actors: Vec<String> },
    /// Generated command policy matrix row: `forbids @role.viewer`.
    PolicyDeny { actors: Vec<String> },
    /// Command/rule predicate: `allows when target.status = active` or
    /// `allows when self.status = active`, depending on the parent construct.
    AllowsWhen { predicate: Predicate },
    /// Command/rule predicate: `denies when target.deleted_at != nil` or
    /// `denies when self.deleted_at != nil`, depending on the parent construct.
    DeniesWhen { predicate: Predicate },
    /// Workflow transition state edge: `allows from active`.
    AllowsFrom { state: String },
    /// Workflow transition state edge: `denies from paused`.
    DeniesFrom { state: String },
    /// Workflow transition policy: `allows as @role.admin`.
    AllowsAs { actor: String },
    /// Workflow transition policy: `denies as @role.viewer`.
    DeniesAs { actor: String },
    /// Combined transition: `allows from active as @role.admin`.
    AllowsFromAs { state: String, actor: String },
    /// Combined transition: `denies from active as @role.sales`.
    DeniesFromAs { state: String, actor: String },
    /// Extensible view whitelist: `accepted by customer_tags`.
    AcceptedBy { feature: String },
    /// Extensible view whitelist: `rejected by billing`.
    RejectedBy { feature: String },
}

/// Feature-level `policies` block. Categories are named atom lists; field
/// policies are per-resource read/write rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policies {
    pub categories: Vec<PolicyCategory>,
    pub fields: Vec<FieldPolicies>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Named feature-local policy: `create: @role.admin, @role.sales`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyCategory {
    pub name: String,
    /// Atom names like `@role.admin`, `@scope.same_org`, `@actor.system`. The
    /// analyzer validates that each atom resolves through the registry.
    pub atoms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}

/// Per-resource field policies: `fields Customer\n  email\n    read: ...`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicies {
    pub resource: QualifiedName,
    pub fields: Vec<FieldPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicy {
    pub field: String,
    /// Atom list governing reads. `None` = inherit feature-level read policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<Vec<String>>,
    /// Atom list governing writes. `None` = inherit feature-level write policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}
