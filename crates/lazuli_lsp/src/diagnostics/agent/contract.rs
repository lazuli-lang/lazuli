//! `agent` header contract: required children + numeric-config shape checks.
//!
//! Required children: `policy`, `output`, `model @llm.<name>`, `prompt`.
//! Shape checks: `temperature` in `[0.0, 2.0]`, `top_p` in `[0.0, 1.0]`,
//! `max_tokens` ≥ 1, `seed` is an integer. Cross-feature `@llm.<name>`
//! resolution lives in `lazuli_cli::doctor`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_float_in_range, leading_spaces, simple_canonical_diagnostic};

pub(crate) fn agent_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading != 2 || !trimmed.starts_with("agent ") {
            index += 1;
            continue;
        }

        let header_index = index;
        let mut has_policy = false;
        let mut has_output = false;
        let mut has_model = false;
        let mut has_prompt = false;
        let mut model_value: Option<&str> = None;
        let mut bad_config: Vec<(usize, String, String)> = Vec::new();

        index += 1;
        while index < lines.len() {
            let inner = lines[index];
            let inner_trimmed = inner.trim_start();
            let inner_leading = leading_spaces(inner);

            if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                index += 1;
                continue;
            }
            if inner_leading <= 2 {
                break;
            }
            if inner_leading == 4 {
                if inner_trimmed.starts_with("policy ") {
                    has_policy = true;
                } else if inner_trimmed.starts_with("output ") {
                    has_output = true;
                } else if let Some(rest) = inner_trimmed.strip_prefix("model ") {
                    has_model = true;
                    model_value = Some(rest.trim());
                } else if inner_trimmed.starts_with("prompt ") {
                    has_prompt = true;
                } else if let Some(rest) = inner_trimmed.strip_prefix("temperature ") {
                    let value = rest.trim();
                    if !is_float_in_range(value, 0.0, 2.0) {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`temperature` requires a float in [0.0, 2.0]".to_owned(),
                        ));
                    }
                } else if let Some(rest) = inner_trimmed.strip_prefix("top_p ") {
                    let value = rest.trim();
                    if !is_float_in_range(value, 0.0, 1.0) {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`top_p` requires a float in [0.0, 1.0]".to_owned(),
                        ));
                    }
                } else if let Some(rest) = inner_trimmed.strip_prefix("max_tokens ") {
                    let value = rest.trim();
                    let valid = value.parse::<u32>().map(|v| v >= 1).unwrap_or(false);
                    if !valid {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`max_tokens` requires a positive integer".to_owned(),
                        ));
                    }
                } else if let Some(rest) = inner_trimmed.strip_prefix("seed ") {
                    let value = rest.trim();
                    if value.parse::<i64>().is_err() {
                        bad_config.push((
                            index,
                            inner.to_owned(),
                            "`seed` requires an integer".to_owned(),
                        ));
                    }
                }
            }
            index += 1;
        }

        if !has_policy {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare an explicit `policy @policy.<name>`.",
            ));
        }
        if !has_output {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare an `output [stream] <Type>`.",
            ));
        }
        if !has_model {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare a `model @llm.<name>`.",
            ));
        } else if let Some(value) = model_value
            && !value.starts_with("@llm.")
        {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`model` on an `agent` must be a `@llm.<name>` reference.",
            ));
        }
        if !has_prompt {
            diagnostics.push(simple_canonical_diagnostic(
                header_index,
                lines[header_index],
                DiagnosticSeverity::ERROR,
                "agent-contract",
                "`agent` declarations must declare a `prompt \"./path\"` template.",
            ));
        }
        for (idx, owned_line, message) in bad_config {
            diagnostics.push(simple_canonical_diagnostic(
                idx,
                &owned_line,
                DiagnosticSeverity::ERROR,
                "agent-contract",
                &message,
            ));
        }
    }

    diagnostics
}
