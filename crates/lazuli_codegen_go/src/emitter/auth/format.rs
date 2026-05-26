//! Auth emitter formatting helpers.
//!
//! Pure formatting / escaping / duration-parsing utilities lifted from the
//! original `auth.rs` god file. Holding these as a sibling keeps `mod.rs`
//! focused on the orchestrator + contract walks.

use lazuli_ir::{QualifiedName, TheftAction};

use super::super::printer::GoPrinter;

pub(super) fn write_aligned_kv_rows(p: &mut GoPrinter, rows: &[(String, String)]) {
    let key_width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
}

pub(super) fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

pub(super) fn qualified_resource_name(qname: &QualifiedName) -> String {
    qname.name.clone()
}

pub(super) fn password_algorithm_expr(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "argon2id" => "auth.AlgoArgon2id".to_owned(),
        "bcrypt" => "auth.AlgoBcrypt".to_owned(),
        other => format!("auth.PasswordAlgorithm(\"{}\")", escape_string(other)),
    }
}

pub(super) fn mfa_method_expr(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "totp" => "auth.MfaMethodTOTP".to_owned(),
        other => format!("auth.MfaMethod(\"{}\")", escape_string(other)),
    }
}

pub(super) fn duration_expr(raw: &str) -> (String, Option<String>) {
    duration_expr_for(raw, "AuthSessions.ttl")
}

pub(super) fn duration_expr_for(raw: &str, label: &str) -> (String, Option<String>) {
    match parse_duration_literal(raw) {
        Some(expr) => (expr, None),
        None => (
            "0 * time.Second".to_owned(),
            Some(format!(
                "// TODO(auth-ttl): unsupported {label} literal \"{}\"; emit a time.Duration expression.",
                escape_string(raw),
            )),
        ),
    }
}

pub(super) fn theft_action_expr(action: TheftAction) -> &'static str {
    match action {
        TheftAction::RevokeSessionFamily => "auth.TheftRevokeSessionFamily",
        TheftAction::RevokeUser => "auth.TheftRevokeUser",
    }
}

fn parse_duration_literal(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let compact = trimmed.replace(' ', "");
    if let Some(parsed) = parse_number_unit(&compact) {
        return Some(parsed);
    }

    let mut parts = trimmed.split_whitespace();
    let n = parts.next()?;
    let unit = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    format_duration_unit(n, unit)
}

fn parse_number_unit(compact: &str) -> Option<String> {
    let split_at = compact
        .char_indices()
        .find_map(|(idx, ch)| (!ch.is_ascii_digit()).then_some(idx))?;
    let (n, unit) = compact.split_at(split_at);
    format_duration_unit(n, unit)
}

fn format_duration_unit(n: &str, unit: &str) -> Option<String> {
    if n.is_empty() || !n.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let value: u64 = n.parse().ok()?;
    if value == 0 {
        return Some("0".to_owned());
    }
    let unit = unit.trim().to_ascii_lowercase();
    match unit.as_str() {
        "ms" | "millisecond" | "milliseconds" => Some(scale_duration(value, "time.Millisecond")),
        "s" | "sec" | "secs" | "second" | "seconds" => Some(scale_duration(value, "time.Second")),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(scale_duration(value, "time.Minute")),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(scale_duration(value, "time.Hour")),
        "d" | "day" | "days" => Some(format!("{value} * 24 * time.Hour")),
        "w" | "week" | "weeks" => Some(format!("{value} * 7 * 24 * time.Hour")),
        _ => None,
    }
}

fn scale_duration(value: u64, unit: &str) -> String {
    if value == 1 {
        unit.to_owned()
    } else {
        format!("{value} * {unit}")
    }
}

pub(super) fn escape_route_segment(raw: &str) -> String {
    raw.split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub(super) fn pascal_case(s: &str) -> String {
    super::super::casing::pascal_case(s)
}

pub(super) fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}
