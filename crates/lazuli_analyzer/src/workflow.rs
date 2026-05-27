//! Async-work lowering — jobs, tenant migrations, MCP, notifications,
//! event groups.
//!
//! ## Why this slot exists
//!
//! Lazuli's async-work surface has half a dozen sibling primitives
//! (`job`, `poller`, `webhook`, `tenant_migration`, `channel`,
//! `notification`, `mcp_server`, `event_group`) that share a common
//! spine: retry, idempotency, fanout, external-call refs, emit
//! predicates. Pulling the leaf-builders into a single sibling module
//! keeps the analyzer's top-level `lib.rs` focused on the per-feature
//! orchestrator and away from the strut-work that each individual
//! primitive shares with its neighbors.
//!
//! The orchestrators (`lower_job`, `lower_poller`, `lower_webhook`,
//! `lower_channel`, `lower_notification`, `lower_mcp_server`,
//! `lower_event_group`, `lower_tenant_migration`) stay in `lib.rs` for
//! now — the next round can pull each one into here once their
//! per-primitive helpers no longer leak across slices. Today the
//! orchestrators reach into too many sibling helpers to migrate in a
//! single pass.
//!
//! ## What lives here
//!
//! * `lower_emit_predicate` (+ `parse_emit_predicate_kind`) — closed
//!   catalog of `path = "literal"` / `path in ("a", "b", ...)` plus
//!   the verbatim `Other { raw }` carve-out.
//! * `lower_retry` — `count` + closed `BackoffStrategy` (Fixed |
//!   Exponential).
//! * `lower_fanout` — `tenants <axis>` scope projection.
//! * `lower_external_call` — `<slot>.<op>(args)` callout into the
//!   typed `ExternalCallRef`.
//! * `lower_job_body` — discriminate handler-style vs declarative job
//!   body (reuses `lower_command_effect`).
//! * `lower_job_trigger` (+ `qualified_event_name`) — event-name vs
//!   cron trigger discriminator.
//! * `lower_tenant_migration_target` — `query.<name>` /
//!   `command.<name>` / `<feature>.query.<name>` /
//!   `<feature>.command.<name>` discriminator.
//! * `lower_event_variant_field` — typed payload field row for B5
//!   event-group variants. Reuses `type_ref_from_syntax`.
//! * `lower_mcp_tool` / `_resource` / `_prompt` / `_param` — MCP
//!   server surface leaf lowerings.
//! * `lower_notification_digest` / `_throttle` — typed projection of
//!   the notification digest / throttle slots; closed-catalog
//!   `DigestStrategy` (Merge | Append) with `invalid_*` carve-outs.
//!
//! ## What does NOT live here
//!
//! The orchestrators themselves (`lower_job`, `lower_poller`,
//! `lower_webhook`, `lower_channel`, `lower_notification`,
//! `lower_mcp_server`, `lower_event_group`, `lower_tenant_migration`)
//! stay in `lib.rs` until their cross-cutting helper dependencies
//! are also pulled into siblings. The boundary is leaf vs envelope.
//!
//! Source AST shapes: `lazuli_syntax::{JobBody, JobTrigger, JobRetry,
//! JobFanout, JobExternalCall, McpTool, McpResource, McpPrompt,
//! McpParam, NotificationDigest, NotificationThrottle,
//! EventVariantFieldDecl}`. Destination IR shapes:
//! `lazuli_ir::{EmitPredicate, RetryPolicy, FanoutSpec,
//! ExternalCallRef, JobBody, JobTrigger, MCPTool, MCPResource,
//! MCPPrompt, MCPParam, NotificationDigest, NotificationThrottle,
//! EventField, TenantMigrationTargetOperation}`.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::command::{lower_command_effect, lower_let_binding, lower_target_expr};
use crate::expr::lower_raw_expr;
use crate::helpers::{find_word, span_of, strip_quotes};
use crate::{type_ref_from_syntax, type_ref_from_text};

/// EVENT-EMIT-PREDICATE bucket cycle — lift the raw `when` clause of
/// an `emit` line into the typed `EmitPredicate`. The closed catalog:
///
/// * `path = "literal"` — `EmitPredicateKind::Equals`.
/// * `path in ("a", "b", ...)` — `EmitPredicateKind::In`.
/// * anything else — `EmitPredicateKind::Other { raw }`.
///
/// The lift is intentionally conservative: shapes that don't match
/// the typed catalog are preserved verbatim so codegen can emit a
/// runtime-evaluated stub without losing authoring intent.
pub(crate) fn lower_emit_predicate(raw: &str) -> ir::EmitPredicate {
    let trimmed = raw.trim();
    let kind = parse_emit_predicate_kind(trimmed).unwrap_or_else(|| ir::EmitPredicateKind::Other {
        raw: trimmed.to_owned(),
    });
    ir::EmitPredicate {
        raw: trimmed.to_owned(),
        kind,
        span_ref: None,
    }
}

fn parse_emit_predicate_kind(text: &str) -> Option<ir::EmitPredicateKind> {
    // `path = "literal"` — split on the first `=` not followed by `=`
    // (avoid `==` if a future surface accepts it). The current closed
    // surface only authors a single `=`.
    if let Some((lhs, rhs)) = text.split_once('=') {
        let path = lhs.trim();
        let literal_raw = rhs.trim();
        if !path.is_empty() && !path.contains(' ') {
            if let Some(literal) = strip_quotes(literal_raw) {
                return Some(ir::EmitPredicateKind::Equals {
                    path: path.to_owned(),
                    literal: literal.to_owned(),
                });
            }
        }
    }
    // `path in ("a", "b", ...)`
    if let Some(in_pos) = find_word(text, "in") {
        let path = text[..in_pos].trim();
        let rhs = text[in_pos + 2..].trim();
        if !path.is_empty() && !path.contains(' ') && rhs.starts_with('(') && rhs.ends_with(')') {
            let inner = &rhs[1..rhs.len() - 1];
            let literals: Vec<String> = inner
                .split(',')
                .filter_map(|raw| strip_quotes(raw.trim()).map(str::to_owned))
                .collect();
            if !literals.is_empty() {
                return Some(ir::EmitPredicateKind::In {
                    path: path.to_owned(),
                    literals,
                });
            }
        }
    }
    None
}

pub(crate) fn lower_mcp_tool(tool: &syntax::McpTool) -> ir::MCPTool {
    let params = tool.params.iter().map(lower_mcp_param).collect();
    ir::MCPTool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        params,
        returns_kind: tool.returns.clone(),
        handler_fn: tool.handler.clone(),
        policy: tool.policy.clone(),
        span_ref: Some(span_of(tool.span)),
    }
}

pub(crate) fn lower_mcp_resource(resource: &syntax::McpResource) -> ir::MCPResource {
    ir::MCPResource {
        name: resource.name.clone(),
        uri_template: resource.uri_template.clone(),
        mime: resource.mime.clone(),
        handler_fn: resource.handler.clone(),
        policy: resource.policy.clone(),
        span_ref: Some(span_of(resource.span)),
    }
}

pub(crate) fn lower_mcp_prompt(prompt: &syntax::McpPrompt) -> ir::MCPPrompt {
    let params = prompt.params.iter().map(lower_mcp_param).collect();
    ir::MCPPrompt {
        name: prompt.name.clone(),
        description: prompt.description.clone(),
        params,
        template_path: prompt.template.clone(),
        span_ref: Some(span_of(prompt.span)),
    }
}

pub(crate) fn lower_mcp_param(param: &syntax::McpParam) -> ir::MCPParam {
    ir::MCPParam {
        name: param.name.clone(),
        ty_literal: param.ty.clone(),
        required: param.required,
    }
}

/// Notifications expanded bucket cycle — lower AST `NotificationDigest`
/// into the typed IR. `template_strategy` falls through `merge` /
/// `append` into the closed-catalog enum; unknown values are preserved
/// in `invalid_template_strategy` so doctor can report
/// `NOTIF-DIGEST-003` without widening the enum.
pub(crate) fn lower_notification_digest(
    digest: &syntax::NotificationDigest,
) -> ir::NotificationDigest {
    let (template_strategy, invalid_template_strategy) = match digest.template_strategy.as_deref() {
        Some("merge") => (Some(ir::DigestStrategy::Merge), None),
        Some("append") => (Some(ir::DigestStrategy::Append), None),
        Some(raw) => (None, Some(raw.to_owned())),
        None => (None, None),
    };
    ir::NotificationDigest {
        every: digest.every.clone(),
        group_by: digest.group_by.clone(),
        max_size: digest.max_size,
        template_strategy,
        invalid_template_strategy,
    }
}

/// Notifications expanded bucket cycle — lower AST
/// `NotificationThrottle` into the typed IR. Pure field-for-field
/// projection; no validation here (doctor `NOTIF-THROTTLE-*` covers
/// the closed-catalog and combinatorial rules).
pub(crate) fn lower_notification_throttle(
    throttle: &syntax::NotificationThrottle,
) -> ir::NotificationThrottle {
    ir::NotificationThrottle {
        max_per: throttle.max_per.clone(),
        per_recipient: throttle.per_recipient,
        per_channel: throttle.per_channel,
        burst: throttle.burst,
    }
}

/// B5 framework gap 1 — lift one typed event-variant field row into
/// `ir::EventField`. Reuses `type_ref_from_syntax` so `@semantic.X`,
/// `@cap.X`, and built-in scalars all flow through the same lifter
/// resource fields use. `optional` falls back to `!required` when
/// neither modifier was authored — matches the resource-field
/// convention.
pub(crate) fn lower_event_variant_field(decl: &syntax::EventVariantFieldDecl) -> ir::EventField {
    let optional = if decl.required {
        false
    } else {
        // Treat unmarked event-variant fields as required by default
        // (events are projection contracts; missing values are a
        // codegen-time bug). Authors opt into optionality explicitly.
        decl.optional
    };
    ir::EventField {
        name: decl.name.clone(),
        type_ref: type_ref_from_syntax(&decl.type_text),
        optional,
    }
}

pub(crate) fn lower_tenant_migration_target(raw: &str) -> ir::TenantMigrationTargetOperation {
    let parts: Vec<&str> = raw.split('.').collect();
    match parts.as_slice() {
        ["query", name] => ir::TenantMigrationTargetOperation::Query {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "query", name] => ir::TenantMigrationTargetOperation::Query {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        ["command", name] => ir::TenantMigrationTargetOperation::Command {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "command", name] => ir::TenantMigrationTargetOperation::Command {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        _ => ir::TenantMigrationTargetOperation::Query {
            feature: None,
            name: raw.to_owned(),
        },
    }
}

pub(crate) fn lower_job_trigger(feature: &str, trigger: &syntax::JobTrigger) -> ir::JobTrigger {
    match trigger {
        syntax::JobTrigger::Event(name) => ir::JobTrigger::Event {
            event: qualified_event_name(feature, name),
        },
        syntax::JobTrigger::Schedule(cron) => ir::JobTrigger::Schedule { cron: cron.clone() },
    }
}

fn qualified_event_name(feature: &str, name: &str) -> ir::QualifiedName {
    if let Some((ns, ev)) = name.split_once('.') {
        ir::QualifiedName {
            feature: Some(ns.to_owned()),
            name: ev.to_owned(),
        }
    } else {
        ir::QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    }
}

pub(crate) fn lower_retry(retry: &syntax::JobRetry) -> ir::RetryPolicy {
    ir::RetryPolicy {
        count: retry.count,
        backoff: match retry.backoff.as_str() {
            "exponential" => ir::BackoffStrategy::Exponential,
            _ => ir::BackoffStrategy::Fixed,
        },
    }
}

pub(crate) fn lower_fanout(fanout: &syntax::JobFanout) -> ir::FanoutSpec {
    ir::FanoutSpec {
        scope: ir::FanoutScope::Tenants,
        axis: fanout.axis.clone(),
    }
}

pub(crate) fn lower_external_call(call: &syntax::JobExternalCall) -> ir::ExternalCallRef {
    ir::ExternalCallRef {
        slot: call.slot.clone(),
        op: call.op.clone(),
        args: call
            .args
            .iter()
            .map(|arg| ir::NamedArg {
                name: arg.name.clone(),
                value: lower_raw_expr(&arg.value),
            })
            .collect(),
        span_ref: Some(span_of(call.span)),
    }
}

pub(crate) fn lower_job_body(body: &syntax::JobBody) -> ir::JobBody {
    match body {
        syntax::JobBody::Handler(h) => ir::JobBody::Handler(ir::JobHandler {
            path: ir::PathRef::authored(&h.path),
            returns: h.returns.as_deref().map(|t| type_ref_from_text(t)),
        }),
        syntax::JobBody::Declarative(d) => ir::JobBody::Declarative(ir::JobDeclarative {
            target: d.target.as_ref().map(lower_target_expr),
            lets: d.lets.iter().map(lower_let_binding).collect(),
            effect: d
                .effect
                .as_ref()
                .map(lower_command_effect)
                .unwrap_or(ir::CommandEffect::None),
        }),
        syntax::JobBody::None => ir::JobBody::Declarative(ir::JobDeclarative {
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::None,
        }),
    }
}
