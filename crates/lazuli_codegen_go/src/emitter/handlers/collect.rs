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
    Command, CommandEffect, CommandInput, EvalContainsRhs, EvalPredicate, Expr, ExtensionContract,
    Feature, JobBody, Module, PolicyRef, Predicate, Query, TestAssertion, TestBlock, TypeRef,
};

use super::paths::path_name_for;
use super::types::go_type_for_stub;
use super::{
    HandlerNamespace, HandlerSignature, HandlerStub, SignatureMap, StubKey,
};

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

/// Stub the command's user-side handler when the generated `Effect`
/// will go through `ReturnsFromRegistry`. Skips creates/updates/
/// deletes (no user fn needed). Picks the handler name from
/// `command.handler` when present (`handler @fn.X`), else falls back
/// to the command name itself. Input/output types come straight from
/// the command's IR. Stubs are keyed by `(feature, handler_name)` —
/// when multiple commands share a handler the FIRST seen wins
/// (deterministic by IR walk order).
fn collect_command_handler_stub(
    command: &Command,
    feature: &str,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    use lazuli_ir::CommandEffect;
    // Synthesized auto-photo commands (FR-3a) are wired by the
    // codegen-emitted `auto_photo.gen.go` directly against the
    // runtime `lazuli.AutoPhoto*` helper. Scaffolding a user handler
    // here would conflict with the runtime registration — skip.
    if command.synthesized_from_cap_file.is_some() {
        return;
    }
    // Skip declarative effects — the runtime executes the SQL,
    // there's no user fn to scaffold.
    if matches!(
        command.effect,
        CommandEffect::Creates(_) | CommandEffect::Updates(_) | CommandEffect::Deletes(_)
    ) {
        return;
    }
    // `CommandEffect::None` without an explicit `handler @fn.X` is a
    // legacy no-effect command (`command.Invoke` path). No user fn
    // expected.
    let handler_name = match (&command.handler, &command.effect) {
        (Some(h), _) if h.namespace == "fn" => h.name.clone(),
        (_, CommandEffect::Returns(_)) => command.name.clone(),
        _ => return,
    };
    let resource_pascal = crate::emitter::command::effect_resource_pascal(&command.effect);
    let input_type = match &command.input {
        CommandInput::Typed(_) | CommandInput::Short(_) => {
            crate::emitter::command::command_input_struct_name(&command.name, &resource_pascal)
        }
        CommandInput::Empty if !command.route.is_empty() => {
            crate::emitter::command::command_input_struct_name(&command.name, &resource_pascal)
        }
        CommandInput::Empty => "struct{}".to_owned(),
    };
    let output_type = match &command.effect {
        CommandEffect::Returns(ret) => go_type_for_stub(&ret.return_type),
        _ => "any".to_owned(),
    };
    let site = format!("{feature}.command.{}", command.name);
    let path_name = path_name_for(&handler_name);
    let key = StubKey {
        feature: feature.to_owned(),
        path_name: path_name.clone(),
    };
    stubs.entry(key).or_insert(HandlerStub {
        feature: feature.to_owned(),
        namespace: HandlerNamespace::Fn,
        name: handler_name,
        site,
        input_type,
        output_type,
    });
}

fn collect_command_refs(
    command: &Command,
    feature: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    for slot in &command.route {
        let slot_site = format!("{}.route.{}", command.name, slot.name);
        collect_type_ref(&slot.type_ref, feature, &slot_site, signatures, stubs);
        collect_optional_text_handler_ref(&slot.from, feature, &slot_site, signatures, stubs);
    }
    if let CommandInput::Typed(slots) = &command.input {
        for slot in slots {
            collect_type_ref(
                &slot.type_ref,
                feature,
                &format!("{}.input.{}", command.name, slot.name),
                signatures,
                stubs,
            );
        }
    }
    if let Some(target) = &command.target {
        for arg in &target.args {
            collect_expr_refs(
                &arg.value,
                feature,
                &format!("{}.target.{}", command.name, arg.name),
                signatures,
                stubs,
            );
        }
    }
    for binding in &command.lets {
        collect_expr_refs(
            &binding.value,
            feature,
            &format!("{}.let.{}", command.name, binding.name),
            signatures,
            stubs,
        );
    }
    collect_command_effect_refs(&command.effect, feature, &command.name, signatures, stubs);

    // Stub the user-side handler that the generated `Effect:
    // lazuli.ReturnsFromRegistry[I, O]("<feature>.<n>")` will look
    // up at dispatch. Two trigger shapes:
    //   1. `CommandEffect::Returns` — the command declares `returns
    //      <Type>` and (implicitly or via `handler @fn.X`) needs a
    //      user fn that returns that type.
    //   2. `CommandEffect::None + handler @fn.X` — no declarative
    //      body, handler resolves to user code returning `any`.
    // Creates/Updates/Deletes don't need a stub (the runtime owns
    // the SQL transaction). When `handler @fn.X` is explicit, prefer
    // its name; otherwise fall back to the command name itself —
    // matches what `command.rs::emit_effect` emits.
    collect_command_handler_stub(command, feature, stubs);
    collect_policy_ref(
        &Some(command.policy.clone()),
        feature,
        &format!("{}.policy", command.name),
        signatures,
        stubs,
    );
    if let Some(audit) = &command.audit {
        for subject in &audit.subjects {
            collect_text_handler_refs(
                subject,
                feature,
                &format!("{}.audit", command.name),
                signatures,
                stubs,
            );
        }
    }
    if let Some(approval) = &command.approval {
        collect_optional_text_handler_ref(
            &approval.required_when,
            feature,
            &format!("{}.approval.required_when", command.name),
            signatures,
            stubs,
        );
        collect_text_handler_refs(
            &approval.by,
            feature,
            &format!("{}.approval.by", command.name),
            signatures,
            stubs,
        );
    }
    for invalidates in &command.invalidates {
        for arg in &invalidates.args {
            collect_expr_refs(
                &arg.value,
                feature,
                &format!("{}.invalidates.{}", command.name, arg.name),
                signatures,
                stubs,
            );
        }
    }
    for call in &command.external_calls {
        for arg in &call.args {
            collect_expr_refs(
                &arg.value,
                feature,
                &format!("{}.calls.{}.{}", command.name, call.slot, arg.name),
                signatures,
                stubs,
            );
        }
    }
    collect_test_block_refs(
        &command.tests,
        feature,
        &format!("{}.tests", command.name),
        signatures,
        stubs,
    );
}

fn collect_command_effect_refs(
    effect: &CommandEffect,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match effect {
        CommandEffect::Creates(effect) => {
            for assignment in &effect.assignments {
                collect_expr_refs(
                    &assignment.value,
                    feature,
                    &format!("{site}.creates.{}", assignment.field),
                    signatures,
                    stubs,
                );
            }
        }
        CommandEffect::Updates(effect) => {
            for assignment in &effect.assignments {
                collect_expr_refs(
                    &assignment.value,
                    feature,
                    &format!("{site}.updates.{}", assignment.field),
                    signatures,
                    stubs,
                );
            }
        }
        CommandEffect::Deletes(_) | CommandEffect::None => {}
        CommandEffect::Returns(effect) => {
            collect_type_ref(&effect.return_type, feature, site, signatures, stubs);
        }
    }
}

fn collect_query_refs(
    query: &Query,
    feature: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match query {
        Query::List(query) => {
            let site = format!("query.list.{}", query.name);
            for param in &query.params {
                collect_type_ref(&param.type_ref, feature, &site, signatures, stubs);
            }
            for predicate in &query.scope {
                collect_predicate_refs(predicate, feature, &site, signatures, stubs);
            }
            for filter in &query.filters {
                collect_predicate_refs(&filter.predicate, feature, &site, signatures, stubs);
                collect_optional_text_handler_ref(
                    &filter.when,
                    feature,
                    &format!("{}.filter.when", site),
                    signatures,
                    stubs,
                );
            }
            collect_optional_text_handler_ref(
                &query.modifier,
                feature,
                &format!("{}.modifier", site),
                signatures,
                stubs,
            );
            if let Some(cache) = &query.cache {
                collect_cache_refs(cache, feature, &site, signatures, stubs);
            }
        }
        Query::Lookup(query) => {
            let site = format!("query.lookup.{}", query.name);
            for param in &query.params {
                collect_type_ref(&param.type_ref, feature, &site, signatures, stubs);
            }
            for predicate in &query.scope {
                collect_predicate_refs(predicate, feature, &site, signatures, stubs);
            }
            for filter in &query.filters {
                collect_predicate_refs(&filter.predicate, feature, &site, signatures, stubs);
                collect_optional_text_handler_ref(
                    &filter.when,
                    feature,
                    &format!("{}.filter.when", site),
                    signatures,
                    stubs,
                );
            }
            for key in &query.keys {
                collect_path_refs(&key.path, feature, &site, signatures, stubs);
                collect_expr_refs(&key.equals, feature, &site, signatures, stubs);
            }
        }
        Query::Sql(query) => {
            let site = match query.sql_kind {
                lazuli_ir::SqlQueryKind::Sql => format!("query.sql.{}", query.name),
                lazuli_ir::SqlQueryKind::View => format!("query.view.{}", query.name),
            };
            for param in &query.params {
                collect_type_ref(&param.type_ref, feature, &site, signatures, stubs);
            }
            for predicate in &query.scope {
                collect_predicate_refs(predicate, feature, &site, signatures, stubs);
            }
            collect_type_ref(&query.returns, feature, &site, signatures, stubs);
            if let Some(cache) = &query.cache {
                collect_cache_refs(cache, feature, &site, signatures, stubs);
            }
        }
    }
}

fn collect_cache_refs(
    cache: &lazuli_ir::QueryCache,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    collect_text_handler_refs(
        &cache.key,
        feature,
        &format!("{site}.cache.key"),
        signatures,
        stubs,
    );
    collect_optional_text_handler_ref(
        &cache.namespace,
        feature,
        &format!("{site}.cache.namespace"),
        signatures,
        stubs,
    );
}

fn collect_type_ref(
    type_ref: &TypeRef,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match type_ref {
        TypeRef::Builtin(_) | TypeRef::Capability(_) => {}
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => {
            collect_text_handler_refs(&qname.name, feature, site, signatures, stubs);
        }
        TypeRef::Many(inner) => {
            collect_type_ref(inner, feature, site, signatures, stubs);
        }
        TypeRef::Unresolved(raw) => {
            collect_text_handler_refs(raw, feature, site, signatures, stubs);
        }
    }
}

fn collect_policy_ref(
    policy: &Option<PolicyRef>,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if let Some(PolicyRef::Atom(value) | PolicyRef::Unresolved(value)) = policy {
        collect_text_handler_refs(value, feature, site, signatures, stubs);
    }
}

fn collect_job_trigger_refs(
    trigger: &lazuli_ir::JobTrigger,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match trigger {
        lazuli_ir::JobTrigger::Event { event } => {
            collect_text_handler_refs(&event.name, feature, site, signatures, stubs);
        }
        lazuli_ir::JobTrigger::Schedule { cron } => {
            collect_text_handler_refs(cron, feature, site, signatures, stubs);
        }
    }
}

fn collect_predicate_refs(
    predicate: &Predicate,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            collect_expr_refs(left, feature, site, signatures, stubs);
            collect_expr_refs(right, feature, site, signatures, stubs);
        }
        Predicate::Has {
            collection,
            element,
        } => {
            collect_expr_refs(collection, feature, site, signatures, stubs);
            collect_expr_refs(element, feature, site, signatures, stubs);
        }
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_predicate_refs(predicate, feature, site, signatures, stubs);
            }
        }
    }
}

fn collect_expr_refs(
    expr: &Expr,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match expr {
        Expr::Path(path) => {
            collect_path_refs(path, feature, site, signatures, stubs);
        }
        Expr::String(value) => collect_text_handler_refs(value, feature, site, signatures, stubs),
        Expr::Enum(value) => {
            if let Some(qname) = &value.type_name {
                collect_text_handler_refs(&qname.name, feature, site, signatures, stubs);
            }
            collect_text_handler_refs(&value.variant, feature, site, signatures, stubs);
        }
        Expr::Integer(_) | Expr::Boolean(_) | Expr::Nil => {}
        Expr::FnCall(call) => {
            collect_text_handler_refs(&call.name.name, feature, site, signatures, stubs);
            for arg in &call.args {
                collect_expr_refs(arg, feature, site, signatures, stubs);
            }
        }
    }
}

fn collect_path_refs(
    path: &lazuli_ir::Path,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if path.segments.is_empty() {
        return;
    }
    collect_text_handler_refs(&path.segments.join("."), feature, site, signatures, stubs);
}

fn collect_test_block_refs(
    tests: &Option<TestBlock>,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let Some(tests) = tests else {
        return;
    };
    for assertion in &tests.assertions {
        match assertion {
            TestAssertion::PolicyAllow { actors } | TestAssertion::PolicyDeny { actors } => {
                for actor in actors {
                    collect_text_handler_refs(actor, feature, site, signatures, stubs);
                }
            }
            TestAssertion::AllowsWhen { predicate } | TestAssertion::DeniesWhen { predicate } => {
                collect_predicate_refs(predicate, feature, site, signatures, stubs);
            }
            TestAssertion::AllowsAs { actor }
            | TestAssertion::DeniesAs { actor }
            | TestAssertion::AllowsFromAs { actor, .. }
            | TestAssertion::DeniesFromAs { actor, .. } => {
                collect_text_handler_refs(actor, feature, site, signatures, stubs);
            }
            TestAssertion::AllowsFrom { .. }
            | TestAssertion::DeniesFrom { .. }
            | TestAssertion::AcceptedBy { .. }
            | TestAssertion::RejectedBy { .. } => {}
        }
    }
}

fn collect_optional_text_handler_ref(
    value: &Option<String>,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if let Some(value) = value {
        collect_text_handler_refs(value, feature, site, signatures, stubs);
    }
}

fn collect_text_handler_refs(
    text: &str,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    for reference in extract_handler_refs(text) {
        push_handler_ref(
            reference.namespace,
            &reference.name,
            feature,
            site,
            signatures,
            stubs,
        );
    }
}

fn push_handler_ref(
    namespace: HandlerNamespace,
    name: &str,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if name.is_empty() {
        return;
    }

    let path_name = path_name_for(name);
    if path_name.is_empty() {
        return;
    }

    let key = StubKey {
        feature: feature.to_owned(),
        path_name,
    };
    if stubs.contains_key(&key) {
        return;
    }

    let signature = signatures
        .get(&(feature.to_owned(), namespace, name.to_owned()))
        .cloned()
        .unwrap_or_else(HandlerSignature::any);

    stubs.insert(
        key,
        HandlerStub {
            feature: feature.to_owned(),
            namespace,
            name: name.to_owned(),
            site: format!("{feature}.{site}"),
            input_type: signature.input_type,
            output_type: signature.output_type,
        },
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HandlerRef {
    pub(super) namespace: HandlerNamespace,
    pub(super) name: String,
}

pub(super) fn extract_handler_refs(text: &str) -> Vec<HandlerRef> {
    let mut refs = Vec::new();
    let mut offset = 0;

    while let Some(relative_at) = text[offset..].find('@') {
        let start = offset + relative_at;
        let rest = &text[start..];
        let namespace = if rest.starts_with(HandlerNamespace::Fn.prefix()) {
            Some(HandlerNamespace::Fn)
        } else if rest.starts_with(HandlerNamespace::Hook.prefix()) {
            Some(HandlerNamespace::Hook)
        } else {
            None
        };

        let Some(namespace) = namespace else {
            offset = start + 1;
            continue;
        };

        let prefix_len = namespace.prefix().len();
        let after_prefix = &rest[prefix_len..];
        let name_len = after_prefix
            .char_indices()
            .find_map(
                |(index, ch)| {
                    if is_ref_char(ch) { None } else { Some(index) }
                },
            )
            .unwrap_or(after_prefix.len());
        let name = after_prefix[..name_len].trim_matches('.').to_owned();
        if !name.is_empty() {
            refs.push(HandlerRef { namespace, name });
        }
        offset = start + prefix_len + name_len;
    }

    refs
}

fn is_ref_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/')
}
