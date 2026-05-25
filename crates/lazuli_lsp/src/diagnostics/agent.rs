//! Diagnostics for `agent <name>` declarations (Lazuli AI primitive).
//!
//! Covers five orthogonal checks, all file-local — cross-feature
//! reachability (tool target resolution, policy compatibility,
//! discriminator enum/record lookup) lives in `lazuli_cli::doctor`. The
//! LSP is the fast inner loop; doctor is the workspace pass.
//!
//! | Producer | Concern |
//! |---|---|
//! | [`agent_contract_diagnostics`] | The `agent` header demands `policy`, `output`, `model @llm.*`, `prompt`; plus shape checks for `temperature` / `top_p` / `max_tokens` / `seed`. |
//! | [`agent_tools_diagnostics`] | Each entry in `tools` is `@tool.<dotted>` or `[<feature>.]<kind>[.<sub>].<name>`. |
//! | [`agent_evals_diagnostics`] | `evals` children are `case <name>` blocks containing `requires` / `forbids` / `golden`; `eval` requires `temperature 0` + `seed`. |
//! | [`agent_discriminator_diagnostics`] | `discriminator` is a `record`-only field marker. |
//! | [`agent_expose_diagnostics`] | `expose http` slot shape + same-file collision check; GET + `output stream` warns. |
//!
//! Shared helpers (`iter_agent_blocks`, `validate_tool_reference_shape`,
//! `validate_eval_predicate_shape`, `contains_token`, `LocalExpose`,
//! `extract_path_slots`, `lsp_normalise_path`) stay here because every
//! consumer is in this file.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    is_float_in_range, is_lower_ident, leading_spaces, simple_canonical_diagnostic,
};

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
        } else if let Some(value) = model_value {
            if !value.starts_with("@llm.") {
                diagnostics.push(simple_canonical_diagnostic(
                    header_index,
                    lines[header_index],
                    DiagnosticSeverity::ERROR,
                    "agent-contract",
                    "`model` on an `agent` must be a `@llm.<name>` reference.",
                ));
            }
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

// =============================================================================
// Cut A — file-local additions for `tools`, `evals`, and discriminator scoping.
//
// These are intentionally file-local: cross-feature resolution of tool
// targets, policy compatibility, and discriminator enum/record lookup
// lives in `crates/lazuli_cli/src/doctor.rs` (Phase 3). The LSP is the
// fast inner loop; doctor is the workspace pass.
//
// See docs/proposals/ai-primitives-v0-implementation.md §6.
// =============================================================================

/// Iterate every `agent <name>` block in the source, yielding the
/// header line index and the body slice (one-based inclusive on the
/// header, exclusive on the next sibling). The caller decides which
/// children to inspect. Shared helper for the three Cut A LSP checks.
pub(crate) fn iter_agent_blocks(source: &str) -> Vec<(usize, Vec<usize>)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);
        if leading == 2 && trimmed.starts_with("agent ") {
            let header = index;
            let mut body = Vec::new();
            index += 1;
            while index < lines.len() {
                let inner = lines[index];
                let inner_trimmed = inner.trim_start();
                if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                    body.push(index);
                    index += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                body.push(index);
                index += 1;
            }
            blocks.push((header, body));
            continue;
        }
        index += 1;
    }
    blocks
}

/// Reject tool entries whose *shape* is invalid. Cross-feature
/// reachability is doctor's job — this layer only catches malformed
/// shorthand (e.g. `query.list` with no name; `customer..by_id`).
pub(crate) fn agent_tools_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (_, body) in iter_agent_blocks(source) {
        let mut in_tools = false;
        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);
            if leading == 4 {
                in_tools = trimmed == "tools";
                continue;
            }
            if !in_tools {
                continue;
            }
            if leading != 6 {
                continue;
            }
            if let Some(message) = validate_tool_reference_shape(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    raw,
                    DiagnosticSeverity::ERROR,
                    "agent_tools_diagnostics",
                    &message,
                ));
            }
        }
    }

    diagnostics
}

/// Validate one tool-entry source token. The closed shapes:
///   - `@tool.<seg>(.<seg>)*` — adapter tool
///   - `query.list.<name>` / `query.lookup.<name>` / `query.sql.<name>` /
///     `query.view.<name>`
///   - `query.<name>` (unspecified subkind — doctor narrows)
///   - `command.<name>` / `api.<name>`
///   - `<feature>.<above>` cross-feature prefix
pub(crate) fn validate_tool_reference_shape(text: &str) -> Option<String> {
    if text.split_whitespace().count() != 1 {
        return Some("each tool entry is a single qualified reference (one per line)".to_owned());
    }
    let token = text.trim();
    if token.contains("..") {
        return Some(format!("tool reference `{token}` has an empty segment"));
    }

    if let Some(rest) = token.strip_prefix("@tool.") {
        if rest.is_empty() {
            return Some("`@tool.` requires a name (e.g. `@tool.web_search`)".to_owned());
        }
        if rest.split('.').any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "`@tool.<...>` segments must be lower_snake idents; got `{token}`"
            ));
        }
        return None;
    }

    let segments: Vec<&str> = token.split('.').collect();
    let valid_local = matches!(
        segments.as_slice(),
        ["query", "list", _name]
            | ["query", "lookup", _name]
            | ["query", "sql", _name]
            | ["query", "view", _name]
            | ["query", _name]
            | ["command", _name]
            | ["api", _name]
    );
    if valid_local {
        if segments.iter().any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "tool reference `{token}` segments must be lower_snake idents"
            ));
        }
        return None;
    }

    let valid_cross = matches!(
        segments.as_slice(),
        [_feature, "query", "list", _name]
            | [_feature, "query", "lookup", _name]
            | [_feature, "query", "sql", _name]
            | [_feature, "query", "view", _name]
            | [_feature, "query", _name]
            | [_feature, "command", _name]
            | [_feature, "api", _name]
    );
    if valid_cross {
        if segments.iter().any(|seg| !is_lower_ident(seg)) {
            return Some(format!(
                "tool reference `{token}` segments must be lower_snake idents"
            ));
        }
        return None;
    }

    Some(format!(
        "tool reference `{token}` is not a recognised shape; expected `<feature>.<kind>.<name>`, `<kind>.<name>`, or `@tool.<dotted>` where kind is `query[.list|.lookup|.sql|.view]`, `command`, or `api`"
    ))
}

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
        if !(rhs.starts_with('"') || rhs.starts_with("@semantic.")) {
            return Some(
                "`contains` rhs must be a quoted string literal or a `@semantic.<Type>` reference"
                    .to_owned(),
            );
        }
        return None;
    }

    None
}

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

/// Cut A.7 — file-local checks on `expose http` blocks. Cross-feature
/// path collisions live in doctor; this layer handles same-file path
/// duplicates, missing path slots, slot-shape misuse, and the
/// GET-streaming warning.
pub(crate) fn agent_expose_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Pass 1: collect every (method, path) declared by agents + apis
    // in this file. Used for the same-file collision check.
    let mut local_paths: Vec<LocalExpose> = Vec::new();

    for (header, body) in iter_agent_blocks(source) {
        let agent_name = lines[header]
            .trim_start()
            .strip_prefix("agent ")
            .map(|n| n.trim().to_owned())
            .unwrap_or_default();
        let mut output_streaming = false;
        let mut input_slot_names: Vec<String> = Vec::new();
        let mut in_input = false;
        let mut in_expose = false;
        let mut expose_header_line: Option<usize> = None;
        let mut expose_method: Option<String> = None;
        let mut expose_path: Option<(usize, String)> = None;
        let mut expose_route_slots: Vec<String> = Vec::new();

        for &line_index in &body {
            let raw = lines[line_index];
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let leading = leading_spaces(raw);

            if leading == 4 {
                in_input = trimmed == "input";
                in_expose = trimmed == "expose http";
                if in_expose {
                    expose_header_line = Some(line_index);
                }
                if let Some(rest) = trimmed.strip_prefix("output ") {
                    let body = rest.trim();
                    if body.starts_with("stream") {
                        output_streaming = true;
                    }
                }
                continue;
            }

            if in_input && leading == 6 {
                if let Some((name_part, _)) = trimmed.split_once(':') {
                    let name = name_part.trim().to_owned();
                    if !name.is_empty() {
                        input_slot_names.push(name);
                    }
                }
            }

            if in_expose && leading == 6 {
                if let Some(rest) = trimmed.strip_prefix("method ") {
                    expose_method = Some(rest.trim().to_ascii_uppercase());
                } else if let Some(rest) = trimmed.strip_prefix("path ") {
                    let unquoted = rest
                        .trim()
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(rest.trim());
                    expose_path = Some((line_index, unquoted.to_owned()));
                } else if let Some(rest) = trimmed.strip_prefix("route ") {
                    if let Some((name_part, _)) = rest.split_once(':') {
                        expose_route_slots.push(name_part.trim().to_owned());
                    }
                }
            }
        }

        let Some(expose_line) = expose_header_line else {
            continue;
        };
        let (path_line, path_str) = match expose_path {
            Some(p) => p,
            None => continue,
        };

        // Slot-unbound check: every `:slot` in the path must have a
        // matching `route` declaration inside expose http.
        let path_slots = extract_path_slots(&path_str);
        for slot in &path_slots {
            if !expose_route_slots.iter().any(|r| r == slot) {
                diagnostics.push(simple_canonical_diagnostic(
                    path_line,
                    lines[path_line],
                    DiagnosticSeverity::ERROR,
                    "agent_expose_slot_unbound_diagnostics",
                    &format!(
                        "agent `{agent_name}` declares path slot `:{slot}` but the `expose http` block has no matching `route {slot}: <Type>` declaration."
                    ),
                ));
            }
        }

        // Slot-must-use-route check: if a path slot's name collides
        // with an `input` slot name and no `route` declaration covers
        // it, the author meant `route` instead of `input`.
        for slot in &path_slots {
            let in_input = input_slot_names.iter().any(|n| n == slot);
            let in_route = expose_route_slots.iter().any(|r| r == slot);
            if in_input && !in_route {
                diagnostics.push(simple_canonical_diagnostic(
                    path_line,
                    lines[path_line],
                    DiagnosticSeverity::ERROR,
                    "agent_expose_slot_must_use_route_diagnostics",
                    &format!(
                        "agent `{agent_name}` path slot `:{slot}` is declared as `input` — use `route {slot}: <Type>` inside `expose http` for URL slots."
                    ),
                ));
            }
        }

        // Method/streaming mismatch: GET + output stream warns.
        if expose_method.as_deref() == Some("GET") && output_streaming {
            diagnostics.push(simple_canonical_diagnostic(
                expose_line,
                lines[expose_line],
                DiagnosticSeverity::WARNING,
                "agent_expose_method_streaming_mismatch_warning",
                &format!(
                    "agent `{agent_name}` exposes `method GET` but `output stream`; streaming responses typically use POST so clients can send body context."
                ),
            ));
        }

        if let Some(method) = expose_method {
            local_paths.push(LocalExpose {
                line: expose_line,
                method,
                path_normalised: lsp_normalise_path(&path_str),
                origin: format!("agent {agent_name}"),
            });
        }
    }

    // Walk `api <name>` blocks for file-local collision check.
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            let name = trimmed
                .strip_prefix("api ")
                .map(|n| n.split_whitespace().next().unwrap_or("").to_owned())
                .unwrap_or_default();
            let api_line = i;
            let mut method: Option<String> = None;
            let mut path: Option<String> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if leading_spaces(inner) == 4 {
                    if let Some(rest) = inner_trim.strip_prefix("method ") {
                        method = Some(rest.trim().to_ascii_uppercase());
                    } else if let Some(rest) = inner_trim.strip_prefix("path ") {
                        let unquoted = rest
                            .trim()
                            .strip_prefix('"')
                            .and_then(|s| s.strip_suffix('"'))
                            .unwrap_or(rest.trim());
                        path = Some(unquoted.to_owned());
                    }
                }
                j += 1;
            }
            if let (Some(method), Some(path)) = (method, path) {
                local_paths.push(LocalExpose {
                    line: api_line,
                    method,
                    path_normalised: lsp_normalise_path(&path),
                    origin: format!("api {name}"),
                });
            }
            i = j;
            continue;
        }
        i += 1;
    }

    // Local collision: any two LocalExpose entries with same
    // (method, normalised_path) but different `origin` collide
    // *within the same file*.
    for (idx_a, a) in local_paths.iter().enumerate() {
        for b in local_paths.iter().skip(idx_a + 1) {
            if a.method == b.method
                && a.path_normalised == b.path_normalised
                && a.origin != b.origin
            {
                diagnostics.push(simple_canonical_diagnostic(
                    a.line,
                    lines[a.line],
                    DiagnosticSeverity::ERROR,
                    "agent_expose_path_conflict_local_diagnostics",
                    &format!(
                        "{} declares an HTTP route that collides with {} (same method + normalised path) inside this file.",
                        a.origin, b.origin,
                    ),
                ));
            }
        }
    }

    diagnostics
}

#[derive(Debug, Clone)]
pub(crate) struct LocalExpose {
    line: usize,
    method: String,
    path_normalised: String,
    origin: String,
}

pub(crate) fn extract_path_slots(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| segment.strip_prefix(':').map(str::to_owned))
        .collect()
}

pub(crate) fn lsp_normalise_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for (i, segment) in path.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        if segment.starts_with(':') {
            out.push_str(":_");
        } else {
            out.push_str(segment);
        }
    }
    out
}
