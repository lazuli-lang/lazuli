//! Doctor fact-type definitions — the data structs threaded through
//! the package load + diagnostic dispatch pipeline.
//!
//! Each struct here is a pure data type: no logic, no IO, no lift. The
//! `facts/` sibling module owns the IR-to-fact projection (collectors);
//! this module owns the *shape* those collectors produce.
//!
//! Wave R7-3 extract: lifted out of `doctor/mod.rs` so the file-local
//! struct-cluster (Tier3FeatureFacts, AuthFacts, OperationalFacts, the
//! `DoctorApp*` family, and the command-routing facts) lives in one
//! place. Visibility is preserved verbatim — `pub(super)` items are
//! still `pub(super)` to this module's parent (`doctor`), and
//! `pub(crate)` items remain crate-visible.
//!
//! Anchors:
//! - Tier 3 feature bundle: `Tier3FeatureFacts`
//! - Migrations rename captures: `ResourcePreviousFact`,
//!   `FieldPreviousFact`
//! - App-contract families: `DoctorAppWorkspace`, `DoctorAppContract`,
//!   `DoctorAppManifest`, `DoctorAppRegistry`, `DoctorAppProfile`,
//!   `DoctorFile`
//! - Auth facts: `AgentFacts`, `AuthFacts`, `FeatureSymbols`,
//!   `SymbolFact`, `ResourceFact`, `ResourceFieldFact`,
//!   `CommandSymbolFact`, `RegistryToolDefect`
//! - Command-routing facts: `CommandKey`, `CommandPolicy`,
//!   `CommandRouteSlot`, `ResolvedCommandTarget`
//! - Operational facts: `ExperienceFacts`, `SourceFact`,
//!   `OperationalFacts`, `IntegrationRequirementFact`,
//!   `ExternalCallFact`, `FileCapabilityFact`, `FileCapabilityBinding`

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lazuli_ir::{self as ir, Agent, AppContract, AppManifest, AppProfile, AppRegistry, AppWorkspace};
use lazuli_syntax::LzxDocument;

use crate::app_manifest::RegistryToolDefectReason;

use super::DoctorDiagnostic;

/// Phase L Tier 3 — lifted job/webhook/notification/event_group bundle
/// for one feature, with source line anchors for diagnostic placement.
///
/// Field-level visibility (Wave 4.3 R2): every field is `pub(crate)` so
/// the per-category aggregators under `doctor/aggregators/*` can read
/// the lifted Tier 3 IR without re-deriving it from source. Construction
/// stays inside `DoctorPackage::load` — these structs are read-only
/// once the package is loaded.
#[derive(Debug, Clone)]
pub(crate) struct Tier3FeatureFacts {
    pub(crate) feature: String,
    pub(crate) path: PathBuf,
    pub(crate) feature_line: usize,
    /// Resolved tenancy axis (`org`, `team`, custom, none) inferred
    /// from the feature's `defaults` block. `None` if the source did
    /// not declare a default. Doctor's tenant_from / fanout checks
    /// use this to cross-check axis references.
    pub(crate) tenancy_axis: Option<String>,
    /// Feature-level default policy from `defaults.policy`. Queries
    /// with no per-query policy inherit this; absent defaults imply the
    /// runtime's public fallback.
    pub(crate) defaults_policy: Option<lazuli_ir::PolicyRef>,
    /// Feature-level `defaults.timestamps`. Most correctness dispatchers
    /// only need commands/resources, but audit timestamp checks must know
    /// whether `updated_at` is framework-managed.
    pub(crate) defaults_timestamps: bool,
    pub(crate) jobs: Vec<lazuli_ir::Job>,
    pub(crate) webhooks: Vec<lazuli_ir::Webhook>,
    pub(crate) notifications: Vec<lazuli_ir::Notification>,
    pub(crate) event_groups: Vec<lazuli_ir::EventGroup>,
    /// Migrations bucket cycle Route C — lifted `TenantMigration`
    /// declarations for this feature, paired with `tenant_migration_lines`
    /// for `TM-*` diagnostic line anchoring.
    pub(crate) tenant_migrations: Vec<lazuli_ir::TenantMigration>,
    /// Migrations bucket cycle Route C — `Resource.previous_names`
    /// captures plus current resource names per feature, for
    /// `PREVIOUSLY-*` cross-checks.
    pub(crate) resource_previous_names: Vec<ResourcePreviousFact>,
    /// Migrations bucket cycle Route C — `Field.previous_names`
    /// captures (resource + field + previous names + line).
    pub(crate) field_previous_names: Vec<FieldPreviousFact>,
    /// Migrations bucket cycle Route C — every current resource name
    /// in this feature (including resources without any `previously`
    /// declaration) so `PREVIOUSLY-FWD-001` can detect stale rename
    /// targets pointing at live symbols.
    pub(crate) all_resource_names_in_feature: BTreeSet<String>,
    /// Migrations bucket cycle Route C — `resource_name -> {field_names}`
    /// per feature for `PREVIOUSLY-FWD-001` on field-level rename hints.
    pub(crate) all_field_names_in_feature: BTreeMap<String, BTreeSet<String>>,
    /// `job_name -> source line` lookup.
    pub(crate) job_lines: BTreeMap<String, usize>,
    pub(crate) webhook_lines: BTreeMap<String, usize>,
    pub(crate) notification_lines: BTreeMap<String, usize>,
    pub(crate) tenant_migration_lines: BTreeMap<String, usize>,
    /// `event_group_pattern -> source line` lookup.
    pub(crate) event_group_lines: BTreeMap<String, usize>,
    /// OpenAPI/Cache bucket cycles — lifted `command` IR per feature.
    /// Doctor reads `Command.deprecated` and `Command.invalidates` from
    /// here for the openapi/cache cross-checks.
    pub(crate) commands: Vec<lazuli_ir::Command>,
    /// `command_name -> source line` lookup. Anchors `deprecated_*` and
    /// `cache_invalidates_*` diagnostics at the command header.
    pub(crate) command_lines: BTreeMap<String, usize>,
    /// Cache bucket cycle — lifted `query` IR per feature. Doctor reads
    /// `Query.cache` (when populated) for the cache cross-checks.
    pub(crate) queries: Vec<lazuli_ir::Query>,
    /// `query_name -> source line` lookup. Anchors `cache_*` diagnostics
    /// at the query header.
    pub(crate) query_lines: BTreeMap<String, usize>,
    /// Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
    /// profile declarations lifted from the canonical-indent slice.
    /// Doctor uses this to (1) resolve query `cache <profile>`
    /// references for `cache-profile-unknown`, (2) build the package-
    /// wide tag index for `cache-tag-unknown`, and (3) cross-check TTL
    /// shape invariants for `cache-ttl-contract`.
    pub(crate) caches: Vec<lazuli_ir::CacheProfile>,
    /// `cache_profile_name -> source line` lookup. Anchors CL.C.3
    /// diagnostics at the profile header.
    pub(crate) cache_lines: BTreeMap<String, usize>,
    /// OpenAPI bucket cycle — every `api <name>` declaration in this
    /// feature (text-pattern era, before Tier 4 lift). Doctor uses this
    /// to surface `openapi_text_pattern_api_block`.
    pub(crate) api_names_text_pattern: Vec<String>,
    /// i18n bucket cycle — lifted typed `api` blocks (post Tier 4).
    /// Doctor reads `Api.locale_negotiate` from here for per-endpoint
    /// override validation.
    pub(crate) apis: Vec<lazuli_ir::Api>,
    /// Phase L Tier 4b — `api_name -> source line` lookup for the lifted
    /// `apis` slot. Anchors `agent_expose_*` cross-checks at each api
    /// header.
    pub(crate) api_lines: BTreeMap<String, usize>,
    /// Cut A.7 — lifted agents for report auto-mount route conflict checks.
    pub(crate) agents: Vec<lazuli_ir::Agent>,
    /// i18n bucket cycle — lifted `translation` block (when authored).
    pub(crate) translation: Option<lazuli_ir::Translation>,
    pub(crate) translation_line: usize,
    /// Phase L Tier 4 follow-up — lifted `record <Name>` declarations
    /// per feature. Replaces the text-scanned `FeatureSymbols.records`
    /// for the agent discriminator cross-checks.
    pub(crate) records: Vec<lazuli_ir::Record>,
    /// Phase L Tier 4 follow-up — lifted `enum <Name>` declarations per
    /// feature. Closes out the canonical-indent slice for `domain`:
    /// `agent_discriminator_target_invalid` and
    /// `check_record_discriminator` both read from here. The retired
    /// `FeatureSymbols.enums` text walker is gone.
    pub(crate) enums: Vec<lazuli_ir::EnumDecl>,
    /// Notifications expanded bucket cycle — lifted `event` /
    /// `event.trace` declarations for this feature. `NOTIF-DIGEST-001`
    /// resolves `notification.digest.group_by` against the trigger
    /// event's payload schema; cross-feature lookup walks `facts`
    /// keyed by `<feature>.<event>`. Tracking the full payload at the
    /// fact level keeps the diagnostic shape-aware without adding a
    /// new fact family.
    pub(crate) events: Vec<lazuli_ir::Event>,
    /// Whether the feature authored a top-level `policies` block.
    /// `Feature.policies` has a default value, so doctor reads the
    /// lowered `span_ref` to distinguish "absent" from "declared".
    pub(crate) policies_declared: bool,
    /// Phase L Tier 4 follow-up — full lifted `policies` block. Used
    /// by `SCOPE-OWNER-COLUMN-001` and other lints that need to walk
    /// a command's policy atom list (resolving `PolicyRef::Local`
    /// through `categories`) without re-deriving the lookup from
    /// text. Empty `Policies::default()` when the feature did not
    /// author a block.
    pub(crate) policies: lazuli_ir::Policies,
    /// Wave 10 — feature-level `extensions` block lifted. Used by
    /// `resource_validates_path_unknown` to resolve
    /// `validates field <f> @validator.<name>` references against
    /// declared `extensions.validator`. Empty when the feature has
    /// no extensions block.
    pub(crate) extensions: Vec<lazuli_ir::Extension>,
    /// Report vocab — lifted `report` declarations per feature. See
    /// `docs/proposals/report-vocab.md`.
    pub(crate) reports: Vec<lazuli_ir::Report>,
    /// `report_name -> source line` lookup. Anchors `REPORT-*`
    /// diagnostics at the report header.
    pub(crate) report_lines: BTreeMap<String, usize>,
    /// Resources captured (full `Resource`) per feature — used by
    /// `REPORT-COLUMN-MISMATCH-001` to resolve `row.<field>` against
    /// the source query's projection.
    pub(crate) resources: Vec<lazuli_ir::Resource>,
    /// Raw `ReportDecl` AST per feature — used by rules that need the
    /// original (pre-lowering) form (e.g. `REPORT-FORMAT-UNKNOWN-001`
    /// scans the AST formats list since lowering drops unknown tokens).
    pub(crate) report_decls: Vec<lazuli_syntax::ReportDecl>,
    /// CL.C.4 — lifted `aggregate <Name>` declarations per feature.
    /// Powers the four domain-model diagnostics:
    /// `AGGREGATE-ROOT-UNKNOWN`, `AGGREGATE-CONTAINS-UNKNOWN`,
    /// `INVARIANT-PREDICATE-INVALID`, `SLUG-UNIQUENESS-IMPLICIT`.
    /// Empty vec when the feature authored no aggregate blocks.
    pub(crate) aggregates: Vec<lazuli_ir::Aggregate>,
    /// CL.C.4 — `aggregate_name -> source line` lookup. Anchors the
    /// `AGGREGATE-*` and aggregate-scoped `INVARIANT-*` diagnostics
    /// at the aggregate header.
    pub(crate) aggregate_lines: BTreeMap<String, usize>,
    /// IR Error-Vocab (Cell ANALYZE-1) — lifted `errors` block. `None`
    /// when the feature did not author one. Used by the 7 `ERR-VOCAB-*`
    /// checks for `default hide/expose`, `expose client 4xx/5xx`, and
    /// per-code `<code> message @translation.<key>` rows.
    pub(crate) errors: Option<lazuli_ir::FeatureErrors>,
    /// IR Error-Vocab (Cell ANALYZE-1) — cloned `Feature.uses` so the
    /// `ERR-VOCAB-002` cross-feature key resolver can walk imported
    /// features without re-deriving the import set from `feature_uses`.
    pub(crate) uses: Vec<String>,
    /// Realtime bucket cycle — lifted `channel <name>` declarations per
    /// feature. Used by `CHANNEL-PAYLOAD-001` to resolve each channel's
    /// `payload <Type>` against the feature's `records` / `resources`.
    /// Empty when the feature declares no realtime channels.
    pub(crate) channels: Vec<lazuli_ir::Channel>,
}

/// Migrations bucket cycle Route C — `Resource` rename fact captured
/// from `previously migrated <old>` at the resource header.
#[derive(Debug, Clone)]
pub(crate) struct ResourcePreviousFact {
    /// Current resource name.
    pub(crate) current_name: String,
    /// Previously-known name(s).
    pub(crate) previous_names: Vec<String>,
    pub(crate) line: usize,
}

/// Migrations bucket cycle Route C — `Field` rename fact captured
/// from `previously migrated <old>` on a resource field.
#[derive(Debug, Clone)]
pub(crate) struct FieldPreviousFact {
    pub(crate) resource_name: String,
    pub(crate) current_name: String,
    pub(crate) previous_names: Vec<String>,
    pub(crate) line: usize,
}

#[derive(Debug)]
pub(crate) struct DoctorAppWorkspace {
    pub(crate) path: PathBuf,
    pub(crate) manifest: AppWorkspace,
}

#[derive(Debug)]
pub(crate) struct DoctorAppContract {
    pub(crate) path: PathBuf,
    pub(crate) manifest: AppContract,
}

#[derive(Debug)]
pub(crate) struct DoctorAppManifest {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) manifest: AppManifest,
}

#[derive(Debug)]
pub(crate) struct DoctorAppRegistry {
    pub(crate) path: PathBuf,
    pub(crate) manifest: AppRegistry,
}

#[derive(Debug)]
pub(crate) struct DoctorAppProfile {
    pub(crate) path: PathBuf,
    pub(crate) profile: AppProfile,
}

#[derive(Debug)]
pub(crate) struct DoctorFile {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) local_diagnostics: Vec<DoctorDiagnostic>,
    pub(crate) lzx: Option<LzxDocument>,
}

// Cut A — typed agent + symbol facts gathered for cross-feature checks.

#[derive(Debug, Clone)]
pub(crate) struct AgentFacts {
    pub(crate) feature: String,
    pub(crate) agent: Agent,
    pub(crate) path: PathBuf,
    /// 1-based source line where the `agent <name>` header lives.
    pub(crate) line: usize,
}

/// Phase L — typed `auth` block facts harvested per feature for auth
/// contract diagnostics:
///   - `auth_password_algorithm_hash_mismatch`
///   - `auth_password_no_session`
///   - `auth_sessions_resource_unknown`
///   - `auth_identity_field_unknown`
///   - `auth_oauth_adapter_unbound`
///   - `auth_oauth_no_password_alt`
///   - `auth_session_ttl_too_short`
///
/// The IR carries the lowered shape; the auxiliary `*_line` fields map
/// each subblock back to the source so diagnostics point at the exact
/// authored token rather than the `auth` header.
#[derive(Debug, Clone)]
pub(crate) struct AuthFacts {
    pub(crate) feature: String,
    pub(crate) auth: ir::Auth,
    pub(crate) path: PathBuf,
    /// 1-based line for the `auth` header.
    pub(crate) line: usize,
    pub(crate) identity_line: usize,
    pub(crate) password_line: Option<usize>,
    pub(crate) password_algorithm_line: Option<usize>,
    pub(crate) sessions_line: Option<usize>,
    pub(crate) sessions_resource_line: Option<usize>,
    pub(crate) mfa_line: Option<usize>,
    /// Per-provider `oauth <provider>` header line.
    pub(crate) oauth_lines: BTreeMap<String, usize>,
}

/// Phase L Tier 4 follow-up — both `records` and `enums` slots retired
/// (lifted into `Tier3FeatureFacts.records` / `Tier3FeatureFacts.enums`
/// from the typed IR). The struct now carries only the command policy
/// hint that `agent_tool_diagnostics` still text-walks while the legacy
/// pipeline owns surface commands.
#[derive(Debug, Clone, Default)]
pub(crate) struct FeatureSymbols {
    /// Maps short command name (e.g. `archive`) to its registered policy
    /// + safety hint. Commands are inherently write-effect for Cut A.
    pub(crate) commands: BTreeMap<String, CommandSymbolFact>,
}

#[derive(Debug, Clone)]
pub(crate) struct SymbolFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
}

/// Phase L Tier 4 follow-up — typed shape of a `resource <Name>`
/// declaration for the `auth_*` cross-checks. Now populated from the
/// IR `Feature.resources` lift instead of a text walker; the
/// `type_ref` slot carries `TypeRef::Capability(CapabilityRef::Hashed(...))`
/// directly so `cap_hashed_algorithm` is a typed match.
#[derive(Debug, Clone, Default)]
pub(crate) struct ResourceFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) fields: BTreeMap<String, ResourceFieldFact>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceFieldFact {
    /// Typed `TypeRef` lifted from `Field.type_ref`. `cap_hashed_algorithm`
    /// matches `TypeRef::Capability(CapabilityRef::Hashed(...))`;
    /// `is_identity_shaped` matches `Builtin::SemanticEmail/SemanticPhone`
    /// + `Builtin::Id` + the typed `unique` axis.
    pub(crate) type_ref: lazuli_ir::TypeRef,
    /// `Field.unique`. Used by `is_identity_shaped` for unique-shaped
    /// identity detection.
    pub(crate) unique: bool,
    /// 1-based line where the field is declared. Currently unused by
    /// diagnostics; reserved for future field-anchored messages.
    #[allow(dead_code)]
    pub(crate) line: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandSymbolFact {
    pub(crate) base: SymbolFact,
    pub(crate) policy: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RegistryToolDefect {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) name: String,
    pub(crate) reason: RegistryToolDefectReason,
}

impl Default for SymbolFact {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            line: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CommandKey {
    pub(crate) feature: String,
    pub(crate) command: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandPolicy {
    pub(crate) reference: String,
    pub(crate) atoms: Vec<String>,
    pub(crate) routes: BTreeMap<String, CommandRouteSlot>,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandRouteSlot {
    pub(crate) bound_from_context: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedCommandTarget {
    pub(crate) key: CommandKey,
    pub(crate) args: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExperienceFacts {
    pub(crate) view_actions: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) view_routes: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct SourceFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) name: String,
}

#[derive(Debug, Default)]
pub(crate) struct OperationalFacts {
    pub(crate) features: BTreeMap<String, SourceFact>,
    pub(crate) integration_requirements: Vec<IntegrationRequirementFact>,
    pub(crate) external_calls: Vec<ExternalCallFact>,
    pub(crate) env_references: Vec<SourceFact>,
    pub(crate) file_capabilities: Vec<SourceFact>,
    /// Row 30 — typed `@cap.File(...)` sites carrying the lowered
    /// `FileCapability` + origin + binding context (`ResourceField` or
    /// `ApiOutput`). Populated alongside `file_capabilities` so the
    /// storage diagnostics can run against typed IR shape, while the
    /// existing text-pattern fact powers the `APP-CAP-001` check.
    pub(crate) file_capability_facts: Vec<FileCapabilityFact>,
    pub(crate) jobs: Vec<SourceFact>,
    pub(crate) schedules: Vec<SourceFact>,
    pub(crate) webhooks: Vec<SourceFact>,
    pub(crate) apis: Vec<SourceFact>,
    pub(crate) web_surfaces: Vec<SourceFact>,
    pub(crate) mobile_surfaces: Vec<SourceFact>,
    pub(crate) web_routes: Vec<SourceFact>,
    pub(crate) mobile_routes: Vec<SourceFact>,
}

#[derive(Debug, Clone)]
pub(crate) struct IntegrationRequirementFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) feature: String,
    pub(crate) slot: String,
    pub(crate) contract: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalCallFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) feature: String,
    pub(crate) subject_kind: String,
    pub(crate) subject: String,
    pub(crate) slot: String,
    pub(crate) operation: String,
    pub(crate) has_timeout: bool,
    pub(crate) has_retry: bool,
    pub(crate) has_idempotency: bool,
}

/// Row 30 — one typed `@cap.File(...)` site harvested from a `.lzi`
/// source file. `feature` is the enclosing `feature <name>` block;
/// `binding` discriminates between a resource field and an api output
/// so the storage diagnostics can apply context-sensitive rules (e.g.
/// `visibility` is required on api outputs but defaults to `private`
/// on resource fields).
#[derive(Debug, Clone)]
pub(crate) struct FileCapabilityFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) feature: String,
    pub(crate) binding: FileCapabilityBinding,
    pub(crate) capability: lazuli_ir::FileCapability,
}

#[derive(Debug, Clone)]
pub(crate) enum FileCapabilityBinding {
    /// `<field>: @cap.File(...)` inside a `resource <Name>` block.
    ResourceField { resource: String, field: String },
    /// `output @cap.File(...)` inside an `api <Name>` block.
    ApiOutput { api: String },
}
