//! Per-IR-section walkers used by the feature dispatcher.
//!
//! The dispatcher in `super::collect_feature_handler_refs` calls one
//! `walk_*` per major IR section so the file structure mirrors the IR
//! taxonomy (jobs, webhooks, notifications, event_groups, auth,
//! extensions, agents). Each helper is intentionally short — the
//! interesting logic lives in `refs.rs` / `commands.rs`; these walkers
//! only carry the site labels.

use std::collections::BTreeMap;

use lazuli_ir::{EvalContainsRhs, EvalPredicate, ExtensionContract, Feature, JobBody};

use super::super::{HandlerNamespace, HandlerStub, SignatureMap, StubKey};
use super::commands::collect_command_effect_refs;
use super::refs::{
    collect_expr_refs, collect_job_trigger_refs, collect_optional_text_handler_ref,
    collect_path_refs, collect_policy_ref, collect_predicate_refs, collect_text_handler_refs,
    collect_type_ref, push_handler_ref,
};

pub(super) fn walk_jobs(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    for job in &feature.jobs {
        let site = format!("job.{}", job.name);
        collect_job_trigger_refs(&job.trigger, feature_name, &site, signatures, stubs);
        collect_policy_ref(&job.policy, feature_name, &site, signatures, stubs);
        if let Some(tenant_from) = &job.tenant_from {
            collect_path_refs(
                &tenant_from.path,
                feature_name,
                &format!("job.{}.tenant_from", job.name),
                signatures,
                stubs,
            );
        }
        if let Some(idempotency) = &job.idempotency {
            collect_path_refs(
                &idempotency.by,
                feature_name,
                &format!("job.{}.idempotency", job.name),
                signatures,
                stubs,
            );
        }
        for call in &job.external_calls {
            for arg in &call.args {
                collect_expr_refs(
                    &arg.value,
                    feature_name,
                    &format!("job.{}.calls.{}.{}", job.name, call.slot, arg.name),
                    signatures,
                    stubs,
                );
            }
        }
        match &job.body {
            JobBody::Handler(handler) => {
                collect_text_handler_refs(
                    &handler.path.path,
                    feature_name,
                    &format!("job.{}.handler", job.name),
                    signatures,
                    stubs,
                );
                if let Some(returns) = &handler.returns {
                    collect_type_ref(returns, feature_name, &site, signatures, stubs);
                }
            }
            JobBody::Declarative(body) => {
                if let Some(target) = &body.target {
                    for arg in &target.args {
                        collect_expr_refs(
                            &arg.value,
                            feature_name,
                            &format!("job.{}.target.{}", job.name, arg.name),
                            signatures,
                            stubs,
                        );
                    }
                }
                for binding in &body.lets {
                    collect_expr_refs(
                        &binding.value,
                        feature_name,
                        &format!("job.{}.let.{}", job.name, binding.name),
                        signatures,
                        stubs,
                    );
                }
                collect_command_effect_refs(
                    &body.effect,
                    feature_name,
                    &format!("job.{}", job.name),
                    signatures,
                    stubs,
                );
            }
        }
    }
}

pub(super) fn walk_webhooks(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    for webhook in &feature.webhooks {
        let site = format!("webhook.{}", webhook.name);
        collect_text_handler_refs(
            &webhook.verify.path,
            feature_name,
            &format!("webhook.{}.verify", webhook.name),
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &webhook.handler.path,
            feature_name,
            &format!("webhook.{}.handler", webhook.name),
            signatures,
            stubs,
        );
        collect_policy_ref(&webhook.policy, feature_name, &site, signatures, stubs);
        if let Some(idempotency) = &webhook.idempotency {
            collect_path_refs(
                &idempotency.by,
                feature_name,
                &format!("webhook.{}.idempotency", webhook.name),
                signatures,
                stubs,
            );
        }
        if let Some(replay) = &webhook.replay
            && let Some(dedupe_by) = &replay.dedupe_by
        {
            collect_path_refs(
                dedupe_by,
                feature_name,
                &format!("webhook.{}.replay.dedupe_by", webhook.name),
                signatures,
                stubs,
            );
        }
        if let Some(returns) = &webhook.returns {
            collect_type_ref(returns, feature_name, &site, signatures, stubs);
        }
    }
}

pub(super) fn walk_notifications(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    for notification in &feature.notifications {
        let site = format!("notification.{}", notification.name);
        collect_job_trigger_refs(
            &notification.trigger,
            feature_name,
            &site,
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &notification.recipient,
            feature_name,
            &format!("notification.{}.recipient", notification.name),
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &notification.template,
            feature_name,
            &format!("notification.{}.template", notification.name),
            signatures,
            stubs,
        );
        collect_policy_ref(&notification.policy, feature_name, &site, signatures, stubs);
        if let Some(tenant_from) = &notification.tenant_from {
            collect_path_refs(
                &tenant_from.path,
                feature_name,
                &format!("notification.{}.tenant_from", notification.name),
                signatures,
                stubs,
            );
        }
        if let Some(idempotency) = &notification.idempotency {
            collect_path_refs(
                &idempotency.by,
                feature_name,
                &format!("notification.{}.idempotency", notification.name),
                signatures,
                stubs,
            );
        }
        if let Some(digest) = &notification.digest {
            collect_optional_text_handler_ref(
                &digest.group_by,
                feature_name,
                &format!("notification.{}.digest.group_by", notification.name),
                signatures,
                stubs,
            );
        }
    }
}

pub(super) fn walk_event_groups(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    for event_group in &feature.event_groups {
        let site = format!("event_group.{}", event_group.pattern);
        for payload in &event_group.raw_payload {
            collect_text_handler_refs(payload, feature_name, &site, signatures, stubs);
        }
        collect_optional_text_handler_ref(
            &event_group.raw_audit,
            feature_name,
            &site,
            signatures,
            stubs,
        );
    }
}

pub(super) fn walk_auth(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    let Some(auth) = &feature.auth else {
        return;
    };
    if let Some(password) = &auth.password {
        collect_text_handler_refs(
            &password.hash,
            feature_name,
            "auth.password.hash",
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &password.verify,
            feature_name,
            "auth.password.verify",
            signatures,
            stubs,
        );
    }
    if let Some(mfa) = &auth.mfa {
        collect_text_handler_refs(
            &mfa.enroll,
            feature_name,
            "auth.mfa.enroll",
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &mfa.verify,
            feature_name,
            "auth.mfa.verify",
            signatures,
            stubs,
        );
        collect_optional_text_handler_ref(
            &mfa.adapter,
            feature_name,
            "auth.mfa.adapter",
            signatures,
            stubs,
        );
    }
    for oauth in &auth.oauth {
        collect_text_handler_refs(
            &oauth.adapter,
            feature_name,
            &format!("auth.oauth.{}", oauth.provider),
            signatures,
            stubs,
        );
    }
}

/// Feature extension declarations are implementation obligations even when
/// the current IR has no concrete call site yet. Collect them after usage
/// sites so a real reference (for example `auth.password.hash`) keeps the
/// more precise `Site:` comment.
pub(super) fn walk_extensions(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    for extension in &feature.extensions {
        match &extension.contract {
            ExtensionContract::Function { .. } => push_handler_ref(
                HandlerNamespace::Fn,
                &extension.name,
                feature_name,
                &format!("extensions.fn.{}", extension.name),
                signatures,
                stubs,
            ),
            ExtensionContract::Hook { .. } => push_handler_ref(
                HandlerNamespace::Hook,
                &extension.name,
                feature_name,
                &format!("extensions.hook.{}", extension.name),
                signatures,
                stubs,
            ),
            ExtensionContract::CellRenderer { .. }
            | ExtensionContract::ViewBlock { .. }
            | ExtensionContract::FormField { .. }
            | ExtensionContract::Validator { .. }
            | ExtensionContract::QueryModifier { .. }
            | ExtensionContract::IntegrationAdapter { .. } => {}
        }
    }
}

pub(super) fn walk_agents(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();
    for agent in &feature.agents {
        let site = format!("agent.{}", agent.name);
        for slot in &agent.input {
            collect_type_ref(
                &slot.type_ref,
                feature_name,
                &format!("agent.{}.input.{}", agent.name, slot.name),
                signatures,
                stubs,
            );
        }
        if let Some(context) = &agent.context {
            for arg in &context.args {
                collect_expr_refs(
                    &arg.value,
                    feature_name,
                    &format!("agent.{}.context.{}", agent.name, arg.name),
                    signatures,
                    stubs,
                );
            }
        }
        collect_policy_ref(&agent.policy, feature_name, &site, signatures, stubs);
        if let Some(output_type) = &agent.output_type {
            collect_type_ref(output_type, feature_name, &site, signatures, stubs);
        }
        for eval in &agent.evals {
            let eval_site = format!("agent.{}.eval.{}", agent.name, eval.name);
            for assertion in &eval.assertions {
                match &assertion.predicate {
                    EvalPredicate::Closed(predicate) => {
                        collect_predicate_refs(
                            predicate,
                            feature_name,
                            &eval_site,
                            signatures,
                            stubs,
                        );
                    }
                    EvalPredicate::Contains { rhs, .. } => {
                        if let EvalContainsRhs::SemanticType(qname) = rhs {
                            collect_text_handler_refs(
                                &qname.name,
                                feature_name,
                                &eval_site,
                                signatures,
                                stubs,
                            );
                        }
                    }
                    EvalPredicate::ToolsCalls { .. } => {}
                    EvalPredicate::Unparsed(text) => {
                        collect_text_handler_refs(
                            text,
                            feature_name,
                            &eval_site,
                            signatures,
                            stubs,
                        );
                    }
                }
            }
        }
    }
}
