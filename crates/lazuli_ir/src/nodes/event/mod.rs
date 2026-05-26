//! Event + Rule IR — `event <name>`, `event_group`, `rule`, built-in traces.
//!
//! Events are Lazuli's pub/sub spine. The `event` primitive declares a
//! typed payload that commands `emit`, jobs `trigger on`, and workflows
//! react to. Without a typed event vocabulary every product re-invents
//! the wheel (string topic names, freeform JSON, cross-feature glue
//! that drifts); Lazuli locks the contract so doctor can refuse
//! incompatible producers/subscribers cold.
//!
//! ## Two siblings: Event vs EventGroup
//!
//! - [`Event`] — concrete declaration with a typed payload. Lives on
//!   `Feature.events`.
//! - [`EventGroup`] — `event_group <pattern> on <Resource>` (Phase L
//!   Tier 3). Declares a *family* of names under a glob pattern with a
//!   shared payload spine; concrete events authored under the group
//!   inherit the spine and contribute their own payload shape via
//!   [`EventVariant`]. Doctor walks the pattern to bind concrete
//!   events. [`EventGroup::raw_payload`] is a verbatim string list
//!   until Tier 4 lifts to typed [`EventField`].
//!
//! ## Outbox vs best-effort
//!
//! [`OutboxMode::Guaranteed`] turns on the transactional outbox: the
//! producing command's pgx tx writes a `lazuli_outbox` row in the same
//! commit as the resource mutation; the runtime pump dispatches
//! post-commit. The default [`OutboxMode::None`] is the legacy
//! best-effort post-commit publish (lossy on crash). Authors can choose
//! per-event; codegen wires the right path.
//!
//! ## Built-in trace events (Cut A.8 + Observability cycle row 35)
//!
//! The runtime emits four reserved trace events without author source:
//! `agent_run`, `command_run`, `job_run`, `webhook_run`. See
//! [`built_in_traces`].
//!
//! ## Rule = declarative deny
//!
//! [`Rule`] is the "this command/transition cannot happen when X" form.
//! See [`rule`].
//!
//! ## Emit predicates (B5 framework gap 2)
//!
//! [`EmitPredicate`] attaches a typed `when <expr>` to a webhook `emits`
//! entry. See [`emit_predicate`].
//!
//! ## See also
//!
//! - `docs/proposals/event-outbox.md` — outbox dispatch design.
//! - `docs/proposals/ai-primitives-cut-a-8.md` — `agent_run`.
//! - `docs/proposals/bucket-observability-cycle.md` §3.5 — the four
//!   built-in trace events.

use serde::{Deserialize, Serialize};

use crate::{SpanRef, TypeRef, is_false};

pub mod built_in_traces;
pub mod emit_predicate;
pub mod event_group;
pub mod outbox;
pub mod rule;

pub use built_in_traces::{
    BuiltInTraceEvent, BuiltInTraceRecord, TraceFiresPer, built_in_trace_event,
    built_in_trace_event_records, built_in_trace_events, is_reserved_trace_event_name,
};
pub use emit_predicate::{EmitPredicate, EmitPredicateKind};
pub use event_group::{EventGroup, EventVariant, EventVariantKind};
pub use outbox::OutboxMode;
pub use rule::{OperationKind, OperationRef, Rule};

/// Concrete event declaration with a typed payload. One [`Event`] per
/// authored `event <name> { … }` block; lives on `Feature.events`.
/// The `kind` axis splits domain events (reaction graph) from trace
/// events (observers only); the `outbox` axis chooses transactional
/// vs best-effort dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub kind: EventKind,
    pub payload: Vec<EventField>,
    /// `payload none` — explicit opt-out sentinel for intentionally
    /// payload-less events (heartbeats, liveness signals). When `true`
    /// the event has no typed payload by design; doctor must NOT fire
    /// VOCAB-EVENT-PAYLOAD-001. Defaults to `false` (not authored).
    #[serde(default, skip_serializing_if = "is_false")]
    pub payload_none: bool,
    /// Observability bucket cycle row 37 — optional severity hint
    /// authored on `event.trace <name>`. Closed catalog:
    /// `debug`, `info`, `warn`, `error`. None defaults to `info` at
    /// the adapter. Rejected on `EventKind::Domain` by doctor
    /// (`event_trace_level_on_domain_event_diagnostics`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// EVENT-OUTBOX §3.3 — transactional-outbox guarantee. `Guaranteed`
    /// means the producing command's pgx tx writes a `lazuli_outbox` row
    /// in the same commit as the resource mutation; the runtime pump
    /// dispatches post-commit. `None` (default) preserves the legacy
    /// best-effort post-commit Publish path.
    #[serde(default, skip_serializing_if = "OutboxMode::is_none")]
    pub outbox: OutboxMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed axis splitting reaction-graph events from trace-only events.
/// The split changes codegen + doctor wiring: `Trace` events never
/// fan out to rules/jobs/workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// Standard domain event published into the feature reaction graph.
    Domain,
    /// `event.trace` — intentionally not part of the reaction graph; for logs,
    /// audit streams, and external observers.
    Trace,
}

/// One typed payload entry on an [`Event`]. The `optional` flag
/// controls whether the field is present on every emit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventField {
    pub name: String,
    pub type_ref: TypeRef,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
}
