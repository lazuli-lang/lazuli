//! Diagnostics for inbound trust boundaries — `webhook <name>` and
//! `escape_route <name>` declarations.
//!
//! Both primitives sit outside generated UI ownership and require an
//! explicit security stance. Two related concerns get their own
//! producers:
//!
//! | Producer | Concern |
//! |---|---|
//! | [`webhook_security_diagnostics`] | webhooks must declare `verify ...` (or explicit `verify none` with a `reason`) and `idempotency by payload.<key>`. |
//! | [`webhook_tenant_from_diagnostics`] | webhooks inside a tenant-scoped feature must declare `tenant_from payload.<axis>_id` or explicit `scope global`. |
//! | [`escape_route_security_diagnostics`] | every `escape_route` block must declare `policy` and `tenant`. |
//!
//! Facts-producer pairs follow the two-pass shape used elsewhere in this
//! subtree: the walker emits one `*Facts` per block; the facts producer
//! turns it into diagnostics.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    collect_feature_tenant_axes, feature_name, leading_spaces, simple_canonical_diagnostic,
};

#[derive(Debug)]
pub(crate) struct WebhookSecurityFacts {
    line_index: usize,
    line: String,
    has_verify: bool,
    verify_none: Option<(usize, String)>,
    verify_none_has_reason: bool,
    has_idempotency: bool,
}

pub(crate) fn webhook_security_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_webhook: Option<WebhookSecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_diagnostics(webhook));
            }
            current_webhook = Some(WebhookSecurityFacts {
                line_index,
                line: line.to_owned(),
                has_verify: false,
                verify_none: None,
                verify_none_has_reason: false,
                has_idempotency: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_diagnostics(webhook));
            }
            continue;
        }

        let Some(webhook) = current_webhook.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed == "verify none" {
                webhook.has_verify = true;
                webhook.verify_none = Some((line_index, line.to_owned()));
            } else if trimmed.starts_with("verify ") {
                webhook.has_verify = true;
            } else if trimmed.starts_with("idempotency by ") {
                webhook.has_idempotency = true;
            }
        } else if leading_spaces(line) == 6
            && webhook.verify_none.is_some()
            && trimmed.starts_with("reason ")
        {
            webhook.verify_none_has_reason = true;
        }
    }

    if let Some(webhook) = current_webhook {
        diagnostics.extend(webhook_diagnostics(webhook));
    }

    diagnostics
}

pub(crate) fn webhook_diagnostics(webhook: WebhookSecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !webhook.has_verify {
        diagnostics.push(simple_canonical_diagnostic(
            webhook.line_index,
            &webhook.line,
            DiagnosticSeverity::WARNING,
            "webhook-verify",
            "webhooks are inbound trust boundaries and must declare `verify ...` or explicit `verify none` with a `reason` child.",
        ));
    }

    if !webhook.has_idempotency {
        diagnostics.push(simple_canonical_diagnostic(
            webhook.line_index,
            &webhook.line,
            DiagnosticSeverity::WARNING,
            "webhook-idempotency",
            "webhooks must declare `idempotency by payload.<business_key>` so verified inbound deliveries cannot be replayed silently.",
        ));
    }

    if let Some((line_index, line)) = webhook.verify_none {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "security-opt-out",
            "`verify none` is an explicit security opt-out. Strict profile allows it for reviewed drafts; production profile treats it as a release blocker.",
        ));

        if !webhook.verify_none_has_reason {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "security-opt-out-reason",
                "`verify none` must include a `reason \"...\"` child.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct WebhookTenantFacts {
    feature: String,
    line_index: usize,
    line: String,
    has_tenant_from: bool,
    has_global_scope: bool,
}

pub(crate) fn webhook_tenant_from_diagnostics(source: &str) -> Vec<Diagnostic> {
    let tenant_axes = collect_feature_tenant_axes(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_webhook: Option<WebhookTenantFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
            }
            current_webhook = current_feature.as_ref().map(|feature| WebhookTenantFacts {
                feature: feature.clone(),
                line_index,
                line: line.to_owned(),
                has_tenant_from: false,
                has_global_scope: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(webhook) = current_webhook.take() {
                diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
            }
            continue;
        }

        let Some(webhook) = current_webhook.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("tenant_from ") {
                webhook.has_tenant_from = true;
            } else if trimmed.starts_with("scope global") {
                webhook.has_global_scope = true;
            }
        }
    }

    if let Some(webhook) = current_webhook {
        diagnostics.extend(webhook_tenant_from_facts_diagnostics(webhook, &tenant_axes));
    }

    diagnostics
}

pub(crate) fn webhook_tenant_from_facts_diagnostics(
    webhook: WebhookTenantFacts,
    tenant_axes: &HashMap<String, HashSet<String>>,
) -> Vec<Diagnostic> {
    if webhook.has_tenant_from || webhook.has_global_scope {
        return Vec::new();
    }

    let Some(axes) = tenant_axes
        .get(&webhook.feature)
        .filter(|axes| !axes.is_empty())
    else {
        return Vec::new();
    };
    let mut axes: Vec<_> = axes.iter().cloned().collect();
    axes.sort();
    let payload_hints: Vec<_> = axes
        .iter()
        .map(|axis| format!("`tenant_from payload.{axis}_id`"))
        .collect();

    vec![simple_canonical_diagnostic(
        webhook.line_index,
        &webhook.line,
        DiagnosticSeverity::WARNING,
        "webhook-tenant-from",
        &format!(
            "webhook in tenant-scoped feature `{}` should declare {} or explicit `scope global` with a reason.",
            webhook.feature,
            payload_hints.join(" or ")
        ),
    )]
}

#[derive(Debug)]
pub(crate) struct EscapeRouteSecurityFacts {
    line_index: usize,
    line: String,
    has_policy: bool,
    has_tenant: bool,
}

pub(crate) fn escape_route_security_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_escape_route: Option<EscapeRouteSecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 && trimmed.starts_with("escape_route ") {
            if let Some(route) = current_escape_route.take() {
                diagnostics.extend(escape_route_diagnostics(route));
            }
            current_escape_route = Some(EscapeRouteSecurityFacts {
                line_index,
                line: line.to_owned(),
                has_policy: false,
                has_tenant: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(route) = current_escape_route.take() {
                diagnostics.extend(escape_route_diagnostics(route));
            }
            continue;
        }

        let Some(route) = current_escape_route.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("policy ") {
                route.has_policy = true;
            } else if trimmed.starts_with("tenant ") {
                route.has_tenant = true;
            }
        }
    }

    if let Some(route) = current_escape_route {
        diagnostics.extend(escape_route_diagnostics(route));
    }

    diagnostics
}

pub(crate) fn escape_route_diagnostics(route: EscapeRouteSecurityFacts) -> Vec<Diagnostic> {
    let mut missing = Vec::new();
    if !route.has_policy {
        missing.push("policy");
    }
    if !route.has_tenant {
        missing.push("tenant");
    }

    if missing.is_empty() {
        Vec::new()
    } else {
        vec![simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::WARNING,
            "escape-route-security",
            &format!(
                "`escape_route` is outside generated UI ownership and must declare {}.",
                missing.join(" and ")
            ),
        )]
    }
}
