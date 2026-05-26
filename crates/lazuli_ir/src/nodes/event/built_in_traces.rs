//! Built-in trace events — Cut A.8 + Observability cycle row 35.
//!
//! The runtime emits four reserved trace events without author source:
//! `agent_run`, `command_run`, `job_run`, `webhook_run`. Their canonical
//! payloads live in this module so subscribers can rely on a stable
//! contract. Doctor rejects authored `event.trace <reserved>`
//! redeclarations via [`is_reserved_trace_event_name`].
//!
//! Each trace event is bound by [`TraceFiresPer`] — one emission per
//! agent dispatch / command dispatch / flow step / job invocation /
//! webhook delivery. The shape is flat (no nested objects beyond
//! `agent_run.tools[]`) so OpenTelemetry / log adapters don't need
//! per-event glue.

use serde::{Deserialize, Serialize};

use crate::{BuiltinType, QualifiedName, TypeRef};

use super::EventField;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
