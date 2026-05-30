//! Eval-block predicate shape + non-determinism check for `agent.evals`.
//!
//! Cases live at six-space indent (`case <name>`); each `requires` /
//! `forbids` / `golden` child runs through the predicate shape gate. An
//! `evals` block without `temperature 0` + `seed <int>` warns so the
//! inner loop catches non-deterministic runs before doctor.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

use super::iter_agent_blocks;

/// Reject eval cases whose *predicate language* or *vocabulary* is
/// malformed. Cases without `temperature 0` + `seed <int>` also surface
/// a warning here so the inner loop catches non-determinism without
/// waiting on `lazuli doctor`.
pub(crate) fn agent_evals_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (header, body) in iter_agent_blocks(source) {
        let mut in_evals = false;
        let mut has_evals_block = false;
        let mut temperature_zero = false;
        let mut seed_present = false;

        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);
            if leading == 4 {
                if let Some(rest) = trimmed.strip_prefix("temperature ") {
                    temperature_zero = rest.trim().parse::<f64>().ok() == Some(0.0);
                } else if trimmed.starts_with("seed ") {
                    seed_present = true;
                }
                in_evals = trimmed == "evals";
                if in_evals {
                    has_evals_block = true;
                }
                continue;
            }
            if !in_evals {
                continue;
            }
            if leading == 6 {
                if trimmed.starts_with("given ") || trimmed == "given" {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "`given` is legacy vocabulary; eval blocks use `case <name>` then `requires`/`forbids` clauses.",
                    ));
                } else if !trimmed.starts_with("case ") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "eval children must be `case <name>` blocks at six-space indentation.",
                    ));
                } else if trimmed
                    .strip_prefix("case ")
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "`case` requires a name (e.g. `case redacts_email`).",
                    ));
                }
            }
            if leading == 8 {
                if trimmed.starts_with("expect ") || trimmed == "expect" {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "`expect` is legacy vocabulary; eval assertions are `requires <predicate>` or `forbids <predicate>`.",
                    ));
                    continue;
                }
                // Cut A.10: `golden "./path.jsonl" [min_score N]` is a
                // valid case child alongside requires/forbids.
                if trimmed.starts_with("golden ") {
                    let rest = trimmed.strip_prefix("golden ").unwrap_or("").trim();
                    if !rest.starts_with('"') {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            raw,
                            DiagnosticSeverity::ERROR,
                            "agent_evals_diagnostics",
                            "`golden` requires a quoted file path: `golden \"./path.jsonl\"`.",
                        ));
                    }
                    continue;
                }
                let predicate = trimmed
                    .strip_prefix("requires ")
                    .or_else(|| trimmed.strip_prefix("forbids "));
                let Some(predicate) = predicate else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "eval children are `requires <predicate>`, `forbids <predicate>`, or `golden \"./path\"`.",
                    ));
                    continue;
                };
                if predicate.trim().is_empty() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        "eval assertion is missing its predicate body.",
                    ));
                    continue;
                }
                if let Some(message) = validate_eval_predicate_shape(predicate) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        raw,
                        DiagnosticSeverity::ERROR,
                        "agent_evals_diagnostics",
                        &message,
                    ));
                }
            }
        }

        if has_evals_block && (!temperature_zero || !seed_present) {
            let reason = if !temperature_zero {
                "missing `temperature 0`"
            } else {
                "missing `seed <int>`"
            };
            diagnostics.push(simple_canonical_diagnostic(
                header,
                lines[header],
                DiagnosticSeverity::WARNING,
                "eval_nondeterministic_warning",
                &format!(
                    "agent declares `evals` but is non-deterministic ({reason}); cases run as informational results until both `temperature 0` and `seed <int>` are pinned."
                ),
            ));
        }
    }

    diagnostics
}

/// File-local predicate-shape check. The full closed-predicate AST
/// lives in `lazuli_analyzer`; this layer only catches obviously
/// malformed bodies (missing rhs after `contains`, unknown ordered
/// operators, dangling `tools.calls`). Anything that looks like a
/// `<path> <op> <value>` shape passes through — doctor and analyzer
/// own the deeper validation.
pub(crate) fn validate_eval_predicate_shape(body: &str) -> Option<String> {
    let body = body.trim();
    if let Some(rest) = body.strip_prefix("tools.calls ") {
        let mut parts = rest.split_whitespace();
        let op = parts.next();
        let target = parts.next();
        if !matches!(op, Some("includes" | "excludes")) {
            return Some(
                "`tools.calls` operator must be `includes` or `excludes` followed by a tool reference"
                    .to_owned(),
            );
        }
        if target.is_none() {
            return Some("`tools.calls <op>` requires a tool reference target".to_owned());
        }
        if parts.next().is_some() {
            return Some("`tools.calls <op> <ref>` accepts a single tool reference".to_owned());
        }
        return None;
    }

    if let Some(idx) = body.find(" contains ") {
        let lhs = body[..idx].trim();
        let rhs = body[idx + " contains ".len()..].trim();
        if lhs.is_empty() {
            return Some("`contains` predicate requires a left-hand reference".to_owned());
        }
        if rhs.is_empty() {
            return Some("`contains` predicate requires a right-hand value".to_owned());
        }
        // SPEC-04 — accept the canonical BARE semantic type (`contains Email`)
        // alongside a quoted literal or the deprecated `@semantic.<Type>` form.
        let bare_semantic = lazuli_keywords::SEMANTIC_TYPES.contains(&rhs);
        if !(rhs.starts_with('"') || rhs.starts_with("@semantic.") || bare_semantic) {
            return Some(
                "`contains` rhs must be a quoted string literal or a semantic type (e.g. `Email`)"
                    .to_owned(),
            );
        }
        return None;
    }

    None
}
