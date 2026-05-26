//! Cut A.7 — file-local checks on `expose http` blocks.
//!
//! Cross-feature path collisions live in doctor; this layer handles
//! same-file path duplicates, missing path slots, slot-shape misuse,
//! and the GET-streaming warning. Shared helpers (`LocalExpose`,
//! `extract_path_slots`, `lsp_normalise_path`) live here because every
//! consumer is in this file.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

use super::iter_agent_blocks;

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
