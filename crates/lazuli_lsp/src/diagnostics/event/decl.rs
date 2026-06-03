//! Event-declaration shape diagnostics.
//!
//! The three producers here check file-local shape on the
//! declaration surfaces:
//!
//! * [`event_kind_diagnostics`] — reject the abandoned
//!   `observability_only` modifier; the canonical form is
//!   `event.trace <name>`.
//! * [`event_locator_diagnostics`] — `payload = event` is forbidden
//!   (assigning the implicit event object wholesale), and any
//!   `event.*` reference outside an `event.trace <name>` declaration
//!   should resolve to `payload.*` / `envelope.*` instead.
//! * [`target_binding_diagnostics`] — commands and declarative jobs
//!   should bind to `target` (the loaded resource snapshot); `self`
//!   is reserved for rules and workflow transitions.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

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

pub(crate) fn event_locator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        // Skip `#` comments, `event.trace <name>` declarations, AND the
        // `@doctor.allow(...)` waiver node — a reason string containing
        // `event.` is opaque prose, not an authored locator (spec 0028 Gap A).
        if trimmed.starts_with('#')
            || trimmed.starts_with("event.trace ")
            || lazuli_syntax::doctor_allow::line_is_doctor_allow_node(trimmed)
        {
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

#[cfg(test)]
mod doctor_allow_gap_a_tests {
    use super::*;

    #[test]
    fn doctor_allow_reason_with_event_dot_does_not_false_fire() {
        // Spec 0028 Gap A: a `@doctor.allow(...)` reason mentioning `event.`
        // (e.g. "prevent.") must NOT trip event-locator-namespace.
        let src = "@doctor.allow(SOME-RULE-001, reason: \"this prevent. event. drift\")\nfeature x\n";
        assert!(
            event_locator_diagnostics(src).is_empty(),
            "node-line reason must not produce an event-locator finding"
        );
    }

    #[test]
    fn genuine_event_dot_on_authored_line_still_fires() {
        // Guard against over-suppression: a real `event.*` reference on a
        // non-node line still surfaces the warning.
        let src = "  send event.customer_id\n";
        assert!(
            !event_locator_diagnostics(src).is_empty(),
            "a genuine event.* reference must still fire"
        );
    }
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
