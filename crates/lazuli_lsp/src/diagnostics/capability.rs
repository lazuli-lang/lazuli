//! Capability syntax + arg-discipline helpers + the `@cap.File`
//! contract checker.
//!
//! Five shared helpers live alongside one diagnostic producer because
//! they all key on the `@cap.<Family>(<key>:<value>, ...)` arglist
//! shape that the cache / crypto / agent / file producers each parse:
//!
//! - `capability_args` / `capability_arg` — the `@cap.X(...)` arg
//!   tokenizer / per-key getter.
//! - `warn_unknown_capability_args` — emits `capability-arguments`
//!   warnings when an arglist names a key outside the closed catalog.
//! - `is_duration_literal` / `is_file_size_literal` /
//!   `is_retention_duration_literal` / `is_key_scope` /
//!   `is_float_in_range` — literal-shape predicates used across
//!   storage-, cache-, crypto-, and agent-side checks.
//! - `file_capability_contract_diagnostics` (storage-file-contract)
//!   — `@cap.File` must declare `max_size:<size>` and `accept:<mime>`,
//!   and the values must lex as canonical literals.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, simple_canonical_diagnostic};

pub(crate) fn file_capability_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || !line.contains("@cap.File") {
            continue;
        }

        let Some(args) = capability_args(line, "File") else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` should declare `max_size:<size>` and `accept:<mime>` so generated upload APIs and validators have an explicit contract.",
            ));
            continue;
        };

        warn_unknown_capability_args(
            &mut diagnostics,
            line_index,
            line,
            "@cap.File",
            &args,
            // Row 30 — `visibility` + `signed_ttl` are typed arguments
            // recognised by the analyzer (`type_ref_from_syntax`); add
            // them to the canonical set so the LSP no longer warns on
            // the canonical authoring form.
            //
            // 2026-05-27 — `auto_photo_policy` adicionado pra cobrir o
            // auto-photo helper (lazuli runtime AutoPhoto*): quando o
            // arg está presente, codegen sintetiza request/confirm/
            // clear/get_photo_url commands com a policy informada.
            // Hostpoint usa em Host.profile_photo + Traveler.profile_photo.
            &[
                "max_size",
                "accept",
                "visibility",
                "signed_ttl",
                "auto_photo_policy",
            ],
        );

        if !args.iter().any(|(key, _)| key == "max_size") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` should declare `max_size:<size>` such as `25mb`.",
            ));
        }

        if !args.iter().any(|(key, _)| key == "accept") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` should declare `accept:<mime>` such as `text/csv`.",
            ));
        }

        if let Some(max_size) = capability_arg(&args, "max_size")
            && !is_file_size_literal(max_size)
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` max_size should use a positive size literal such as `500kb`, `25mb`, or `1gb`.",
            ));
        }

        if let Some(accept) = capability_arg(&args, "accept")
            && accept.trim().is_empty()
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "storage-file-contract",
                "`@cap.File` accept should name a MIME type such as `text/csv` or `image/png`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn capability_args(line: &str, capability: &str) -> Option<Vec<(String, String)>> {
    let marker = format!("@cap.{capability}(");
    let start = line.find(&marker)? + marker.len();
    let args = line[start..].split_once(')')?.0;

    Some(
        args.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.split_once(':')
                    .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
                    .unwrap_or_else(|| (part.to_owned(), String::new()))
            })
            .collect(),
    )
}

pub(crate) fn capability_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(arg_key, _)| arg_key == key)
        .map(|(_, value)| value.as_str())
}

pub(crate) fn warn_unknown_capability_args(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    capability: &str,
    args: &[(String, String)],
    allowed: &[&str],
) {
    for (key, _) in args {
        if !allowed.contains(&key.as_str()) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "capability-arguments",
                &format!(
                    "{capability} only accepts canonical arguments: {}.",
                    allowed.join(", ")
                ),
            ));
        }
    }
}

pub(crate) fn is_duration_literal(value: &str) -> bool {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();

    if digit_count == 0 || digit_count == value.len() {
        return false;
    }

    let Ok(amount) = value[..digit_count].parse::<u64>() else {
        return false;
    };

    amount > 0 && matches!(&value[digit_count..], "s" | "m" | "h" | "d")
}

pub(crate) fn is_file_size_literal(value: &str) -> bool {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();

    if digit_count == 0 || digit_count == value.len() {
        return false;
    }

    let Ok(amount) = value[..digit_count].parse::<u64>() else {
        return false;
    };

    amount > 0 && matches!(&value[digit_count..], "b" | "kb" | "mb" | "gb")
}

pub(crate) fn is_retention_duration_literal(value: &str) -> bool {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();

    if digit_count == 0 || digit_count == value.len() {
        return false;
    }

    let Ok(amount) = value[..digit_count].parse::<u64>() else {
        return false;
    };

    amount > 0 && matches!(&value[digit_count..], "h" | "d" | "w" | "mo" | "y")
}

pub(crate) fn is_key_scope(value: &str) -> bool {
    value.strip_prefix("@key.").is_some_and(is_identifier)
}

pub(crate) fn is_float_in_range(value: &str, min: f64, max: f64) -> bool {
    value
        .parse::<f64>()
        .map(|v| v >= min && v <= max)
        .unwrap_or(false)
}
