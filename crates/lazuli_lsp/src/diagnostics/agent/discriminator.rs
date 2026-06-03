//! `discriminator` field-marker scoping check.
//!
//! Per proposal §A2 the marker is record-only; authors who attach it to
//! other constructs (agent input, command input, query params) get a
//! fast LSP error. Includes the `contains_token` helper used to avoid
//! false positives on names like `discriminators_list`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

/// Reject the `discriminator` field marker when it appears outside a
/// `record <Name>` block. Per proposal §A2 the marker is record-only;
/// authors who attach it to other constructs (agent input, command
/// input, query params) get a fast LSP error.
pub(crate) fn agent_discriminator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut record_starts: Vec<(usize, usize)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("record ") {
            record_starts.push((index, leading_spaces(line)));
        }
    }

    // Build the half-open ranges that each record occupies. A record
    // ends at the next line whose indent is <= the record's own.
    let mut record_ranges: Vec<(usize, usize)> = Vec::new();
    for (start, record_indent) in record_starts {
        let mut end = lines.len();
        for (offset, line) in lines.iter().enumerate().skip(start + 1) {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if leading_spaces(line) <= record_indent {
                end = offset;
                break;
            }
        }
        record_ranges.push((start, end));
    }

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // The `@doctor.allow(...)` waiver node is not authored field source — a
        // reason mentioning the word `discriminator` is opaque prose, never a
        // misplaced marker (spec 0028 Gap A).
        if lazuli_syntax::doctor_allow::line_is_doctor_allow_node(trimmed) {
            continue;
        }
        // `output discriminator <Enum>` is the agent-side form; not a
        // misuse, skip.
        if trimmed.starts_with("output discriminator ") {
            continue;
        }
        // Look for `discriminator` as a tail modifier on a field-like
        // line: `<name>: <type> ... discriminator`.
        if !contains_token(trimmed, "discriminator") {
            continue;
        }
        if !trimmed.contains(':') {
            continue;
        }
        let in_record = record_ranges
            .iter()
            .any(|(start, end)| index > *start && index < *end);
        if !in_record {
            diagnostics.push(simple_canonical_diagnostic(
                index,
                line,
                DiagnosticSeverity::ERROR,
                "agent_discriminator_diagnostics",
                "`discriminator` is a field marker that only applies inside a `record <Name>` block; it cannot appear elsewhere.",
            ));
        }
    }

    diagnostics
}

/// Stand-alone `discriminator` token (not a substring of a longer
/// identifier). Used to avoid false positives on names like
/// `discriminators_list`.
pub(crate) fn contains_token(line: &str, token: &str) -> bool {
    line.split(|c: char| !(c == '_' || c.is_ascii_alphanumeric()))
        .any(|word| word == token)
}

#[cfg(test)]
mod doctor_allow_gap_a_tests {
    use super::*;

    #[test]
    fn doctor_allow_reason_with_discriminator_word_does_not_false_fire() {
        // Spec 0028 Gap A: the word `discriminator` in a waiver reason is opaque
        // prose — it must NOT raise the field-marker-misuse ERROR.
        let src = "@doctor.allow(SOME-RULE-001, reason: \"discriminator: not a marker here\")\nfeature x\n";
        assert!(
            agent_discriminator_diagnostics(src).is_empty(),
            "node-line reason must not produce a discriminator finding"
        );
    }

    #[test]
    fn genuine_misplaced_discriminator_still_fires() {
        // A real `<name>: <type> discriminator` outside a record still errors.
        let src = "agent triage\n  input\n    kind: Text discriminator\n";
        assert!(
            !agent_discriminator_diagnostics(src).is_empty(),
            "a genuine misplaced discriminator marker must still fire"
        );
    }
}
