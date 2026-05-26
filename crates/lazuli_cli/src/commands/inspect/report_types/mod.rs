//! Inspect report data shapes (`Inspect*` projection types).
//!
//! Every typed sub-block in the inspect report — the storage axis,
//! the auth axis, the agent dispatch graph, the job/webhook/event-
//! group/aggregate/notification clusters, the per-feature security
//! envelope, the `tests` projection, etc. — has a corresponding
//! `Inspect<Name>` carrier here. The projectors in
//! `inspect/mod.rs` (and per-axis projector modules) construct
//! these shapes; serde's `Serialize` derive renders them into the
//! `--format=json` payload.
//!
//! Rules of the file:
//!
//! - Only `InspectReport` and `InspectFeature` are `pub(crate)` —
//!   they cross the inspect module boundary (the IR-side
//!   `inspect_json_value` wiring needs to name them). Every other
//!   shape is `pub(super)` so the parent `inspect/` module can
//!   construct it but no other crate-level caller can.
//! - All struct fields use `pub(super)` so the projectors can fill
//!   them via record literals. Field visibility is intentionally
//!   uniform; do not widen to `pub` unless a cross-tree consumer
//!   actually needs to read the field.
//! - Doc comments stay attached to the field they describe; the
//!   `Cut <X>` / `Phase L Tier <N>` / `Roadmap §<x.y>` anchors are
//!   the contract for what each axis covers.
//!
//! See `crates/lazuli_cli/src/commands/inspect/mod.rs` for the
//! projectors that build these shapes, and `tests.rs` for the
//! golden fixtures asserting their serialized output.

use serde::Serialize;
use std::collections::BTreeMap;

mod agent;
mod aggregate;
mod auth;
mod job;
mod notification;
mod storage;
mod webhook;

// Re-export at the `report_types` namespace so the existing glob in
// `commands/inspect/mod.rs` (`pub(in crate::commands::inspect) use
// report_types::*;`) keeps picking these up unchanged.
pub(in crate::commands::inspect) use agent::*;
pub(in crate::commands::inspect) use aggregate::*;
pub(in crate::commands::inspect) use auth::*;
pub(in crate::commands::inspect) use job::*;
pub(in crate::commands::inspect) use notification::*;
pub(in crate::commands::inspect) use storage::*;
pub(in crate::commands::inspect) use webhook::*;

#[derive(Debug, Serialize)]
pub(crate) struct InspectReport {
    pub(super) schema: &'static str,
    pub(super) source: String,
    pub(super) expand: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) workspace: Option<lazuli_ir::AppWorkspace>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) contracts: Vec<lazuli_ir::AppContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) app: Option<lazuli_ir::AppManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) registry: Option<lazuli_ir::AppRegistry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) webhook_events: Option<Vec<lazuli_ir::WebhookEventRegistry>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) profiles: Vec<lazuli_ir::AppProfile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) routes: Vec<lazuli_ir::AppRoute>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) experiences: Vec<lazuli_ir::Experience>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) surfaces: Vec<lazuli_ir::PlatformSurface>,
    /// Roadmap §1.2 — populated only when `--expand=http` is set. The
    /// unified HTTP hygiene projection covers the three app-level
    /// blocks (`cookie` / `proxy` / `limits`) with `origin` metadata.
    /// `None` when the flag is off or when no block is populated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) http: Option<serde_json::Value>,
    pub(crate) features: Vec<InspectFeature>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InspectFeature {
    pub(super) name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) requirements: Vec<InspectRequirement>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) external_calls: Vec<InspectExternalCall>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) agents: Vec<InspectAgent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) notifications: Vec<InspectNotification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) refs: Option<InspectRefs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<InspectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) locators: Option<Vec<InspectLocators>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) dependencies: Option<Vec<InspectDependency>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) security: Option<InspectSecurity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) defaults: Option<Vec<InspectDefault>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) events: Option<Vec<InspectEvent>>,
    /// Cut A.8 — built-in trace events surfaced alongside the authored
    /// `events` when `--expand=events` is set. Today only `agent_run`;
    /// the slot exists so a future cut adding `job_run`/`webhook_run`
    /// surfaces them without an additional flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) built_in_trace_events: Option<Vec<InspectBuiltInTraceEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) targets: Option<Vec<InspectTarget>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) policies: Option<Vec<InspectPolicy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tests: Option<Vec<InspectTests>>,
    /// Cut A — populated only when `--expand=tools` is set. The
    /// dispatch graph keyed by agent + tool reference; doctor-level
    /// resolution of cross-feature targets is referenced via
    /// `resolution`, while structural facts come from the file alone
    /// (preserves the single-pass-base guarantee).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<InspectAgentToolsEntry>>,
    /// Cut A.7 — populated only when `--expand=expose` is set. Unified
    /// HTTP route table for the feature: every `api` block plus every
    /// agent declaring `expose http`. Cross-feature collisions surface
    /// via doctor; this projection is the per-feature observable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) expose: Option<Vec<InspectExposeEntry>>,
    /// Phase L — populated only when `--expand=auth` is set. Lowered
    /// `auth` block from the canonical-indent slice. `None` when the
    /// feature declares no `auth`; cross-feature checks (e.g. unique
    /// identity per workspace) live in doctor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth: Option<InspectAuth>,
    /// Phase L Tier 2 — populated only when `--expand=storage` is set.
    /// Every typed `@cap.File(...)` site in the feature: resource fields
    /// and api outputs. Omitted entirely when no `@cap.File` is authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) storage: Option<InspectStorage>,
    /// Phase L Tier 3 — populated only when `--expand=jobs` is set.
    /// Every lifted `ir::Job` on the feature. Mirrors `InspectAgent`'s
    /// shape (one struct per job) so an LLM can read it cold without
    /// joining tables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) jobs: Option<Vec<InspectJob>>,
    /// Phase L Tier 3 — populated only when `--expand=webhooks` is
    /// set. Every lifted `ir::Webhook` on the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) webhooks: Option<Vec<InspectWebhook>>,
    /// Phase L Tier 3 — populated only when `--expand=event_groups`
    /// is set. Every lifted `ir::EventGroup` on the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) event_groups: Option<Vec<InspectEventGroup>>,
    /// Migrations bucket cycle Route C — populated only when
    /// `--expand=migrations` is set. Every lifted
    /// `ir::TenantMigration` on the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tenant_migrations: Option<Vec<lazuli_ir::TenantMigration>>,
    /// Cache bucket cycle (CL.C.3) — populated only when
    /// `--expand=caches` is set. Every lifted feature-level
    /// `cache <name>` profile (`ir::CacheProfile`) on the feature.
    /// Inline (per-query) cache slots are projected on each query's
    /// `cache` field regardless of this flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) caches: Option<Vec<lazuli_ir::CacheProfile>>,
    /// CL.C.4 — populated only when `--expand=aggregates` is set.
    /// Every lifted `ir::Aggregate` on the feature. Roadmap §1.7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) aggregates: Option<Vec<InspectAggregate>>,
    /// Phase L Tier 4b — populated only when `--expand=commands` is set.
    /// Every lifted `ir::Command` on the feature, serialized verbatim
    /// from IR so the projection stays in lockstep with the lowered
    /// shape. Cross-feature checks (audit emit_to, policy resolution,
    /// rate-limit shape) surface via doctor; this projection is the
    /// per-feature observable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) commands: Option<Vec<lazuli_ir::Command>>,
    /// Phase L Tier 4b — populated only when `--expand=apis` (or
    /// `--expand=api`) is set. Every lifted `ir::Api` on the feature.
    /// Cross-feature path collision lives in doctor
    /// (`agent_expose_path_conflict_cross_feature_diagnostics`); this
    /// projection is the per-feature observable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) apis: Option<Vec<lazuli_ir::Api>>,
    /// Phase L Tier 4c — populated only when `--expand=resources` is
    /// set. Every lifted `ir::Resource` on the feature, serialized
    /// verbatim (fields with typed capability + semantic + pii
    /// decorators, `retention`, `has_many`, `constraints`,
    /// `validate`, `previous_names`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) resources: Option<Vec<lazuli_ir::Resource>>,
    /// Phase L Tier 4d — populated only when `--expand=queries` is
    /// set. Every lifted `ir::Query` on the feature (`List`/`Lookup`/
    /// `Sql` variants, each with their full v0 child coverage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) queries: Option<Vec<lazuli_ir::Query>>,
    /// Phase L Tier 4d — populated only when `--expand=records` is
    /// set. Every lifted `ir::Record` on the feature (fields +
    /// optional discriminator_field marker).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) records: Option<Vec<lazuli_ir::Record>>,
    /// IR Error-Vocab (Cell PARSE-1) — populated only when
    /// `--expand=errors` is set. The lifted feature-level `errors`
    /// block (`ir::FeatureErrors`): exposure defaults, 4xx/5xx field
    /// allowlists, and per-code message overrides. `None` when the
    /// feature declares no `errors` block; `Some(default)` (with all
    /// vectors empty) when the block exists but has no overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) errors: Option<lazuli_ir::FeatureErrors>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectRequirement {
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) contract: String,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectExternalCall {
    pub(super) subject: String,
    pub(super) slot: String,
    pub(super) operation: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) args: Vec<InspectCallArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) retry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) idempotency: Option<String>,
    pub(super) audit: bool,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectCallArg {
    pub(super) name: String,
    pub(super) value: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectRefs {
    pub(super) declared: Vec<InspectRefGroup>,
    pub(super) used: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) missing: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) unused: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectRefGroup {
    pub(super) group: String,
    pub(super) namespaces: Vec<String>,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectSummary {
    pub(super) provides: InspectProvides,
    pub(super) resources: Vec<String>,
    pub(super) records: Vec<String>,
    pub(super) queries: Vec<String>,
    pub(super) commands: Vec<String>,
    pub(super) workflows: Vec<InspectWorkflowSummary>,
    pub(super) jobs: Vec<String>,
    pub(super) webhooks: Vec<String>,
    pub(super) events: Vec<String>,
    pub(super) surfaces: Vec<String>,
    pub(super) anchors: Vec<String>,
    pub(super) extends: Vec<String>,
    pub(super) extended_by: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectProvides {
    pub(super) types: Vec<String>,
    pub(super) queries: Vec<String>,
    pub(super) events: Vec<String>,
    pub(super) anchors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectWorkflowSummary {
    pub(super) name: String,
    pub(super) transitions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectLocators {
    pub(super) subject: String,
    pub(super) kind: String,
    pub(super) bindings: Vec<InspectBinding>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectBinding {
    pub(super) name: String,
    pub(super) origin: String,
    pub(super) meaning: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectDependency {
    pub(super) kind: String,
    pub(super) from: String,
    pub(super) to: String,
    pub(super) origin: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectSecurity {
    pub(super) fields: Vec<InspectSecurityField>,
    pub(super) event_payloads: Vec<InspectSecurityEventPayload>,
    pub(super) operations: Vec<InspectSecurityOperation>,
    pub(super) webhooks: Vec<InspectSecurityWebhook>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectSecurityField {
    pub(super) resource: String,
    pub(super) field: String,
    pub(super) markers: Vec<String>,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectSecurityEventPayload {
    pub(super) event: String,
    pub(super) field: String,
    pub(super) markers: Vec<String>,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectSecurityOperation {
    pub(super) subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) scope_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) rate_limits: Vec<String>,
    pub(super) scope_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) audit: Option<InspectAudit>,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectAudit {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) fields: Vec<String>,
    /// Observability bucket cycle row 37 — `audit ... emit_to <X>`
    /// destination. `None` means "runtime falls back to the reserved
    /// `audit_log` stream".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) emit_to: Option<String>,
    pub(super) origin: &'static str,
}

// -----------------------------------------------------------------------------
// Phase L Tier 3 — inspect projections for jobs / webhooks / event_groups.
//
// `--expand=jobs`, `--expand=webhooks`, and `--expand=event_groups` produce
// these per-feature arrays. The shape mirrors `InspectAgent` /
// `InspectNotification` so a consumer (LLM or human) can read the full
// `notification`/`job`/`webhook` triple cold without joining tables.
// Row 32 of `docs/next-checklist.md`.
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub(super) struct InspectSecurityWebhook {
    pub(super) webhook: String,
    pub(super) verify: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) secrets: Vec<String>,
    pub(super) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectDefault {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) origin: &'static str,
    pub(super) applies_to: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectEvent {
    pub(super) name: String,
    pub(super) payload: Vec<InspectPayloadField>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectPayloadField {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) condition: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectTarget {
    pub(super) command: String,
    pub(super) target: String,
    pub(super) origin: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectPolicy {
    pub(super) subject: String,
    pub(super) policy: String,
    pub(super) atoms: Vec<String>,
    pub(super) origin: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) requires: Vec<InspectPolicyRequirement>,
    /// IR Error-Vocab (Cell PARSE-1) — per-policy or per-command
    /// `when_denied @translation.<key>` override surfaced from the
    /// lifted IR. `None` when neither the `policies.<category>` nor
    /// `command.policy` declared an override. Resolution-chain steps 1
    /// and 2 (proposal §2.E).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) when_denied: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectPolicyRequirement {
    pub(super) policy: String,
    pub(super) atoms: Vec<String>,
    pub(super) origin: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectTests {
    pub(super) subject: String,
    pub(super) groups: BTreeMap<String, Vec<InspectTestAssertion>>,
}

#[derive(Debug, Serialize)]
pub(super) struct InspectTestAssertion {
    pub(super) assertion: String,
    pub(super) origin: String,
}
