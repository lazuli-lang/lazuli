//! Event payload field-reference diagnostics.
//!
//! Two producers, both gated on
//! [`super::facts::collect_canonical_feature_facts`] /
//! [`super::facts::collect_event_contracts`]:
//!
//! * [`event_payload_reference_diagnostics`] — inside an
//!   `event_group ... on <resource>` payload block, every RHS field
//!   reference must resolve against the named resource's declared
//!   fields. Catches typos and stale references after a resource
//!   field is renamed.
//! * [`event_consumer_payload_diagnostics`] — inside an
//!   event-triggered job, every `payload.<field>` reference must exist
//!   on the producer event contract (declared fields + inherited
//!   `event_group` payload fields).
//!
//! The two `*_diagnostic` builders below are the per-line emitters
//! the producers call when a reference fails the check.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::{feature_name, leading_spaces, simple_canonical_diagnostic};

use super::facts::{collect_canonical_feature_facts, collect_event_contracts};
use super::parsers::{
    events_resource_name, payload_assignment_rhs, payload_field_references,
    resource_field_reference,
};

#[derive(Debug)]
pub(crate) struct JobPayloadReference {
    field: String,
    line_index: usize,
    line: String,
}

#[derive(Debug)]
pub(crate) struct EventTriggeredJobFacts {
    pub(crate) feature: String,
    pub(crate) trigger: Option<String>,
    pub(crate) payload_references: Vec<JobPayloadReference>,
}

pub(crate) fn event_payload_reference_diagnostics(source: &str) -> Vec<Diagnostic> {
    let features = collect_canonical_feature_facts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_events_resource: Option<String> = None;
    let mut in_payload = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_events_resource = None;
                in_payload = false;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_events_resource = None;
                in_payload = false;
            }
            4 if current_top == Some("domain") => {
                current_events_resource = events_resource_name(trimmed).map(str::to_owned);
                in_payload = false;
            }
            6 if current_top == Some("domain") && current_events_resource.is_some() => {
                in_payload = trimmed == "payload";
            }
            8 if current_top == Some("domain") && in_payload => {
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource_name) = current_events_resource.as_deref() else {
                    continue;
                };
                let Some(rhs) = payload_assignment_rhs(trimmed) else {
                    continue;
                };
                let Some(field_name) = resource_field_reference(rhs) else {
                    continue;
                };
                let Some(resource) = features
                    .get(feature_name)
                    .and_then(|feature| feature.resources.get(resource_name))
                else {
                    continue;
                };

                if !resource.fields.contains(field_name) {
                    diagnostics.push(event_payload_reference_diagnostic(
                        line_index,
                        line,
                        resource_name,
                        field_name,
                    ));
                }
            }
            _ => {}
        }
    }

    diagnostics
}

pub(crate) fn event_consumer_payload_diagnostics(source: &str) -> Vec<Diagnostic> {
    let contracts = collect_event_contracts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_job: Option<EventTriggeredJobFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
            }
            current_job = current_feature
                .as_ref()
                .map(|feature| EventTriggeredJobFacts {
                    feature: feature.clone(),
                    trigger: None,
                    payload_references: Vec::new(),
                });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(job) = current_job.take() {
                diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
            }
            continue;
        }

        let Some(job) = current_job.as_mut() else {
            continue;
        };

        if let Some(event_ref) = trimmed.strip_prefix("trigger event ") {
            let event_ref = event_ref.split_whitespace().next().unwrap_or(event_ref);
            job.trigger = Some(event_ref.to_owned());
        }

        for field in payload_field_references(line) {
            job.payload_references.push(JobPayloadReference {
                field,
                line_index,
                line: line.to_owned(),
            });
        }
    }

    if let Some(job) = current_job {
        diagnostics.extend(event_triggered_job_payload_diagnostics(job, &contracts));
    }

    diagnostics
}

pub(crate) fn event_triggered_job_payload_diagnostics(
    job: EventTriggeredJobFacts,
    contracts: &HashMap<String, HashSet<String>>,
) -> Vec<Diagnostic> {
    let Some(trigger) = job.trigger else {
        return Vec::new();
    };
    let event_ref = if trigger.contains('.') {
        trigger
    } else {
        format!("{}.{}", job.feature, trigger)
    };
    let Some(contract) = contracts.get(&event_ref) else {
        return Vec::new();
    };

    job.payload_references
        .into_iter()
        .filter(|reference| !contract.contains(&reference.field))
        .map(|reference| {
            event_consumer_payload_diagnostic(
                reference.line_index,
                &reference.line,
                &event_ref,
                &reference.field,
            )
        })
        .collect()
}

pub(crate) fn event_consumer_payload_diagnostic(
    line_index: usize,
    line: &str,
    event_ref: &str,
    field: &str,
) -> Diagnostic {
    simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "event-consumer-payload",
        &format!(
            "`payload.{field}` is not declared by event `{event_ref}`. Consumers may only read fields from the producer event contract, including inherited `event_group` payload fields."
        ),
    )
}

pub(crate) fn event_payload_reference_diagnostic(
    line_index: usize,
    line: &str,
    resource_name: &str,
    field_name: &str,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: line_index as u32,
                character: leading_spaces(line) as u32,
            },
            end: Position {
                line: line_index as u32,
                character: line.len().max(leading_spaces(line) + 1) as u32,
            },
        },
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String("event-payload-field".to_owned())),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "event payload references `{field_name}`, but resource `{resource_name}` has no field named `{field_name}`. Shared event payload expressions resolve against the `event_group ... on {resource_name}` resource."
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}
