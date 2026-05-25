//! IR walker — discover every `@fn.<name>` / `@hook.<name>` reference
//! that a feature declares (whether explicitly via `extensions`, or
//! incidentally through a `handler @fn.X` clause / `@fn.X(...)` call
//! in a `let` binding / etc.) and lift each one into a `HandlerStub`.
//!
//! Two sources feed the walker:
//! - **Extension declarations** — `extensions { function ... }` and
//!   `extensions { hook<T> ... }` blocks carry typed I/O signatures
//!   that we lift into `HandlerSignature` so the emitted stub has
//!   tight types from the start.
//! - **Reference sites** — every textual `@fn.X` / `@hook.X` mention
//!   the IR carries (policy refs, paths in `lets`, audit subjects,
//!   workflow guards, command handlers, agent eval predicates, …)
//!   becomes a stub if no extension declaration claimed it first.
//!   These fall back to `(any, any)` signatures.
//!
//! Determinism: features walk via a `BTreeMap<&str, &Feature>` so
//! `module.features` ordering is irrelevant; the resulting
//! `BTreeMap<StubKey, HandlerStub>` is keyed by `(feature, path_name)`
//! and the emitter's outer iteration is `BTreeMap::into_values`.
//!
//! Boundary: this module only collects; it never emits Go source. The
//! sibling `emit.rs` consumes the stubs.

use std::collections::BTreeMap;

use lazuli_ir::{
    EvalContainsRhs, EvalPredicate, ExtensionContract, Feature, JobBody, Module,
};

use super::types::go_type_for_stub;
use super::{HandlerNamespace, HandlerSignature, HandlerStub, SignatureMap, StubKey};

mod commands;
mod refs;

use commands::{collect_command_refs, collect_command_effect_refs, collect_query_refs};
#[cfg(test)]
pub(super) use refs::{HandlerRef, extract_handler_refs};
use refs::{
    collect_expr_refs, collect_optional_text_handler_ref, collect_path_refs, collect_policy_ref,
    collect_predicate_refs, collect_test_block_refs, collect_text_handler_refs, collect_type_ref,
    push_handler_ref,
};

/// Job-trigger collector reused by job and notification dispatchers.
use refs::collect_job_trigger_refs;

pub(super) fn collect_handler_stubs(module: &Module) -> BTreeMap<StubKey, HandlerStub> {
    let signatures = collect_extension_signatures(module);
    let mut stubs = BTreeMap::<StubKey, HandlerStub>::new();

    let mut features: BTreeMap<&str, &Feature> = BTreeMap::new();
    for feature in &module.features {
        features.insert(feature.name.as_str(), feature);
    }

    for feature in features.values() {
        collect_feature_handler_refs(feature, &signatures, &mut stubs);
    }

    stubs
}

fn collect_extension_signatures(module: &Module) -> SignatureMap {
    let mut signatures = SignatureMap::new();

    for feature in &module.features {
        for extension in &feature.extensions {
            let (namespace, signature) = match &extension.contract {
                ExtensionContract::Function { input, output } => (
                    HandlerNamespace::Fn,
                    HandlerSignature {
                        input_type: go_type_for_stub(input),
                        output_type: go_type_for_stub(output),
                    },
                ),
                ExtensionContract::Hook { type_arg } => (
                    HandlerNamespace::Hook,
                    HandlerSignature {
                        input_type: go_type_for_stub(type_arg),
                        output_type: go_type_for_stub(type_arg),
                    },
                ),
                ExtensionContract::CellRenderer { .. }
                | ExtensionContract::ViewBlock { .. }
                | ExtensionContract::FormField { .. }
                | ExtensionContract::Validator { .. }
                | ExtensionContract::QueryModifier { .. }
                | ExtensionContract::IntegrationAdapter { .. } => continue,
            };
            signatures.insert(
                (feature.name.clone(), namespace, extension.name.clone()),
                signature,
            );
        }
    }

    signatures
}

fn collect_feature_handler_refs(
    feature: &Feature,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let feature_name = feature.name.as_str();

    for value in &feature.uses {
        collect_text_handler_refs(value, feature_name, "uses", signatures, stubs);
    }

    collect_policy_ref(
        &feature.defaults.policy,
        feature_name,
        "defaults.policy",
        signatures,
        stubs,
    );

    for resource in &feature.resources {
        collect_optional_text_handler_ref(
            &resource.validate.as_ref().map(|p| p.path.clone()),
            feature_name,
            &format!("resource.{}.validate", resource.name),
            signatures,
            stubs,
        );
        for validation in &resource.validates {
            collect_text_handler_refs(
                &validation.path.path,
                feature_name,
                &format!("resource.{}.validates.{}", resource.name, validation.field),
                signatures,
                stubs,
            );
        }
        for field in &resource.fields {
            let site = format!("resource.{}.{}", resource.name, field.name);
            collect_type_ref(&field.type_ref, feature_name, &site, signatures, stubs);
            collect_optional_text_handler_ref(
                &field.derived_from,
                feature_name,
                &site,
                signatures,
                stubs,
            );
        }
    }

    for record in &feature.records {
        for field in &record.fields {
            let site = format!("record.{}.{}", record.name, field.name);
            collect_type_ref(&field.type_ref, feature_name, &site, signatures, stubs);
            collect_optional_text_handler_ref(
                &field.derived_from,
                feature_name,
                &site,
                signatures,
                stubs,
            );
        }
    }

    for event in &feature.events {
        for field in &event.payload {
            collect_type_ref(
                &field.type_ref,
                feature_name,
                &format!("event.{}.{}", event.name, field.name),
                signatures,
                stubs,
            );
        }
    }

    for command in &feature.commands {
        collect_command_refs(command, feature_name, signatures, stubs);
    }

    for api in &feature.apis {
        collect_type_ref(
            &api.output,
            feature_name,
            &format!("api.{}.output", api.name),
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &api.handler.path,
            feature_name,
            &format!("api.{}.handler", api.name),
            signatures,
            stubs,
        );
    }

    for query in &feature.queries {
        collect_query_refs(query, feature_name, signatures, stubs);
    }

    for rule in &feature.rules {
        let site = format!("rule.{}", rule.title);
        collect_predicate_refs(&rule.when, feature_name, &site, signatures, stubs);
        collect_optional_text_handler_ref(
            &rule.message_ref,
            feature_name,
            &site,
            signatures,
            stubs,
        );
        collect_test_block_refs(&rule.tests, feature_name, &site, signatures, stubs);
    }

    for workflow in &feature.workflows {
        let site = format!("workflow.{}", workflow.name);
        collect_policy_ref(
            &workflow.default_policy,
            feature_name,
            &site,
            signatures,
            stubs,
        );
        for transition in &workflow.transitions {
            let transition_site = format!("workflow.{}.{}", workflow.name, transition.name);
            collect_optional_text_handler_ref(
                &transition.requires,
                feature_name,
                &transition_site,
                signatures,
                stubs,
            );
            collect_test_block_refs(
                &transition.tests,
                feature_name,
                &transition_site,
                signatures,
                stubs,
            );
        }
    }

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
        if let Some(replay) = &webhook.replay {
            if let Some(dedupe_by) = &replay.dedupe_by {
                collect_path_refs(
                    dedupe_by,
                    feature_name,
                    &format!("webhook.{}.replay.dedupe_by", webhook.name),
                    signatures,
                    stubs,
                );
            }
        }
        if let Some(returns) = &webhook.returns {
            collect_type_ref(returns, feature_name, &site, signatures, stubs);
        }
    }

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

    if let Some(auth) = &feature.auth {
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

    // Feature extension declarations are implementation obligations even when
    // the current IR has no concrete call site yet. Collect them after usage
    // sites so a real reference (for example `auth.password.hash`) keeps the
    // more precise `Site:` comment.
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

