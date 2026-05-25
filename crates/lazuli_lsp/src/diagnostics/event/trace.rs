//! `event.trace` trigger reachability diagnostics.
//!
//! `event.trace <name>` declares an observability-only event. By
//! design it sits *outside* the reaction graph — no jobs subscribe to
//! it, no transactional outbox sees it, no broker registration is
//! emitted. So if an author writes `trigger event <name>` for a name
//! that resolves to an `event.trace` declaration, the job will never
//! fire. This producer catches that.
//!
//! The recognizer uses [`collect_trace_events`] from
//! [`super::facts`] to pre-compute the set of qualified trace event
//! names; the second pass then matches every `trigger event ...` line
//! against that set.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{feature_name, leading_spaces, simple_canonical_diagnostic};

use super::facts::collect_trace_events;

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
