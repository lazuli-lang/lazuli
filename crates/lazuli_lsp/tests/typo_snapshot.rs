//! Diagnostic-class snapshot test for R2 Wave 1 typo detection.
//!
//! Runs `lazuli_lsp::diagnostics_for_source` over
//! `editors/vscode/tests/grammar/typos.lzi` and asserts each expected
//! diagnostic listed in `typos.lzi.expected-diagnostics.json` is
//! emitted at the expected 1-indexed line, with the expected code +
//! severity + suggestion. Complements the inline per-context tests in
//! `crates/lazuli_lsp/src/lib.rs` — those guard each detector in
//! isolation; THIS guards that all 8 still fire when colocated in
//! one source file (the realistic editor scenario).
//!
//! Invocation:  `cargo test -p lazuli_lsp --test typo_snapshot`
//! Also wired into `editors/vscode/package.json::test:diagnostics`.
//!
//! The fixture lives under `editors/vscode/tests/grammar/` so the
//! grammar (TextMate) snap and the LSP diagnostic snap stay
//! side-by-side; future contributors find both in one place.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_lsp::diagnostics_for_source;
use serde_json::Value;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

fn repo_path(rel: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/lazuli_lsp, walk up two levels to repo root.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().parent().unwrap().join(rel)
}

fn diagnostic_code(d: &Diagnostic) -> Option<&str> {
    match d.code.as_ref()? {
        NumberOrString::String(s) => Some(s.as_str()),
        NumberOrString::Number(_) => None,
    }
}

fn severity_label(s: Option<DiagnosticSeverity>) -> &'static str {
    match s {
        Some(DiagnosticSeverity::ERROR) => "Error",
        Some(DiagnosticSeverity::WARNING) => "Warning",
        Some(DiagnosticSeverity::INFORMATION) => "Information",
        Some(DiagnosticSeverity::HINT) => "Hint",
        _ => "Unknown",
    }
}

#[test]
fn typos_fixture_emits_all_expected_diagnostics() {
    let fixture_path = repo_path("editors/vscode/tests/grammar/typos.lzi");
    let expected_path =
        repo_path("editors/vscode/tests/grammar/typos.lzi.expected-diagnostics.json");

    let source = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|err| panic!("read fixture {fixture_path:?}: {err}"));
    let expected_raw = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|err| panic!("read expected json {expected_path:?}: {err}"));
    let expected: Value = serde_json::from_str(&expected_raw)
        .unwrap_or_else(|err| panic!("parse expected json {expected_path:?}: {err}"));

    let diagnostics = diagnostics_for_source(&source);

    let expected_list = expected
        .get("diagnostics")
        .and_then(Value::as_array)
        .expect("`diagnostics` array missing from expected JSON");

    let mut missing: Vec<String> = Vec::new();

    for entry in expected_list {
        let exp_line_1based = entry
            .get("line")
            .and_then(Value::as_u64)
            .expect("each expected diagnostic must have a numeric `line`")
            as u32;
        let exp_line_0based = exp_line_1based.saturating_sub(1);
        let exp_code = entry
            .get("code")
            .and_then(Value::as_str)
            .expect("each expected diagnostic must have a string `code`");
        let exp_severity = entry
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("Error");
        let exp_msg_contains = entry
            .get("message_contains")
            .and_then(Value::as_str)
            .unwrap_or("");
        let exp_suggestion = entry
            .get("suggestion")
            .and_then(Value::as_str)
            .unwrap_or("");

        let hit = diagnostics.iter().find(|d| {
            d.range.start.line == exp_line_0based
                && diagnostic_code(d) == Some(exp_code)
                && severity_label(d.severity) == exp_severity
                && (exp_msg_contains.is_empty() || d.message.contains(exp_msg_contains))
                && (exp_suggestion.is_empty() || d.message.contains(exp_suggestion))
        });

        if hit.is_none() {
            missing.push(format!(
                "  line {exp_line_1based} code={exp_code} severity={exp_severity} \
                 contains={exp_msg_contains:?} suggestion={exp_suggestion:?}"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "{} expected diagnostic(s) not found in {}:\n{}\n\nactual diagnostics ({} total):\n{}",
        missing.len(),
        fixture_path.display(),
        missing.join("\n"),
        diagnostics.len(),
        format_diagnostics(&diagnostics),
    );
}

#[test]
fn typos_fixture_covers_all_eight_diagnostic_classes() {
    let expected_path =
        repo_path("editors/vscode/tests/grammar/typos.lzi.expected-diagnostics.json");
    let expected_raw =
        std::fs::read_to_string(&expected_path).expect("read expected json");
    let expected: Value = serde_json::from_str(&expected_raw).expect("parse expected json");

    let codes: std::collections::BTreeSet<&str> = expected
        .get("diagnostics")
        .and_then(Value::as_array)
        .expect("`diagnostics` array")
        .iter()
        .filter_map(|d| d.get("code").and_then(Value::as_str))
        .collect();

    let required: &[&str] = &[
        "app-unknown-kind",
        "registry-unknown-kind",
        "feature-unknown-kind",
        "view-unknown-kind",
        "surface-unknown-kind",
        "command-statement-unknown",
        "query-statement-unknown",
        "audience-unknown-kind",
    ];

    let missing: Vec<&str> = required.iter().copied().filter(|c| !codes.contains(c)).collect();
    assert!(
        missing.is_empty(),
        "expected typos.lzi.expected-diagnostics.json to cover all 8 R2-Wave-1 classes; \
         missing: {missing:?}",
    );
}

fn format_diagnostics(diagnostics: &[Diagnostic]) -> String {
    let mut by_line: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for d in diagnostics {
        by_line
            .entry(d.range.start.line)
            .or_default()
            .push(format!(
                "  line {:>3} [{}] {}: {}",
                d.range.start.line + 1,
                severity_label(d.severity),
                diagnostic_code(d).unwrap_or("<numeric>"),
                d.message,
            ));
    }
    by_line.into_values().flatten().collect::<Vec<_>>().join("\n")
}
