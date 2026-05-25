//! Event-driven envelope lowerings — `webhook`, `channel`,
//! `notification`, `mcp_server`, `event_group`, `tenant_migration`.
//!
//! ## Why these ship together
//!
//! Each block in this module is an envelope around the shared workflow
//! leaf catalog: `retry`, `tenant_from`, `idempotency`, `external_call`,
//! `emit_predicate`, `notification_digest`, `notification_throttle`,
//! `event_variant_field`, `mcp_*`. The envelopes differ in their
//! trigger surface (inbound HTTP webhook, outbound notification,
//! realtime channel, MCP server, declarative event group, cross-tenant
//! migration) but the body shapes converge on the same handful of
//! shared specs. Grouping them in one module makes the trigger-vs-spec
//! split auditable.
//!
//! ## Closed-catalog enforcement at lowering
//!
//! Two diagnostics fire here:
//! * `UnsupportedVerifyScheme` — `webhook verify <scheme>` outside
//!   `{hmac}` (Phase L Tier 3).
//! * `UnknownEnum` — `mcp_server transport <transport>` outside
//!   `{stdio, http_sse, http_streamable}` (MCP-TRANSPORT-001).
//!
//! Everything else is mechanical projection; doctor handles
//! cross-feature reference resolution downstream
//! (`CHANNEL-PAYLOAD-001`, `MCP-AUTH-001`, `TM-IDEMP-001`).
//!
//! ## What does NOT live here
//!
//! `lower_event_group`'s per-variant `EventField` builder
//! (`lower_event_variant_field`) lives in `workflow.rs` along with the
//! MCP leaf builders (`lower_mcp_tool`, `lower_mcp_resource`,
//! `lower_mcp_prompt`) and the workflow leaves the envelopes call into
//! (`lower_retry`, `lower_fanout`, `lower_job_trigger`,
//! `lower_external_call`, `lower_emit_predicate`,
//! `lower_notification_digest`, `lower_notification_throttle`,
//! `lower_tenant_migration_target`).

use crate::errors::AnalyzeError;
use crate::expr::{lower_path_string, lower_policy_atom, lower_policy_expr};
use crate::helpers::span_of;
use crate::types::type_ref_from_text;
use crate::workflow::{
    lower_emit_predicate, lower_event_variant_field, lower_job_trigger, lower_mcp_prompt,
    lower_mcp_resource, lower_mcp_tool, lower_notification_digest, lower_notification_throttle,
    lower_retry, lower_tenant_migration_target,
};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Phase L Tier 3 — lower a canonical-indent `webhook` block into
/// `ir::Webhook`. `verify: PathRef` falls back to a conventional path
/// derived from the webhook name (the legacy IR field is non-optional);
/// `structured_verify` carries the real structured spec lifted by
/// `parse_webhook_verify`.
pub fn lower_webhook(webhook: &syntax::Webhook) -> Result<ir::Webhook, AnalyzeError> {
    let structured_verify = Some(ir::VerifySpec {
        scheme: match webhook.verify.scheme.as_str() {
            "hmac" => ir::VerifyScheme::Hmac,
            other => {
                return Err(AnalyzeError::UnsupportedVerifyScheme {
                    scheme: other.to_owned(),
                });
            }
        },
        algorithm: webhook.verify.algorithm.clone(),
        secret_env: webhook
            .verify
            .secret_env
            .as_deref()
            .map(extract_env_binding)
            .unwrap_or_default(),
        header: webhook.verify.header.clone().unwrap_or_default(),
    });
    let tenant_from = webhook
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = webhook
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let policy = webhook
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));

    let (handler, returns) = match &webhook.handler {
        Some(h) => (
            ir::PathRef::authored(&h.path),
            h.returns.as_deref().map(|t| type_ref_from_text(t)),
        ),
        None => (
            ir::PathRef::convention(format!("./webhooks/{}.go", webhook.name)),
            None,
        ),
    };

    // Webhooks expanded cycle — typed payload reference (`payload from
    // webhook_events.<name>`). The parser stripped the catalog prefix
    // already, so the IR just keeps the suffix.
    let payload_from = webhook
        .payload_from
        .as_deref()
        .map(|name| ir::WebhookEventRef {
            name: name.to_owned(),
        });

    // `replay` short form (`replay allow within "..."`) and long form
    // (nested children) collapse onto the same `ReplaySpec`.
    let replay = webhook.replay.as_ref().map(|r| ir::ReplaySpec {
        mode: match r.mode.as_str() {
            "deny" => ir::ReplayMode::Deny,
            _ => ir::ReplayMode::Allow,
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_deref().map(lower_path_string),
    });

    // `dlq` discriminator (mutual exclusion enforced by the parser).
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        syntax::WebhookDlq::Emit { event, .. } => ir::DlqSpec::Emit {
            event: event.clone(),
        },
        syntax::WebhookDlq::Handler { path, .. } => ir::DlqSpec::Handler {
            path: ir::PathRef::authored(path),
        },
        syntax::WebhookDlq::Drop { reason, .. } => ir::DlqSpec::Drop {
            reason: reason.clone(),
        },
    });

    // Inbound retry shares the jobs `RetryPolicy` shape (Atrito #5).
    let retry = webhook.retry.as_ref().map(lower_retry);

    let policy_expr = webhook.policy_expr.as_ref().map(lower_policy_expr);
    let scope_global = webhook
        .scope_global
        .as_ref()
        .map(|sg| ir::WebhookScopeGlobalSpec {
            reason: sg.reason.clone(),
        });
    // B5 framework gap 2 — lift per-branch emit predicates onto the
    // typed `EmitPredicate` shape. The AST carries the raw `when`
    // clauses; we promote `path = "literal"` and
    // `path in ("a", "b")` to typed variants and fall back to
    // `EmitPredicateKind::Other { raw }` for anything else. Length
    // matches `webhook.emits` when any predicate is authored; an
    // empty vec means "flat list, no per-branch dispatch".
    let emit_predicates = if webhook.emits_predicates.is_empty() {
        Vec::new()
    } else {
        webhook
            .emits_predicates
            .iter()
            .map(|raw| raw.as_deref().map(lower_emit_predicate))
            .collect::<Vec<_>>()
    };

    Ok(ir::Webhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify: ir::PathRef::convention(format!("./webhooks/{}_verify.go", webhook.name)),
        structured_verify,
        tenant_from,
        scope_global,
        idempotency,
        policy,
        policy_expr,
        policy_when_denied: None,
        handler,
        returns,
        emits: webhook.emits.clone(),
        emit_predicates,
        payload_from,
        replay,
        dlq,
        retry,
        previous_names: Vec::new(),
        span_ref: Some(span_of(webhook.span)),
    })
}

/// Extract the env binding name from `env.<NAME>` (`secret env.X`).
fn extract_env_binding(raw: &str) -> String {
    raw.trim()
        .strip_prefix("env.")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| raw.trim().to_owned())
}

/// B5 framework gap 2 — lift a raw `when <predicate>` clause into the
/// typed `ir::EmitPredicate`. Recognised shapes:
///
/// * `path = "literal"` — equality.
/// * `path in ("a", "b")` — set membership.
/// * anything else — `EmitPredicateKind::Other { raw }`.
///
/// The lift is intentionally conservative: shapes that don't match
/// the typed catalog are preserved verbatim so codegen can emit a
/// runtime-evaluated stub without losing authoring intent.
/// Realtime bucket cycle MVP — lower a canonical-indent `channel`
/// block into `ir::Channel`. Mechanical projection: the parser
/// already enforces presence of all three required children, so the
/// lowering only wraps the verbatim strings into the typed shapes
/// (`TenantFromSpec`, `PolicyRef::Atom`, payload string verbatim).
/// Doctor `CHANNEL-PAYLOAD-001` resolves the payload reference
/// downstream.
pub fn lower_channel(channel: &syntax::Channel) -> ir::Channel {
    ir::Channel {
        name: channel.name.clone(),
        tenant_from: ir::TenantFromSpec {
            path: lower_path_string(&channel.tenant_from),
        },
        policy: lower_policy_atom(&channel.policy),
        policy_when_denied: None,
        payload: channel.payload.clone(),
        span_ref: Some(span_of(channel.span)),
    }
}

/// Phase L Tier 3 — lower a canonical-indent `notification` block into
/// `ir::Notification`. Reuses `JobTrigger`, `IdempotencyKey`,
/// `RetryPolicy`, `TenantFromSpec` from the job lowering helpers.
pub fn lower_notification(
    feature: &str,
    notification: &syntax::Notification,
) -> Result<ir::Notification, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &notification.trigger);
    let tenant_from = notification
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = notification
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = notification.retry.as_ref().map(lower_retry);
    let policy = notification
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    let digest = notification.digest.as_ref().map(lower_notification_digest);
    let throttle = notification
        .throttle
        .as_ref()
        .map(lower_notification_throttle);
    let policy_expr = notification.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Notification {
        name: notification.name.clone(),
        trigger,
        channels: notification.channels.clone(),
        recipient: notification.recipient.clone(),
        template: notification.template.clone(),
        policy,
        policy_expr,
        tenant_from,
        idempotency,
        retry,
        emits: notification.emits.clone(),
        digest,
        throttle,
        previous_names: Vec::new(),
        span_ref: Some(span_of(notification.span)),
    })
}

/// MCP bucket cycle — lower a canonical-indent `mcp_server` block into
/// `ir::MCPServerSpec`. Value-preserving except for the closed-catalog
/// `transport` mapping, which rejects unknown literals at lower-time.
pub fn lower_mcp_server(server: &syntax::McpServer) -> Result<ir::MCPServerSpec, AnalyzeError> {
    let transport = match server.transport.as_str() {
        "stdio" => ir::MCPTransport::Stdio,
        "http_sse" => ir::MCPTransport::HttpSse,
        "http_streamable" => ir::MCPTransport::HttpStreamable,
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("MCP-TRANSPORT-001 mcp_server `{}` transport", server.name),
                value: other.to_owned(),
            });
        }
    };
    let auth = server.auth.as_deref().and_then(parse_mcp_auth);
    let metadata = ir::MCPServerMetadata {
        name: server.metadata.name.clone(),
        description: server.metadata.description.clone(),
        version: server.metadata.version.clone(),
    };
    let tools = server.tools.iter().map(lower_mcp_tool).collect::<Vec<_>>();
    let resources = server
        .resources
        .iter()
        .map(lower_mcp_resource)
        .collect::<Vec<_>>();
    let prompts = server
        .prompts
        .iter()
        .map(lower_mcp_prompt)
        .collect::<Vec<_>>();
    Ok(ir::MCPServerSpec {
        name: server.name.clone(),
        transport,
        scope_feature: server.scope_feature.clone(),
        auth,
        metadata,
        tools,
        resources,
        prompts,
        span_ref: Some(span_of(server.span)),
    })
}

/// Parse `bearer env.<NAME>` into `ir::MCPAuth::BearerEnvVar`. Anything
/// else (future `oauth ...`, malformed line) returns `None`; doctor
/// `MCP-AUTH-001` (registered in proposal) catches malformed shapes.
fn parse_mcp_auth(raw: &str) -> Option<ir::MCPAuth> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("bearer env.") {
        let env = rest.trim().to_owned();
        if env.is_empty() {
            return None;
        }
        return Some(ir::MCPAuth::BearerEnvVar { env });
    }
    None
}

/// Phase L Tier 3 — lower a canonical-indent `event_group` into
/// `ir::EventGroup`. The payload bag and authored events stay as raw
/// strings; B5 framework gap 1 lifts the per-event typed payload
/// blocks into `variants`.
pub fn lower_event_group(group: &syntax::EventGroup) -> ir::EventGroup {
    // EVENT-OUTBOX §3.3 — lower the parallel bool vec into the typed
    // `OutboxMode` catalog. Index-paired with `events`; when the AST
    // emits an empty vec (legacy / pre-outbox payloads) we expand to
    // a same-length `None` vec so downstream code can read by index.
    let events_outbox: Vec<ir::OutboxMode> = if group.events_outbox_guaranteed.is_empty() {
        vec![ir::OutboxMode::None; group.events.len()]
    } else {
        group
            .events_outbox_guaranteed
            .iter()
            .map(|g| {
                if *g {
                    ir::OutboxMode::Guaranteed
                } else {
                    ir::OutboxMode::None
                }
            })
            .collect()
    };

    // B5 framework gap 1 — lift per-event field bodies into
    // `EventVariant` records. Each variant carries its `EventField`s
    // lifted via `type_ref_from_syntax`, the closed kind catalog
    // (committed vs trace), and the outbox flag mirrored from the
    // parallel slot above. Back-compat: variants whose body was
    // empty come through with an empty `fields` Vec; legacy fixtures
    // that didn't author `event_variants`/`event_variant_kinds` at
    // all leave `variants` empty.
    let variants: Vec<ir::EventVariant> =
        if group.event_variants.is_empty() && group.event_variant_kinds.is_empty() {
            Vec::new()
        } else {
            group
                .events
                .iter()
                .enumerate()
                .map(|(idx, short_name)| {
                    let kind = match group
                        .event_variant_kinds
                        .get(idx)
                        .copied()
                        .unwrap_or(syntax::EventVariantKindAst::Committed)
                    {
                        syntax::EventVariantKindAst::Committed => ir::EventVariantKind::Committed,
                        syntax::EventVariantKindAst::Trace => ir::EventVariantKind::Trace,
                    };
                    let fields = group
                        .event_variants
                        .get(idx)
                        .map(|rows| {
                            rows.iter()
                                .map(lower_event_variant_field)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let outbox = events_outbox
                        .get(idx)
                        .copied()
                        .unwrap_or(ir::OutboxMode::None);
                    ir::EventVariant {
                        name: short_name.clone(),
                        kind,
                        outbox,
                        fields,
                        span_ref: group
                            .event_variants
                            .get(idx)
                            .and_then(|rows| rows.first().map(|f| span_of(f.span))),
                    }
                })
                .collect()
        };

    ir::EventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        raw_payload: group.payload.clone(),
        raw_audit: group.audit.clone(),
        events: group.events.clone(),
        events_outbox,
        variants,
        span_ref: Some(span_of(group.span)),
    }
}

/// Migrations bucket cycle Route C — lower a canonical-indent
/// `tenant_migration` block into `ir::TenantMigration`. Mirrors
/// `lower_job` for the shared spine (idempotency / retry / timeout /
/// handler) and adds the `target tenants <axis>` slot. The lowering
/// does **not** enforce that `idempotency` is authored; that is
/// `TM-IDEMP-001`'s job downstream.
pub fn lower_tenant_migration(
    tm: &syntax::TenantMigration,
) -> Result<ir::TenantMigration, AnalyzeError> {
    let idempotency = tm
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path })
        .unwrap_or_else(|| ir::IdempotencyKey {
            by: ir::Path::from_segments(Vec::<String>::new()),
        });
    let retry = tm.retry.as_ref().map(lower_retry);
    Ok(ir::TenantMigration {
        name: tm.name.clone(),
        target: ir::TenantMigrationTarget {
            operation: tm.target_ref.as_deref().map(lower_tenant_migration_target),
            axis: tm.target_axis.clone(),
        },
        idempotency,
        retry,
        timeout: tm.timeout.clone(),
        handler: ir::PathRef::authored(&tm.handler),
        previous_names: Vec::new(),
        span_ref: Some(span_of(tm.span)),
    })
}
