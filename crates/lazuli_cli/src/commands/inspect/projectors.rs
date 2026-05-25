//! IR-to-inspect projectors for typed top-level constructs.
//!
//! Each function here takes a lifted `lazuli_ir::*` value (Job,
//! Webhook, EventGroup, Aggregate, Invariant) and produces the
//! matching `Inspect<X>` carrier the inspect report serializes.
//! The projectors are pure — no source-text walking, no IO — so
//! they're cheap to call from any axis that joins them onto a
//! feature.
//!
//! Why a separate module: these are the bridge between the typed
//! IR (lifted in `inspect_canonical_source_with_aliases`) and the
//! JSON-stable carriers in `report_types.rs`. Keeping them out of
//! `mod.rs` lets the canonical-source orchestrator focus on
//! aggregating projector outputs rather than expressing the
//! per-construct field mapping.

use super::{
    InspectAggregate, InspectEventGroup, InspectInvariant, InspectJob, InspectJobBody,
    InspectJobDeclarative, InspectJobExternalCall, InspectJobFanout, InspectJobHandler,
    InspectJobRetry, InspectJobTrigger, InspectWebhook, InspectWebhookDlq,
    InspectWebhookPayloadFrom, InspectWebhookReplay, InspectWebhookRetry, InspectWebhookVerify,
};
use super::formatters::{
    compare_op_to_string, format_qname, inspect_command_effect_to_string, op_as_str,
    path_to_string, policy_ref_to_string, predicate_to_string, tool_ref_to_string,
};

pub(super) fn project_job(job: &lazuli_ir::Job) -> InspectJob {
    let operational_kind: &'static str = match (&job.trigger, &job.queue) {
        (lazuli_ir::JobTrigger::Schedule { .. }, _) => "scheduled",
        (lazuli_ir::JobTrigger::Event { .. }, Some(_)) => "queued_worker",
        (lazuli_ir::JobTrigger::Event { .. }, None) => "reactor",
    };
    let trigger = match &job.trigger {
        lazuli_ir::JobTrigger::Event { event } => InspectJobTrigger::Event(format_qname(event)),
        lazuli_ir::JobTrigger::Schedule { cron } => InspectJobTrigger::Schedule(cron.clone()),
    };
    InspectJob {
        name: job.name.clone(),
        operational_kind,
        trigger,
        queue: job.queue.clone(),
        idempotency_by: job.idempotency.as_ref().map(|i| path_to_string(&i.by)),
        retry: job.retry.as_ref().map(|r| InspectJobRetry {
            count: r.count,
            backoff: match r.backoff {
                lazuli_ir::BackoffStrategy::Exponential => "exponential",
                lazuli_ir::BackoffStrategy::Fixed => "fixed",
            },
        }),
        policy: job.policy.as_ref().map(policy_ref_to_string),
        tenant_from: job.tenant_from.as_ref().map(|t| path_to_string(&t.path)),
        fanout: job.fanout.as_ref().map(|f| InspectJobFanout {
            scope: match f.scope {
                lazuli_ir::FanoutScope::Tenants => "tenants",
            },
            axis: f.axis.clone(),
        }),
        timeout: job.timeout.clone(),
        external_calls: job
            .external_calls
            .iter()
            .map(|c| InspectJobExternalCall {
                slot: c.slot.clone(),
                op: c.op.clone(),
                args: c.args.iter().map(|a| a.name.clone()).collect(),
            })
            .collect(),
        body: match &job.body {
            lazuli_ir::JobBody::Handler(h) => InspectJobBody::Handler(InspectJobHandler {
                path: h.path.path.clone(),
                returns: h.returns.as_ref().map(type_ref_to_string),
            }),
            lazuli_ir::JobBody::Declarative(d) => {
                let target_text = d.target.as_ref().map(inspect_target_expr_to_string);
                let lets_text: Vec<String> =
                    d.lets.iter().map(inspect_let_binding_to_string).collect();
                let effect_text = match &d.effect {
                    lazuli_ir::CommandEffect::None => None,
                    other => Some(inspect_command_effect_to_string(other)),
                };
                if target_text.is_none() && lets_text.is_empty() && effect_text.is_none() {
                    InspectJobBody::None
                } else {
                    InspectJobBody::Declarative(InspectJobDeclarative {
                        target: target_text,
                        lets: lets_text,
                        effect: effect_text,
                    })
                }
            }
        },
        emits: job.emits.clone(),
        origin: "job",
    }
}

pub(super) fn project_webhook(webhook: &lazuli_ir::Webhook) -> InspectWebhook {
    let verify = match &webhook.structured_verify {
        Some(v) => InspectWebhookVerify {
            scheme: match v.scheme {
                lazuli_ir::VerifyScheme::Hmac => "hmac",
            },
            algorithm: v.algorithm.clone(),
            secret_env: v.secret_env.clone(),
            header: v.header.clone(),
        },
        None => InspectWebhookVerify {
            scheme: "hmac",
            algorithm: String::new(),
            secret_env: String::new(),
            header: String::new(),
        },
    };
    // Webhooks expanded cycle — typed projections for the four new
    // children. Each Option<…> is skipped when absent so consumers
    // that lived through Tier 3 see no churn.
    let payload_from = webhook
        .payload_from
        .as_ref()
        .map(|r| InspectWebhookPayloadFrom {
            name: r.name.clone(),
            path: format!("webhook_events.{}", r.name),
        });
    let replay = webhook.replay.as_ref().map(|r| InspectWebhookReplay {
        mode: match r.mode {
            lazuli_ir::ReplayMode::Allow => "allow",
            lazuli_ir::ReplayMode::Deny => "deny",
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_ref().map(path_to_string),
    });
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        lazuli_ir::DlqSpec::Emit { event } => InspectWebhookDlq::Emit {
            event: event.clone(),
        },
        lazuli_ir::DlqSpec::Handler { path } => InspectWebhookDlq::Handler {
            path: path.path.clone(),
        },
        lazuli_ir::DlqSpec::Drop { reason } => InspectWebhookDlq::Drop {
            reason: reason.clone(),
        },
    });
    let retry = webhook.retry.as_ref().map(|r| InspectWebhookRetry {
        count: r.count,
        backoff: match r.backoff {
            lazuli_ir::BackoffStrategy::Fixed => "fixed",
            lazuli_ir::BackoffStrategy::Exponential => "exponential",
        },
    });
    InspectWebhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify,
        tenant_from: webhook
            .tenant_from
            .as_ref()
            .map(|t| path_to_string(&t.path)),
        idempotency_by: webhook.idempotency.as_ref().map(|i| path_to_string(&i.by)),
        policy: webhook.policy.as_ref().map(policy_ref_to_string),
        handler: webhook.handler.path.clone(),
        returns: webhook.returns.as_ref().map(type_ref_to_string),
        emits: webhook.emits.clone(),
        payload_from,
        replay,
        dlq,
        retry,
        origin: "webhook",
    }
}

pub(super) fn project_event_group(group: &lazuli_ir::EventGroup) -> InspectEventGroup {
    InspectEventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        payload: group.raw_payload.clone(),
        audit: group.raw_audit.clone(),
        events: group.events.clone(),
        origin: "event_group",
    }
}

// CL.C.4 — project an `ir::Aggregate` into the inspect view.
pub(super) fn project_aggregate(agg: &lazuli_ir::Aggregate) -> InspectAggregate {
    InspectAggregate {
        name: agg.name.clone(),
        root: format_qname(&agg.root),
        contains: agg.contains.iter().map(format_qname).collect(),
        invariants: agg.invariants.iter().map(project_invariant).collect(),
        origin: "aggregate",
    }
}

pub(super) fn project_invariant(inv: &lazuli_ir::Invariant) -> InspectInvariant {
    let (when, when_kind): (String, &'static str) = match &inv.when {
        lazuli_ir::EvalPredicate::Closed(pred) => {
            (predicate_to_string(pred), "closed")
        }
        lazuli_ir::EvalPredicate::Contains { lhs, rhs } => {
            let rhs_str = match rhs {
                lazuli_ir::EvalContainsRhs::Literal(t) => format!("\"{t}\""),
                lazuli_ir::EvalContainsRhs::SemanticType(q) => format_qname(q),
            };
            (
                format!("{} contains {}", path_to_string(lhs), rhs_str),
                "contains",
            )
        }
        lazuli_ir::EvalPredicate::ToolsCalls { op, target } => (
            format!("tools.calls {} {}", op_as_str(op), tool_ref_to_string(target)),
            "tools_calls",
        ),
        lazuli_ir::EvalPredicate::Unparsed(text) => (text.clone(), "unparsed"),
    };
    InspectInvariant {
        name: inv.name.clone(),
        when,
        when_kind,
        message: inv.message.clone(),
    }
}
