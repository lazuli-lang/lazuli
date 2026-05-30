//! Cut A.8 — built-in `agent_run` trace event diagnostics.
//!
//! `agent_run` is registered by the IR as a built-in trace event. The
//! language reserves the name (authored `event.trace agent_run` is
//! rejected) and validates subscriber jobs against the canonical
//! payload schema so a job referencing a non-existent field doesn't
//! fail silently at runtime.
//!
//! Diagnostic ids:
//!   - `event_trace_reserved_name_diagnostics` (error) — `event.trace
//!     <name>` where `<name>` is a built-in.
//!   - `agent_run_subscriber_payload_drift_diagnostics` (error) — a
//!     subscriber job references a field not in the canonical payload.
//!   - `trigger_trace_unknown_diagnostics` (error) — `trigger
//!     @trace.<name>` / `trigger event.trace <name>` references an
//!     event that's neither built-in nor authored in the same file.
//!
//! See `docs/proposals/ai-primitives-cut-a-8.md`.

use std::collections::BTreeSet;

use lazuli_ir as ir;

use crate::doctor::parsers::{format_name_list, is_lzi_path, payload_field_list};
use crate::doctor::scanners::leading_spaces;
use crate::doctor::{DoctorDiagnostic, DoctorFile, DoctorSeverity};

pub(crate) fn agent_run_trace_diagnostics(files: &[DoctorFile]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let canonical_payload: BTreeSet<String> = ir::built_in_trace_events()
        .into_iter()
        .find(|e| e.name == "agent_run")
        .map(|e| e.payload.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();

    // Observability bucket cycle row 35 — pre-compute the set of
    // built-in trace event names once per check so `trigger
    // @trace.<X>` and `trigger event.trace <X>` resolution both
    // consult the same registry. Authored `event.trace <name>`
    // declarations in scope are gathered per file below.
    let built_in_names: BTreeSet<String> = ir::built_in_trace_events()
        .into_iter()
        .map(|e| e.name)
        .collect();

    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();

        // Observability bucket cycle row 35 — collect authored
        // `event.trace <name>` declarations *in this file* so
        // `trigger_trace_unknown` doesn't false-positive on
        // legitimate subscriber references to authored events.
        let authored_trace_names: BTreeSet<String> = lines
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    return None;
                }
                trimmed
                    .strip_prefix("event.trace ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned)
            })
            .collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }

            // Reserved-name: `event.trace <name>` where <name> is a
            // built-in. Reject the authored declaration.
            if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                if ir::is_reserved_trace_event_name(name) {
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading_spaces(line) + 1,
                        severity: DoctorSeverity::Error,
                        code: "event_trace_reserved_name_diagnostics".to_owned(),
                        message: format!(
                            "`event.trace {name}` is reserved by the IR as a built-in trace event; the runtime emits it automatically. Authoring this declaration is rejected — remove the block and subscribe via `job ... trigger event.trace {name}` instead."
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // Payload-drift: `trigger event.trace agent_run` inside a
            // job, followed by `payload <field>` references at deeper
            // indent. Reject any field not in the canonical schema.
            if trimmed.starts_with("trigger event.trace ") {
                let name = trimmed
                    .strip_prefix("trigger event.trace ")
                    .map(|n| n.trim().to_owned())
                    .unwrap_or_default();
                if canonical_payload_event(&name, &canonical_payload) {
                    diagnostics.extend(scan_payload_field_drift(
                        file,
                        &lines,
                        i,
                        &name,
                        &canonical_payload,
                    ));
                }
            }

            // Observability bucket cycle row 35 — `trigger_trace_unknown`.
            // The `@trace.<name>` namespace and the bare-form
            // `trigger event.trace <name>` both have to resolve to a
            // built-in trace event or an authored `event.trace <name>`
            // in the same file. We catch the failure here so a typo
            // doesn't fall through to runtime as "subscriber wired to
            // an event that nobody emits."
            let trace_ref = trimmed
                .strip_prefix("trigger @trace.")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
                .or_else(|| {
                    trimmed
                        .strip_prefix("trigger event.trace ")
                        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
                });
            if let Some(name) = trace_ref
                && !name.is_empty()
                && !built_in_names.contains(&name)
                && !authored_trace_names.contains(&name)
            {
                let mut known: Vec<String> = built_in_names.iter().cloned().collect();
                known.extend(authored_trace_names.iter().cloned());
                diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading_spaces(line) + 1,
                        severity: DoctorSeverity::Error,
                        code: "trigger_trace_unknown_diagnostics".to_owned(),
                        message: format!(
                            "`trigger @trace.{name}` does not resolve. Built-in trace events: {}. Authored trace events in scope: {}.",
                            format_name_list(&built_in_names),
                            format_name_list(&authored_trace_names),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
            }

            i += 1;
        }
    }

    diagnostics
}

pub(crate) fn canonical_payload_event(name: &str, canonical: &BTreeSet<String>) -> bool {
    !canonical.is_empty() && ir::is_reserved_trace_event_name(name)
}

/// After spotting `trigger event.trace agent_run`, walk subsequent
/// lines at deeper indent and flag any `<field> = <expr>` reference
/// where `<field>` is not part of the canonical payload. The check is
/// scoped to the job block that owns the trigger — we stop at the
/// next sibling at the same or shallower indent.
pub(crate) fn scan_payload_field_drift(
    file: &DoctorFile,
    lines: &[&str],
    trigger_line: usize,
    event_name: &str,
    canonical: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let trigger_indent = leading_spaces(lines[trigger_line]);
    let mut i = trigger_line + 1;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let leading = leading_spaces(line);
        // Stop at the next sibling-or-shallower line. Trigger's
        // subscriber body is everything deeper than the trigger.
        if leading <= trigger_indent {
            break;
        }
        // Lines like `tokens_input = payload.tokens_input` or
        // `<field>: <expr>` reference a payload field on the LHS.
        if let Some(field) = trimmed
            .split_once('=')
            .or_else(|| trimmed.split_once(':'))
            .map(|(lhs, _)| lhs.trim())
        {
            // The LHS may include a dotted prefix (e.g.
            // `payload.tokens_input`). Strip the leading segment if
            // it's `payload`.
            let candidate = field
                .strip_prefix("payload.")
                .unwrap_or(field)
                .split_whitespace()
                .next()
                .unwrap_or("");
            if !candidate.is_empty()
                && candidate
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !canonical.contains(candidate)
            {
                // Surface as drift only if the LHS resembles a
                // field reference (lowercase ident); avoid false
                // positives on full expressions.
                if candidate
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase())
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading + 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_run_subscriber_payload_drift_diagnostics".to_owned(),
                        message: format!(
                            "subscriber references `{candidate}` but `agent_run`'s canonical payload does not declare it. Valid fields: {}.",
                            payload_field_list(canonical),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                    let _ = event_name; // pin for future per-event errors
                }
            }
        }
        i += 1;
    }
    diagnostics
}
