//! Diagnostics for the `event` family — declaration shape, trace
//! triggers, rule self-bindings, and required-field nil-checks.
//!
//! This is band A of the event cluster: the producers that share the
//! "what does this header / clause mean" theme. Band B
//! (`event_payload_reference`, consumer/job payloads, tenant_from,
//! scheduled_job_tenancy, etc.) lives alongside but is split off in a
//! follow-up extraction once the helpers it needs
//! (`CanonicalFeatureFacts`, `collect_canonical_feature_facts`,
//! `collect_feature_tenant_axes`) are themselves moved out of lib.rs.
//!
//! | Producer | Concern |
//! |---|---|
//! | [`emits_derived_diagnostics`] | `emits <event> from <effect>` must cite `creates`/`updates`/`deletes` and reject inline bindings. |
//! | [`event_kind_diagnostics`] | Reject the abandoned `observability_only` legacy event kind. |
//! | [`event_trace_trigger_diagnostics`] | `job <name> trigger event.trace <kind>` only triggers on declared `event.trace` events. |
//! | [`event_locator_diagnostics`] | `event` declarations are forbidden inside feature-level `events` blocks (use `event <name>` siblings). |
//! | [`target_binding_diagnostics`] | `target` is only valid inside `creates`/`updates`/`deletes` effect blocks. |
//! | [`rule_self_diagnostics`] | Field-level rules cannot use bare `self` — write `self.<field>` or use a resource-level rule. |
//! | [`required_field_nil_rule_diagnostics`] | A `required` field cannot then be guarded with a `= nil` / `!= nil` rule. |
//!
//! Cluster-local helpers (`collect_trace_events`,
//! `collect_required_resource_fields`,
//! `predicate_references_nil_self_field`, `legacy_rule_subject_alias`)
//! stay here because every consumer is in this file.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    event_group_prefix, feature_name, field_name, leading_spaces, qualify_group_event_name,
    simple_canonical_diagnostic,
};

pub(crate) fn emits_derived_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Match `emits <event> from creates|updates|deletes`. The header is
        // the only place where this clause is canonical.
        let Some(rest) = trimmed.strip_prefix("emits ") else {
            continue;
        };
        let mut tokens = rest.split_whitespace();
        let Some(_event_token) = tokens.next() else {
            continue;
        };
        let Some(from_keyword) = tokens.next() else {
            continue;
        };
        if from_keyword != "from" {
            continue;
        }
        let Some(effect) = tokens.next() else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "emits-derived-contract",
                "`emits <event> from <effect>` requires the effect block name (`creates`, `updates`, or `deletes`).",
            ));
            continue;
        };
        if !matches!(effect, "creates" | "updates" | "deletes") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "emits-derived-contract",
                "`emits <event> from <effect>` requires `creates`, `updates`, or `deletes`. the runtime derives the payload by name match against that effect's bindings.",
            ));
            continue;
        }

        // The body, if present, must be empty. Inline bindings duplicate what
        // the cited effect already declares, defeating the point of `from`.
        let header_indent = leading_spaces(line);
        let child_indent = header_indent + 2;
        for next in lines.iter().skip(line_index + 1) {
            let next_trimmed = next.trim_start();
            if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                continue;
            }
            let next_indent = leading_spaces(next);
            if next_indent <= header_indent {
                break;
            }
            if next_indent == child_indent && next_trimmed.contains(" = ") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "emits-derived-contract",
                    "`emits <event> from <effect>` derives the payload from the cited effect's bindings; inline `<field> = <value>` children duplicate that mapping. Remove the body or drop `from <effect>`.",
                ));
                break;
            }
        }
    }

    diagnostics
}

pub(crate) fn event_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed == "observability_only" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-kind",
                "observability-only events should use `event.trace <name>` instead of the `observability_only` modifier.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn event_trace_trigger_diagnostics(source: &str) -> Vec<Diagnostic> {
    let trace_events = collect_trace_events(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        let Some(event_ref) = trimmed.strip_prefix("trigger event ") else {
            continue;
        };
        let event_ref = event_ref.split_whitespace().next().unwrap_or(event_ref);
        let is_trace = if event_ref.contains('.') {
            trace_events.contains(event_ref)
        } else {
            current_feature
                .as_deref()
                .map(|feature| trace_events.contains(&format!("{feature}.{event_ref}")))
                .unwrap_or(false)
        };

        if is_trace {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-trace-trigger",
                "`event.trace` declarations are outside the reaction graph and should not be used as job triggers; promote the event to `event` first.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_trace_events(source: &str) -> HashSet<String> {
    let mut events = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut current_group_prefix: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            current_group_prefix = None;
            continue;
        }

        if leading_spaces(line) == 4 {
            current_group_prefix = event_group_prefix(trimmed).map(str::to_owned);
        }

        if leading_spaces(line) == 4 && trimmed.starts_with("event.trace ") {
            if let (Some(feature), Some(event)) = (
                current_feature.as_deref(),
                trimmed.split_whitespace().nth(1),
            ) {
                events.insert(format!("{feature}.{event}"));
            }
        } else if leading_spaces(line) == 6 && trimmed.starts_with("event.trace ") {
            if let (Some(feature), Some(prefix), Some(event)) = (
                current_feature.as_deref(),
                current_group_prefix.as_deref(),
                trimmed.split_whitespace().nth(1),
            ) {
                events.insert(format!(
                    "{feature}.{}",
                    qualify_group_event_name(prefix, event)
                ));
            }
        }
    }

    events
}

pub(crate) fn event_locator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.starts_with('#') || trimmed.starts_with("event.trace ") {
            continue;
        }

        if line.contains("payload = event") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-locator-namespace",
                "do not assign the implicit event object wholesale. Use explicit `payload.<field>` or `envelope.<field>` values.",
            ));
            continue;
        }

        if line.contains("event.") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "event-locator-namespace",
                "event-triggered jobs should use `envelope.*` for bus metadata and `payload.*` for authored event fields, e.g. `envelope.id` or `payload.customer_id`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn target_binding_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            continue;
        }

        if matches!(current_top, Some("command" | "job"))
            && (line.contains("self.") || line.contains("(self)") || line.contains("= self"))
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "target-binding",
                "commands and declarative jobs should use `target` for the loaded target record; reserve `self` for rules and workflow transitions.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn rule_self_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if !trimmed.starts_with("deny ") {
            continue;
        }

        let Some((_, predicate)) = trimmed.split_once(" when ") else {
            continue;
        };

        if let Some(alias) = legacy_rule_subject_alias(predicate) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "rule-self",
                &format!(
                    "rules should use `self` for the target snapshot, not `{alias}`. Use `self.<field>` in rule predicates."
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn required_field_nil_rule_diagnostics(source: &str) -> Vec<Diagnostic> {
    let required_fields = collect_required_resource_fields(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if !trimmed.starts_with("deny ") {
            continue;
        }

        let Some(feature) = current_feature.as_deref() else {
            continue;
        };
        let Some((operation, predicate)) = trimmed
            .strip_prefix("deny ")
            .and_then(|rest| rest.split_once(" when "))
        else {
            continue;
        };
        let Some(resource) = operation
            .split_once('.')
            .map(|(resource, _)| resource.trim())
        else {
            continue;
        };

        for field in required_fields
            .iter()
            .filter_map(|(field_feature, field_resource, field)| {
                (field_feature == feature && field_resource == resource).then_some(field)
            })
        {
            if predicate_references_nil_self_field(predicate, field) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "required-field-nil-rule",
                    &format!(
                        "rule predicate checks `self.{field}` against `nil`, but `{resource}.{field}` is declared `required`; make the field optional or remove the impossible nil branch.",
                    ),
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn collect_required_resource_fields(source: &str) -> HashSet<(String, String, String)> {
    let mut fields = HashSet::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(feature_name(trimmed));
            current_top = None;
            current_resource = None;
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            current_resource = None;
            continue;
        }

        if current_top != Some("domain") {
            continue;
        }

        if leading_spaces(line) == 4 {
            current_resource = trimmed
                .strip_prefix("resource ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_owned);
            continue;
        }

        if leading_spaces(line) == 6
            && trimmed.contains(" required")
            && let (Some(feature), Some(resource), Some(field)) = (
                current_feature.as_deref(),
                current_resource.as_deref(),
                field_name(trimmed),
            )
        {
            fields.insert((feature.to_owned(), resource.to_owned(), field.to_owned()));
        }
    }

    fields
}

pub(crate) fn predicate_references_nil_self_field(predicate: &str, field: &str) -> bool {
    let left = format!("self.{field}");
    predicate.contains(&format!("{left} = nil")) || predicate.contains(&format!("{left} != nil"))
}

pub(crate) fn legacy_rule_subject_alias(predicate: &str) -> Option<&str> {
    let first = predicate.split_whitespace().next()?;
    let (head, _) = first.split_once('.')?;

    if matches!(
        head,
        "self" | "target" | "ctx" | "params" | "payload" | "envelope" | "route" | "input"
    ) {
        None
    } else if head
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
    {
        Some(head)
    } else {
        None
    }
}
