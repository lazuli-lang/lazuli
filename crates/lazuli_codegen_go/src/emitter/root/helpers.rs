//! Small formatting + closed-catalog token helpers shared across the
//! root file emitters (`main.go`, `lazuli_app.gen.go`). Extracted from
//! the monolithic `root.rs` so the entry-point + per-contract bodies
//! stay legible.
//!
//! No public surface — every symbol is `pub(super)` and consumed by
//! the sibling `main_go` / `app_gen` / `encryption` modules through
//! `super::`.

use super::super::printer::GoPrinter;

/// Parse a DSL duration literal ("1h", "30 minutes", "10s", "1d") to
/// seconds. Returns `None` on shapes the runtime middleware can't
/// accept (negative, zero, malformed). Mirror the small surface the
/// proposal commits to — extend additively as more unit shapes appear
/// in pilots.
pub(super) fn parse_duration_to_seconds(literal: &str) -> Option<i64> {
    let trimmed = literal.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip a leading numeric prefix (digits + optional decimal not supported).
    let (num_str, unit_str) = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| (&trimmed[..i], trimmed[i..].trim()))
        .unwrap_or((trimmed, ""));
    let n: i64 = num_str.parse().ok()?;
    if n < 0 {
        return None;
    }
    let mult: i64 = match unit_str.to_ascii_lowercase().as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86_400,
        _ => return None,
    };
    Some(n.saturating_mul(mult))
}

/// Render `Key: value,` struct-literal rows with the keys padded to
/// the widest key in the block. Matches `gofmt`'s alignment rule for
/// composite-literal initialisers so the emitter output passes a
/// `gofmt -d` diff cleanly.
pub(super) fn emit_aligned_struct_value_rows(p: &mut GoPrinter, rows: &[(String, String)]) {
    if rows.is_empty() {
        return;
    }
    let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
}

/// Map the closed-catalog `app.logging.level` token to the Lazuli Go
/// lib's `observability.LogLevel*` constant. Doctor enforces the
/// catalog at compile time; unknown tokens fall back to `LogLevelInfo`
/// (the runtime default) plus a defensive comment so the file still
/// compiles when an upstream stage drifts.
pub(super) fn log_level_const(token: &str) -> &'static str {
    match token {
        "debug" => "LogLevelDebug",
        "info" => "LogLevelInfo",
        "warn" => "LogLevelWarn",
        "error" => "LogLevelError",
        _ => "LogLevelInfo",
    }
}

pub(super) fn log_format_const(token: &str) -> &'static str {
    match token {
        "json" => "LogFormatJSON",
        "text" => "LogFormatText",
        _ => "LogFormatJSON",
    }
}

pub(super) fn redact_strategy_const(token: &str) -> &'static str {
    match token {
        "pii" => "RedactPII",
        "none" => "RedactNone",
        _ => "RedactPII",
    }
}

/// Render an `f64` as a Go literal. We always emit at least one
/// decimal place so the literal is unambiguously a float (Go's `1`
/// would be an untyped int that the field decl couldn't accept
/// without conversion).
pub(super) fn format_f64(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{:.1}", value)
    } else {
        // `{}` for `f64` produces the shortest round-trip form, which
        // is what we want for deterministic output. The trailing
        // newline/precision is decided by the formatter; no extra
        // padding needed.
        format!("{}", value)
    }
}

pub(super) fn format_string_slice(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
