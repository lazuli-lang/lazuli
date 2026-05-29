//! Per-feature security projection.
//!
//! Walks a feature's source lines (split into the canonical
//! `feature <Name> { ... }` slice) and collects every typed security
//! observable: PII / E2EE / hashed / encrypted / token / file
//! capability markers on fields; markers on event payload fields;
//! the per-command / per-query / per-job / per-webhook security
//! operation envelope (audit + scope-override + parsed audit
//! origin); and the inbound-webhook security row.
//!
//! Pure text-walker today. Cross-feature checks (e.g.
//! "every @cap.Pii field has a documented purpose") live in
//! `lazuli doctor`; this module is the inspect-side observable.

use super::expand::{collect_event_decls, leading_spaces};
use super::{
    InspectOrigin, InspectSecurity, InspectSecurityEventPayload, InspectSecurityField,
    InspectSecurityOperation, InspectSecurityWebhook, InspectSessionCookie,
};

pub(super) fn inspect_security(
    lines: &[String],
    feature_name: &str,
    auth: Option<&lazuli_ir::Auth>,
) -> InspectSecurity {
    InspectSecurity {
        fields: inspect_security_fields(lines),
        event_payloads: inspect_security_event_payloads(lines),
        operations: inspect_security_operations(lines),
        webhooks: inspect_security_webhooks(lines),
        session_cookie: auth.and_then(|a| inspect_session_cookie(feature_name, a)),
    }
}

/// `cookie-sessions-child` — project the lowered `auth.sessions.cookie`
/// transport envelope onto the security axis. Returns `None` when the
/// feature declared no `auth.sessions`, or declared sessions without a
/// `cookie` child (the runtime then keeps its hardcoded cookie literals).
/// Each axis is echoed verbatim from the IR so the projection reads
/// exactly like the authored block; absent axes stay `None`.
pub(super) fn inspect_session_cookie(
    feature_name: &str,
    auth: &lazuli_ir::Auth,
) -> Option<InspectSessionCookie> {
    let sessions = auth.sessions.as_ref()?;
    let cookie = sessions.cookie.as_ref()?;
    Some(InspectSessionCookie {
        resource: sessions.resource.name.clone(),
        name: cookie.name.clone(),
        same_site: cookie.same_site.clone(),
        secure: cookie.secure,
        http_only: cookie.http_only,
        domain: cookie.domain.clone(),
        path: cookie.path.clone(),
        origin: InspectOrigin {
            feature: feature_name.to_owned(),
            line: cookie.span_ref.map(|span| span.start),
        },
    })
}

pub(super) fn inspect_security_fields(lines: &[String]) -> Vec<InspectSecurityField> {
    let mut fields = Vec::new();
    let mut current_resource: Option<String> = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("resource ") {
            current_resource = trimmed.split_whitespace().nth(1).map(str::to_owned);
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if !trimmed.starts_with("resource ") {
                current_resource = None;
            }
            continue;
        }

        if leading_spaces(line) == 6 {
            let Some(resource) = current_resource.as_deref() else {
                continue;
            };
            let Some(field) = super::field_name_from_typed_line(trimmed) else {
                continue;
            };
            let markers: Vec<_> = super::security_markers(line).collect();
            if markers.is_empty() {
                continue;
            }

            fields.push(InspectSecurityField {
                resource: resource.to_owned(),
                field: field.to_owned(),
                markers,
                origin: "field",
            });
        }
    }

    fields
}

pub(super) fn inspect_security_event_payloads(
    lines: &[String],
) -> Vec<InspectSecurityEventPayload> {
    let mut payloads = Vec::new();

    for event in collect_event_decls(lines) {
        for field_line in event.payload {
            let Some(field) = super::field_name_from_typed_line(&field_line) else {
                continue;
            };
            let markers: Vec<_> = super::security_markers(&field_line).collect();
            if markers.is_empty() {
                continue;
            }

            payloads.push(InspectSecurityEventPayload {
                event: event.name.clone(),
                field: field.to_owned(),
                markers,
                origin: "event",
            });
        }
    }

    payloads
}

pub(super) fn inspect_security_operations(lines: &[String]) -> Vec<InspectSecurityOperation> {
    let mut operations = Vec::new();

    for block in super::query_blocks(lines) {
        let name = super::query_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = super::direct_child_value(block, "policy ");
        let scope_reason = scope_override_reason(block);
        let scope_override = block
            .iter()
            .any(|line| line.trim_start().starts_with("scope override"));
        let rate_limits = super::direct_child_values(block, "rate_limit ");
        let audit = super::parse_audit(block, "query");

        if policy.is_some() || scope_override || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("query.{name}"),
                policy,
                tenant_from: None,
                scope_reason,
                rate_limits,
                scope_override,
                audit,
                origin: "query",
            });
        }
    }

    for block in super::command_blocks(lines) {
        let name = super::command_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = super::direct_child_value(block, "policy ");
        let rate_limits = super::direct_child_values(block, "rate_limit ");
        let audit = super::parse_audit(block, "command");

        if policy.is_some() || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("command.{name}"),
                policy,
                tenant_from: None,
                scope_reason: None,
                rate_limits,
                scope_override: false,
                audit,
                origin: "command",
            });
        }
    }

    for block in super::top_level_blocks(lines, "job ") {
        let name = super::named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = super::direct_child_value(block, "policy ");
        let tenant_from = super::direct_child_value(block, "tenant_from ");
        let rate_limits = super::direct_child_values(block, "rate_limit ");
        let audit = super::parse_audit(block, "job");

        if policy.is_some() || tenant_from.is_some() || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("job.{name}"),
                policy,
                tenant_from,
                scope_reason: None,
                rate_limits,
                scope_override: false,
                audit,
                origin: "job",
            });
        }
    }

    for block in super::top_level_blocks(lines, "webhook ") {
        let name = super::named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = super::direct_child_value(block, "policy ");
        let rate_limits = super::direct_child_values(block, "rate_limit ");
        let audit = super::parse_audit(block, "webhook");

        if policy.is_some() || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("webhook.{name}"),
                policy,
                tenant_from: None,
                scope_reason: None,
                rate_limits,
                scope_override: false,
                audit,
                origin: "webhook",
            });
        }
    }

    operations
}

pub(super) fn scope_override_reason(lines: &[String]) -> Option<String> {
    let mut in_scope_override = false;

    for line in lines {
        let trimmed = line.trim_start();
        let indent = leading_spaces(line);

        if indent == 6 && trimmed.starts_with("scope override") {
            in_scope_override = true;
            continue;
        }

        if in_scope_override && indent <= 6 && !trimmed.is_empty() {
            in_scope_override = false;
        }

        if in_scope_override && indent == 8 {
            if let Some(reason) = trimmed.strip_prefix("reason ") {
                return Some(reason.trim().to_owned());
            }
        }
    }

    None
}

pub(super) fn inspect_security_webhooks(lines: &[String]) -> Vec<InspectSecurityWebhook> {
    let mut webhooks = Vec::new();

    for block in super::top_level_blocks(lines, "webhook ") {
        let name = super::named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let verify =
            super::direct_child_value(block, "verify ").unwrap_or_else(|| "missing".to_owned());
        let secrets = block
            .iter()
            .filter_map(|line| {
                if leading_spaces(line) == 6 {
                    line.trim_start()
                        .strip_prefix("secret ")
                        .map(str::trim)
                        .map(str::to_owned)
                } else {
                    None
                }
            })
            .collect();

        webhooks.push(InspectSecurityWebhook {
            webhook: name.to_owned(),
            verify,
            secrets,
            origin: "webhook.verify",
        });
    }

    webhooks
}
