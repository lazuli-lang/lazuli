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
//! `agent_run`, `command_run`, `job_run`, `webhook_run`. Their canonical
//! payloads live in this module ([`built_in_trace_events`],
//! [`built_in_trace_event_records`]) so subscribers can rely on a stable
//! contract. Doctor rejects authored `event.trace <reserved>`
//! redeclarations via [`is_reserved_trace_event_name`].
//!
//! Each trace event is bound by [`TraceFiresPer`] — one emission per
//! agent dispatch / command dispatch / flow step / job invocation /
//! webhook delivery. The shape is flat (no nested objects beyond
//! `agent_run.tools[]`) so OpenTelemetry / log adapters don't need
//! per-event glue.
//!
//! ## Rule = declarative deny
//!
//! [`Rule`] is the "this command/transition cannot happen when X" form.
//! It declares an [`OperationRef`] target and a [`crate::Predicate`]
//! over the operation's input. The message is either a literal string
//! or a [`TranslationKeyRef`] via `message @translation.<key>`. Rules
//! are evaluated at the operation boundary; codegen emits them as
//! pre-execution gates.
//!
//! ## Emit predicates (B5 framework gap 2)
//!
//! [`EmitPredicate`] attaches a typed `when <expr>` to a webhook `emits`
//! entry. Three closed shapes — equality, set membership, opaque. The
//! opaque variant ([`EmitPredicateKind::Other`]) is the escape hatch:
//! codegen treats it as a runtime-evaluated Go expression so authors
//! can iterate before the typed lift catches up. Doctor still tracks
//! the path the predicate reads ([`EmitPredicate::payload_path`]) for
//! field-resolution diagnostics on the typed shapes.
//!
//! ## See also
//!
//! - `docs/proposals/event-outbox.md` — outbox dispatch design.
//! - `docs/proposals/ai-primitives-cut-a-8.md` — `agent_run`.
//! - `docs/proposals/bucket-observability-cycle.md` §3.5 — the four
//!   built-in trace events.

use serde::{Deserialize, Serialize};

use crate::{
    BuiltinType, Predicate, QualifiedName, SpanRef, TestBlock, TypeRef, is_false,
};

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

/// EVENT-OUTBOX §3.3 — closed catalog for the per-event outbox
/// guarantee. The default is `None` (legacy best-effort dispatch);
/// `Guaranteed` opts the event into the transactional outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxMode {
    None,
    Guaranteed,
}

impl Default for OutboxMode {
    fn default() -> Self {
        OutboxMode::None
    }
}

impl OutboxMode {
    pub fn is_none(&self) -> bool {
        matches!(self, OutboxMode::None)
    }
    pub fn is_guaranteed(&self) -> bool {
        matches!(self, OutboxMode::Guaranteed)
    }
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
// Cut A.8 / Observability cycle row 35 — built-in trace events
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
    /// EVENT-OUTBOX §3.3 — per-event outbox mode, parallel to `events`.
    /// `events_outbox[i]` is the mode authored on `events[i]` (or
    /// `OutboxMode::None` when the line did not author one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events_outbox: Vec<OutboxMode>,
    /// B5 framework gap 1 — per-event typed payload variants. One
    /// entry per `event <name>` authored under the group, carrying
    /// the kind (committed/trace) and the typed payload fields lifted
    /// from the per-event body. When `variants` is non-empty the
    /// codegen prefers the typed projection over the legacy
    /// `Feature.events` lookup. Back-compat: legacy fixtures that
    /// author only `event foo` lines lower as variants with empty
    /// `fields` and `kind = Committed`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<EventVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// B5 framework gap 1 — one variant under an `EventGroup`. Carries
/// the kind (`event <name>` vs `event.trace <name>`), the typed
/// payload fields, and a span back to the source line for diagnostics.
///
/// Reuses `EventField` (the same shape standalone `Feature.events`
/// use) so codegen and doctor can read variant fields with a single
/// path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventVariant {
    /// Short name as authored under the group (e.g. `confirmed` for
    /// `event confirmed`). The full prefixed name is computed by
    /// codegen using the group pattern (`charge_*` + `confirmed` ->
    /// `charge_confirmed`).
    pub name: String,
    /// Closed catalog: committed (`event`) or trace (`event.trace`).
    pub kind: EventVariantKind,
    /// EVENT-OUTBOX §3.3 — `outbox guaranteed` flag authored on the
    /// variant header. Mirrors the parallel `events_outbox` slot but
    /// reaches the codegen directly from the variant record so it
    /// does not need to index two vectors.
    #[serde(default, skip_serializing_if = "OutboxMode::is_none")]
    pub outbox: OutboxMode,
    /// Typed payload fields lifted from the variant body. Empty when
    /// the variant was authored without a field body (back-compat).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<EventField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// B5 framework gap 1 — closed catalog of event-variant kinds.
/// Mirrors `EventKind` (the catalog already used by `Feature.events`)
/// but is duplicated to keep the `EventGroup`-side surface
/// self-contained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVariantKind {
    /// `event <name>` — committed domain bus variant.
    Committed,
    /// `event.trace <name>` — observability-only trace variant.
    Trace,
}

impl EventVariantKind {
    pub fn is_trace(&self) -> bool {
        matches!(self, EventVariantKind::Trace)
    }
}

/// B5 framework gap 2 — typed predicate attached to a webhook `emits`
/// entry. Three closed shapes cover the surface today:
///
/// * `field = "literal"` — equality check (most common).
/// * `field in ("a", "b")` — set membership.
/// * raw — opaque expression preserved verbatim for shapes the
///   typed lifter has not been taught yet. Codegen passes raw
///   predicates through as runtime-evaluated Go expressions inside
///   the dispatch table, so authors can still iterate without the
///   typed lift catching up.
///
/// Lowering also captures the **path** the predicate reads (`field`)
/// so the doctor can fail fast when the path does not resolve against
/// the webhook payload contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmitPredicate {
    /// Original `when <expr>` text verbatim (after the `when ` token,
    /// trimmed). Codegen + doctor both consume the structured shape;
    /// this slot is preserved for diagnostics and round-tripping.
    pub raw: String,
    /// Typed predicate kind. `Other(raw)` keeps the surface
    /// permissive while the typed catalog grows.
    pub kind: EmitPredicateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// B5 framework gap 2 — closed catalog of typed emit predicate shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmitPredicateKind {
    /// `path = "literal"` — equality.
    Equals { path: String, literal: String },
    /// `path in ("a", "b", ...)` — set membership.
    In { path: String, literals: Vec<String> },
    /// Any other predicate shape; codegen treats this as an opaque
    /// runtime expression. Carried verbatim so the dispatch table can
    /// still emit a Go-level comment + a `/* TODO */` placeholder.
    Other { raw: String },
}

impl EmitPredicate {
    /// Returns the payload path the predicate reads, when the typed
    /// catalog recognises one. Used by the doctor diagnostic
    /// `webhook_emit_predicate_field_unresolved_001` to anchor at the
    /// authored path.
    pub fn payload_path(&self) -> Option<&str> {
        match &self.kind {
            EmitPredicateKind::Equals { path, .. } => Some(path.as_str()),
            EmitPredicateKind::In { path, .. } => Some(path.as_str()),
            EmitPredicateKind::Other { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outbox_mode_default_is_none() {
        assert_eq!(OutboxMode::default(), OutboxMode::None);
        assert!(OutboxMode::None.is_none());
        assert!(OutboxMode::Guaranteed.is_guaranteed());
    }

    #[test]
    fn outbox_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(OutboxMode::Guaranteed).unwrap(),
            json!("guaranteed")
        );
    }

    #[test]
    fn event_variant_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(EventVariantKind::Committed).unwrap(),
            json!("committed")
        );
        assert!(EventVariantKind::Trace.is_trace());
        assert!(!EventVariantKind::Committed.is_trace());
    }

    #[test]
    fn emit_predicate_kind_equals_round_trips() {
        let k = EmitPredicateKind::Equals {
            path: "type".to_owned(),
            literal: "active".to_owned(),
        };
        let v = serde_json::to_value(&k).unwrap();
        assert_eq!(v["kind"], json!("equals"));
        let back: EmitPredicateKind = serde_json::from_value(v).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn emit_predicate_payload_path_returns_path_for_typed_variants() {
        let ep = EmitPredicate {
            raw: "type = \"x\"".to_owned(),
            kind: EmitPredicateKind::Equals {
                path: "type".to_owned(),
                literal: "x".to_owned(),
            },
            span_ref: None,
        };
        assert_eq!(ep.payload_path(), Some("type"));

        let other = EmitPredicate {
            raw: "weird".to_owned(),
            kind: EmitPredicateKind::Other {
                raw: "weird".to_owned(),
            },
            span_ref: None,
        };
        assert!(other.payload_path().is_none());
    }

    #[test]
    fn built_in_trace_events_reserves_canonical_names() {
        assert!(is_reserved_trace_event_name("agent_run"));
        assert!(is_reserved_trace_event_name("command_run"));
        assert!(is_reserved_trace_event_name("job_run"));
        assert!(is_reserved_trace_event_name("webhook_run"));
        assert!(!is_reserved_trace_event_name("user_authored"));
    }

    #[test]
    fn built_in_trace_event_lookup_returns_match() {
        let cmd = built_in_trace_event("command_run").unwrap();
        assert_eq!(cmd.name, "command_run");
        assert_eq!(cmd.fires_per, TraceFiresPer::CommandDispatch);
    }

    #[test]
    fn event_group_round_trips_with_empty_optional_slots() {
        let g = EventGroup {
            pattern: "charge_*".to_owned(),
            on_resource: Some("Charge".to_owned()),
            raw_payload: vec![],
            raw_audit: None,
            events: vec!["confirmed".to_owned()],
            events_outbox: vec![OutboxMode::Guaranteed],
            variants: vec![],
            span_ref: None,
        };
        let v = serde_json::to_value(&g).unwrap();
        let back: EventGroup = serde_json::from_value(v).unwrap();
        assert_eq!(back, g);
    }
}
