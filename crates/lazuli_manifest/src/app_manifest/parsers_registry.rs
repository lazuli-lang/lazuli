//! Registry-specific line-level parsers for `parse_app_registry`.
//! Each helper is `pub(super)` and re-exported from `parsers.rs`.

use lazuli_ir::WebhookEventField;

pub(super) fn registry_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        // B1 (W3-blockers) — `bindings` is registry-level sugar over
        // `integrations`. Same IR target (`AppIntegration`), same
        // codegen, but indent-6 children may use the simplified
        // `endpoint env.X` / `auth keys env.A env.B` surface instead
        // of nesting under `credentials platform`. The parser lowers
        // the sugar to canonical credential bindings on the fly.
        "bindings" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "packs" => Some("packs"),
        "tools" => Some("tools"),
        // Webhooks expanded cycle — `webhook_events` is the registry-side
        // catalog of expected inbound envelope shapes.
        "webhook_events" => Some("webhook_events"),
        // Roadmap §1.10 — `secret_rotation <name>` is a NAMED block
        // at indent-2 (not a container with indent-4 children like
        // `env`). The parser detects the header inline and switches
        // current_child to `"secret_rotation"`; indent-4 lines feed
        // the currently open `SecretRotation` entry.
        "secret_rotation" => Some("secret_rotation"),
        _ => None,
    }
}

/// Webhooks expanded cycle — parse one indent-6 line of a
/// `webhook_events.<name>` envelope.
///
/// Grammar (positional, mirrors the per-record field shape):
///
/// ```text
/// <field_name>: <Type> [@semantic.X | @pii.Y ...] (required | optional)
/// ```
///
/// The type token is captured verbatim because the envelope is
/// provider-side. `@semantic.*` / `@pii.*` decorators are collected
/// into `capabilities` in author order. The trailing `required` or
/// `optional` keyword toggles `required`.
pub(super) fn parse_webhook_event_field(trimmed: &str) -> Option<WebhookEventField> {
    let (name_raw, rest) = trimmed.split_once(':')?;
    let name = name_raw.trim();
    if name.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let type_text = tokens[0].to_owned();
    let mut required = true;
    let mut capabilities: Vec<String> = Vec::new();
    for token in &tokens[1..] {
        match *token {
            "required" => required = true,
            "optional" => required = false,
            other if other.starts_with('@') => capabilities.push(other.to_owned()),
            _ => {}
        }
    }
    Some(WebhookEventField {
        name: name.to_owned(),
        type_text,
        required,
        capabilities,
    })
}

pub(super) fn webhook_event_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("webhook_event ")?;
    let name = rest.split_whitespace().next()?;
    (!name.is_empty()).then_some(name)
}
