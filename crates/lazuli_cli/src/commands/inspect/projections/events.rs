//! `--expand=events` projection plus the built-in trace event list.
//!
//! Walks the feature's `event <name>` decls and joins each with any
//! matching `event_group` prefix payloads to surface the effective
//! payload shape per event. Inherited fields carry the
//! `event_group:<pattern>` origin; explicit fields carry the
//! `event:<name>` origin.
//!
//! The companion `inspect_built_in_trace_events` projection lifts the
//! framework's hard-coded trace event catalogue from `lazuli_ir` so
//! consumers see the same shapes the runtime emits.

use super::super::{
    InspectBuiltInTraceEvent, InspectBuiltInTraceField, InspectEvent, InspectPayloadField,
};
use super::super::expand::{collect_event_decls, collect_event_groups};
use super::super::formatters::format_type_ref;

pub(in crate::commands::inspect) fn inspect_events(lines: &[String]) -> Vec<InspectEvent> {
    let event_groups = collect_event_groups(lines);
    collect_event_decls(lines)
        .into_iter()
        .map(|event| {
            let mut payload = Vec::new();
            for group in &event_groups {
                if event.name.starts_with(&group.prefix) {
                    for entry in &group.payload {
                        payload.push(inspect_inherited_payload_field(
                            entry,
                            format!("event_group:{}", group.pattern),
                        ));
                    }
                }
            }

            for field in &event.payload {
                if let Some(field) = inspect_explicit_payload_field(field, &event.name) {
                    payload.push(field);
                }
            }

            InspectEvent {
                name: event.name,
                payload,
            }
        })
        .collect()
}

pub(in crate::commands::inspect) fn inspect_built_in_trace_events() -> Vec<InspectBuiltInTraceEvent> {
    lazuli_ir::built_in_trace_events()
        .into_iter()
        .map(|event| InspectBuiltInTraceEvent {
            name: event.name,
            fires_per: built_in_trace_fires_per_word(event.fires_per).to_owned(),
            payload: event
                .payload
                .into_iter()
                .map(|f| InspectBuiltInTraceField {
                    name: f.name,
                    type_text: format_type_ref(&f.type_ref),
                    optional: f.optional,
                })
                .collect(),
        })
        .collect()
}

fn built_in_trace_fires_per_word(kind: lazuli_ir::TraceFiresPer) -> &'static str {
    match kind {
        lazuli_ir::TraceFiresPer::AgentDispatch => "agent_dispatch",
        lazuli_ir::TraceFiresPer::CommandDispatch => "command_dispatch",
        lazuli_ir::TraceFiresPer::FlowStep => "flow_step",
        lazuli_ir::TraceFiresPer::JobInvocation => "job_invocation",
        lazuli_ir::TraceFiresPer::WebhookDelivery => "webhook_delivery",
    }
}

fn inspect_inherited_payload_field(entry: &str, origin: String) -> InspectPayloadField {
    let Some((name, expression)) = entry.split_once('=') else {
        return InspectPayloadField {
            name: entry.to_owned(),
            ty: "Unknown".to_owned(),
            origin,
            expression: None,
            condition: None,
        };
    };
    let (expression, condition) = expression
        .split_once(" when ")
        .map(|(value, condition)| (value.trim(), Some(condition.trim().to_owned())))
        .unwrap_or((expression.trim(), None));

    InspectPayloadField {
        name: name.trim().to_owned(),
        ty: infer_payload_type(name.trim(), expression).to_owned(),
        origin,
        expression: Some(expression.to_owned()),
        condition,
    }
}

fn inspect_explicit_payload_field(line: &str, event_name: &str) -> Option<InspectPayloadField> {
    let (name, rest) = line.split_once(':')?;
    let ty = rest.split_whitespace().next()?;

    Some(InspectPayloadField {
        name: name.trim().to_owned(),
        ty: ty.to_owned(),
        origin: format!("event:{event_name}"),
        expression: None,
        condition: None,
    })
}

fn infer_payload_type(name: &str, expression: &str) -> &'static str {
    if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    }
}
