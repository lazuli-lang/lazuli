//! `emits ... from` + rule-self + required-field-nil diagnostics.
//!
//! Three loosely related shape rules that share the same authoring
//! intent: keep event payloads, rule subjects, and field requirements
//! consistent across the surface.
//!
//! * [`emits_derived_diagnostics`] — `emits <event> from <effect>` must
//!   cite `creates`/`updates`/`deletes` and reject an inline body that
//!   would duplicate the cited effect's bindings.
//! * [`rule_self_diagnostics`] — `deny ... when <head>.<field>` must
//!   use one of the canonical heads (`self`, `target`, `ctx`,
//!   `params`, `payload`, `envelope`, `route`, `input`); anything else
//!   is the legacy subject alias.
//! * [`required_field_nil_rule_diagnostics`] — a rule predicate cannot
//!   meaningfully check `self.<field> = nil` if the field is declared
//!   `required`; the branch is dead.
//!
//! [`predicate_references_nil_self_field`] and
//! [`legacy_rule_subject_alias`] are the two leaf-helpers consumed by
//! the producers above; they stay here because no other event
//! sub-module touches them.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{feature_name, leading_spaces, simple_canonical_diagnostic};

use super::facts::collect_required_resource_fields;

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
