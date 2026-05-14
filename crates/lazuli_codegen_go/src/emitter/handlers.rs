//! Starter stub emission for user-authored Go extension handlers.
//!
//! This walker finds `@fn.<name>` and `@hook.<name>` references that are
//! already surfaced in the IR and emits one non-`.gen.go` starter file per
//! feature/name pair:
//!
//! ```text
//! <feature>/handlers/<name>.go
//! ```
//!
//! The files are intentionally user territory. Callers pass the files already
//! present under the Go output tree; this module skips any matching path so
//! regeneration never overwrites user-authored code. The orchestrator remains
//! responsible for performing the same check at write time.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{
    BuiltinType, CapabilityRef, Command, CommandEffect, CommandInput, EvalContainsRhs,
    EvalPredicate, Expr, ExtensionContract, Feature, JobBody, Module, PolicyRef, Predicate, Query,
    TestAssertion, TestBlock, TypeRef, View,
};

use crate::GeneratedFile;

use super::patterns::PATTERN_EXTENSION_STUB;

const DIST_GO_PREFIX: &str = "dist/go";

/// Emit starter stubs for every `@fn.*` and `@hook.*` reference in the IR.
///
/// Paths in `GeneratedFile` are relative to the Go output root, matching the
/// rest of `lazuli_codegen_go` (`<feature>/...`). `existing_files` may contain
/// relative paths, `dist/go/...` paths, or absolute paths below either root; all
/// are treated as existing and skipped. Passing an empty set emits all detected
/// stubs.
pub fn emit_handler_stubs(
    module: &Module,
    existing_files: &BTreeSet<PathBuf>,
) -> Vec<GeneratedFile> {
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
        .into_values()
        .filter_map(|stub| {
            let path = handler_path(&stub.feature, &stub.name);
            if path_exists(existing_files, &path) {
                return None;
            }
            Some(GeneratedFile {
                path,
                contents: emit_stub_contents(&stub),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StubKey {
    feature: String,
    path_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandlerStub {
    feature: String,
    namespace: HandlerNamespace,
    name: String,
    site: String,
    input_type: String,
    output_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandlerSignature {
    input_type: String,
    output_type: String,
}

impl HandlerSignature {
    fn any() -> Self {
        Self {
            input_type: "any".to_owned(),
            output_type: "any".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HandlerNamespace {
    Fn,
    Hook,
}

impl HandlerNamespace {
    fn prefix(self) -> &'static str {
        match self {
            HandlerNamespace::Fn => "@fn.",
            HandlerNamespace::Hook => "@hook.",
        }
    }
}

type SignatureKey = (String, HandlerNamespace, String);
type SignatureMap = BTreeMap<SignatureKey, HandlerSignature>;

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

    for surface in &feature.surfaces {
        for view in &surface.views {
            collect_view_refs(view, feature_name, signatures, stubs);
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
            let site = format!("query.sql.{}", query.name);
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

fn collect_view_refs(
    view: &View,
    feature: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match view {
        View::Table(view) => {
            for cell in &view.cells {
                collect_text_handler_refs(
                    &cell.renderer,
                    feature,
                    &format!("view.{}.{}", view.name, cell.field),
                    signatures,
                    stubs,
                );
            }
            collect_test_block_refs(
                &view.tests,
                feature,
                &format!("view.{}.tests", view.name),
                signatures,
                stubs,
            );
        }
        View::SidePanel(view) => {
            for block in &view.blocks {
                collect_text_handler_refs(
                    &block.renderer,
                    feature,
                    &format!("view.{}.block", view.name),
                    signatures,
                    stubs,
                );
            }
            collect_test_block_refs(
                &view.tests,
                feature,
                &format!("view.{}.tests", view.name),
                signatures,
                stubs,
            );
        }
        View::Form(view) => {
            collect_test_block_refs(
                &view.tests,
                feature,
                &format!("view.{}.tests", view.name),
                signatures,
                stubs,
            );
        }
        View::Custom(view) => {
            collect_optional_text_handler_ref(
                &view.source.as_ref().map(|source| source.query.name.clone()),
                feature,
                &format!("view.{}.source", view.name),
                signatures,
                stubs,
            );
            collect_test_block_refs(
                &view.tests,
                feature,
                &format!("view.{}.tests", view.name),
                signatures,
                stubs,
            );
        }
    }
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
struct HandlerRef {
    namespace: HandlerNamespace,
    name: String,
}

fn extract_handler_refs(text: &str) -> Vec<HandlerRef> {
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

fn emit_stub_contents(stub: &HandlerStub) -> String {
    let fn_name = exported_func_name(&stub.name);
    let literal = format!("{}{}", stub.namespace.prefix(), stub.name);
    let escaped_literal = escape_comment(&literal);
    let escaped_site = escape_comment(&stub.site);
    let escaped_error = escape_string(&format!("{} not yet implemented", stub.name));

    format!(
        r#"// Code generated by lazuli as a starter stub. Edit and remove the
// `// IMPLEMENT ME` marker once you ship the real implementation.
// Lazuli will not overwrite this file on regenerate.

package {package}

import (
	"context"
	"errors"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/observability"
)

// {fn_name} is the user-authored implementation of `{escaped_literal}`.
//   Site: {escaped_site}
// IMPLEMENT ME
//lazuli:pattern {pattern_id} {pattern_version}
func {fn_name}(ctx *lazuli.Ctx, input {input_type}) ({output_type}, error) {{
	if ctx.Context == nil {{
		ctx.Context = context.Background()
	}}
	ctx.Context = lazuli.WithSource(ctx.Context, lazuli.SourceTag{{
		Feature: "{feature}",
		Kind:    "handler",
		Op:      "{op}",
	}})
	var endOp func()
	ctx.Context, endOp = observability.StartOp(ctx.Context)
	defer endOp()
	_ = ctx
	_ = input
	_ = context.Background
	return zero[{output_type}](), errors.New("{escaped_error}")
}}

//lazuli:pattern {pattern_id} {pattern_version}
func zero[T any]() T {{
	var z T
	return z
}}
"#,
        package = stub.feature,
        feature = escape_string(&stub.feature),
        fn_name = fn_name,
        escaped_literal = escaped_literal,
        escaped_site = escaped_site,
        pattern_id = PATTERN_EXTENSION_STUB.0,
        pattern_version = PATTERN_EXTENSION_STUB.1,
        op = escape_string(&stub.name),
        input_type = stub.input_type,
        output_type = stub.output_type,
        escaped_error = escaped_error,
    )
}

fn go_type_for_stub(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => go_type_for_builtin(*builtin).to_owned(),
        TypeRef::Capability(capability) => go_type_for_capability(capability).to_owned(),
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => pascal_case(&qname.name),
        TypeRef::Many(inner) => format!("[]{}", go_type_for_stub(inner)),
        TypeRef::Unresolved(raw) => go_type_for_unresolved(raw),
    }
}

fn go_type_for_builtin(builtin: BuiltinType) -> &'static str {
    match builtin {
        BuiltinType::Id => "lazuli.ID",
        BuiltinType::Text => "string",
        BuiltinType::Boolean => "bool",
        BuiltinType::Integer => "int64",
        BuiltinType::Decimal => "float64",
        BuiltinType::Date => "lazuli.Date",
        BuiltinType::DateTime => "lazuli.Time",
        BuiltinType::Json => "lazuli.JSON",
        BuiltinType::SemanticEmail => "lazuli.Email",
        BuiltinType::SemanticMoney => "lazuli.Money",
        BuiltinType::SemanticPhone => "lazuli.Phone",
        BuiltinType::SemanticUrl => "lazuli.URL",
        BuiltinType::SemanticUuid => "lazuli.UUID",
        BuiltinType::SemanticCurrency => "lazuli.Currency",
        BuiltinType::SemanticGeoPoint => "any",
        BuiltinType::CapSecret => "lazuli.Secret",
        BuiltinType::CapFile => "any",
    }
}

fn go_type_for_capability(capability: &CapabilityRef) -> &'static str {
    match capability {
        CapabilityRef::Hashed(_) => "lazuli.HashedRef",
        CapabilityRef::Encrypted(_) => "lazuli.EncryptedRef",
        CapabilityRef::Token(_) => "lazuli.TokenRef",
        CapabilityRef::File(_) => "any",
    }
}

fn go_type_for_unresolved(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_suffix("[]") {
        return format!("[]{}", go_type_for_unresolved(inner));
    }

    match trimmed {
        "ID" | "Id" => "lazuli.ID".to_owned(),
        "Text" | "String" => "string".to_owned(),
        "Boolean" | "Bool" => "bool".to_owned(),
        "Integer" | "Int" => "int64".to_owned(),
        "Decimal" | "Float" => "float64".to_owned(),
        "Date" => "lazuli.Date".to_owned(),
        "DateTime" => "lazuli.Time".to_owned(),
        "Json" | "JSON" => "lazuli.JSON".to_owned(),
        "@semantic.Email" => "lazuli.Email".to_owned(),
        "@semantic.Money" => "lazuli.Money".to_owned(),
        "@semantic.Phone" => "lazuli.Phone".to_owned(),
        "@semantic.Url" | "@semantic.URL" => "lazuli.URL".to_owned(),
        "@semantic.Uuid" | "@semantic.UUID" => "lazuli.UUID".to_owned(),
        "@semantic.Currency" => "lazuli.Currency".to_owned(),
        "@cap.Secret" => "lazuli.Secret".to_owned(),
        _ if trimmed.starts_with("@cap.Hashed") => "lazuli.HashedRef".to_owned(),
        _ if trimmed.starts_with("@cap.Encrypted") => "lazuli.EncryptedRef".to_owned(),
        _ if trimmed.starts_with("@cap.Token") => "lazuli.TokenRef".to_owned(),
        _ if trimmed.starts_with("@cap.File") => "any".to_owned(),
        _ if trimmed.starts_with('@') => "any".to_owned(),
        _ => pascal_case(trimmed),
    }
}

fn handler_path(feature: &str, name: &str) -> String {
    format!("{feature}/handlers/{}.go", path_name_for(name))
}

fn path_exists(existing_files: &BTreeSet<PathBuf>, relative_path: &str) -> bool {
    if existing_files.is_empty() {
        return false;
    }

    let rel = normalize_path_string(relative_path);
    let dist = format!("{DIST_GO_PREFIX}/{rel}");
    let rel_suffix = format!("/{rel}");
    let dist_suffix = format!("/{dist}");

    existing_files.iter().any(|path| {
        let existing = normalize_path(path);
        existing == rel
            || existing == dist
            || existing.ends_with(&rel_suffix)
            || existing.ends_with(&dist_suffix)
    })
}

fn normalize_path(path: &Path) -> String {
    normalize_path_string(&path.to_string_lossy())
}

fn normalize_path_string(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

fn path_name_for(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn exported_func_name(name: &str) -> String {
    let pascal = pascal_case(name);
    if pascal.is_empty() {
        return "Handler".to_owned();
    }
    if pascal
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        return format!("Handler{pascal}");
    }
    pascal
}

fn pascal_case(s: &str) -> String {
    super::casing::pascal_case(s)
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_comment(raw: &str) -> String {
    raw.replace('\n', " ").replace('\r', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Auth, AuthIdentity, AuthPassword, CapabilityRef, CommandKind, Defaults, Extension,
        FieldRef, HashAlgorithm, HashedCapability, Path, PathRef, PathSource, Policies,
        QualifiedName, TypedSlot,
    };

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            features,
        }
    }

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn extension(name: &str, contract: ExtensionContract) -> Extension {
        Extension {
            name: name.to_owned(),
            contract,
            resolved_path: PathRef {
                path: format!("./handlers/{name}.go"),
                source: PathSource::Convention,
            },
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn identity() -> AuthIdentity {
        AuthIdentity {
            field: FieldRef {
                resource: QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                },
                field: "email".to_owned(),
            },
        }
    }

    #[test]
    fn emits_auth_function_stub_with_extension_signature() {
        let mut feature = base_feature("customer_auth");
        feature.auth = Some(Auth {
            identity: identity(),
            password: Some(AuthPassword {
                algorithm: "argon2id".to_owned(),
                hash: "@fn.hash_password".to_owned(),
                verify: "@fn.verify_password".to_owned(),
                rate_limit: None,
            }),
            sessions: None,
            mfa: None,
            oauth: Vec::new(),
            span_ref: None,
        });
        feature.extensions.push(extension(
            "hash_password",
            ExtensionContract::Function {
                input: TypeRef::Builtin(BuiltinType::Text),
                output: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                    algorithm: HashAlgorithm::Argon2id,
                })),
            },
        ));
        let module = module_with_features(vec![feature]);

        let files = emit_handler_stubs(&module, &BTreeSet::new());
        let hash = files
            .iter()
            .find(|file| file.path == "customer_auth/handlers/hash_password.go")
            .expect("hash stub emitted");

        assert!(hash.contents.contains("package customer_auth"));
        assert!(hash.contents.contains(
            "func HashPassword(ctx *lazuli.Ctx, input string) (lazuli.HashedRef, error)"
        ));
        assert!(
            hash.contents
                .contains("//   Site: customer_auth.auth.password.hash")
        );
        assert!(hash.contents.contains(
            "return zero[lazuli.HashedRef](), errors.New(\"hash_password not yet implemented\")"
        ));
        assert!(hash.contents.contains("//lazuli:pattern extension_stub v1"));
        assert!(
            hash.contents
                .contains("ctx.Context = lazuli.WithSource(ctx.Context, lazuli.SourceTag{")
        );
        assert!(
            hash.contents
                .contains("ctx.Context, endOp = observability.StartOp(ctx.Context)")
        );
        assert_eq!(hash.contents.matches("func zero[T any]() T").count(), 1);
    }

    #[test]
    fn skips_relative_existing_files() {
        let mut feature = base_feature("customer_auth");
        feature.auth = Some(Auth {
            identity: identity(),
            password: Some(AuthPassword {
                algorithm: "argon2id".to_owned(),
                hash: "@fn.hash_password".to_owned(),
                verify: "@fn.verify_password".to_owned(),
                rate_limit: None,
            }),
            sessions: None,
            mfa: None,
            oauth: Vec::new(),
            span_ref: None,
        });
        let module = module_with_features(vec![feature]);
        let existing = BTreeSet::from([PathBuf::from("customer_auth/handlers/hash_password.go")]);

        let files = emit_handler_stubs(&module, &existing);

        assert!(
            !files
                .iter()
                .any(|file| file.path == "customer_auth/handlers/hash_password.go")
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "customer_auth/handlers/verify_password.go")
        );
    }

    #[test]
    fn skips_dist_go_existing_files() {
        let mut feature = base_feature("customer_auth");
        feature.auth = Some(Auth {
            identity: identity(),
            password: Some(AuthPassword {
                algorithm: "argon2id".to_owned(),
                hash: "@fn.hash_password".to_owned(),
                verify: "@fn.verify_password".to_owned(),
                rate_limit: None,
            }),
            sessions: None,
            mfa: None,
            oauth: Vec::new(),
            span_ref: None,
        });
        let module = module_with_features(vec![feature]);
        let existing = BTreeSet::from([PathBuf::from(
            "dist/go/customer_auth/handlers/hash_password.go",
        )]);

        let files = emit_handler_stubs(&module, &existing);

        assert!(
            !files
                .iter()
                .any(|file| file.path == "customer_auth/handlers/hash_password.go")
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "customer_auth/handlers/verify_password.go")
        );
    }

    #[test]
    fn extracts_function_ref_from_path_encoded_call_expression() {
        let mut feature = base_feature("customer");
        feature.commands.push(Command {
            name: "recompute_score".to_owned(),
            kind: CommandKind::Update,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: vec![lazuli_ir::LetBinding {
                name: "new_score".to_owned(),
                value: Expr::Path(Path {
                    segments: vec!["@fn".to_owned(), "risk_score(target)".to_owned()],
                }),
            }],
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            deprecated: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
        });
        feature.extensions.push(extension(
            "risk_score",
            ExtensionContract::Function {
                input: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                }),
                output: TypeRef::Builtin(BuiltinType::Integer),
            },
        ));
        let module = module_with_features(vec![feature]);

        let files = emit_handler_stubs(&module, &BTreeSet::new());

        let risk = files
            .iter()
            .find(|file| file.path == "customer/handlers/risk_score.go")
            .expect("risk_score stub emitted");
        assert!(
            risk.contents
                .contains("func RiskScore(ctx *lazuli.Ctx, input Customer) (int64, error)")
        );
        assert!(
            risk.contents
                .contains("//   Site: customer.recompute_score.let.new_score")
        );
    }

    #[test]
    fn emits_hook_ref_with_hook_signature() {
        let mut feature = base_feature("customer");
        feature.uses.push("@hook.before_create".to_owned());
        feature.extensions.push(extension(
            "before_create",
            ExtensionContract::Hook {
                type_arg: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "CreateCustomer".to_owned(),
                }),
            },
        ));
        let module = module_with_features(vec![feature]);

        let files = emit_handler_stubs(&module, &BTreeSet::new());
        let hook = files
            .iter()
            .find(|file| file.path == "customer/handlers/before_create.go")
            .expect("hook stub emitted");

        assert!(hook.contents.contains("`@hook.before_create`"));
        assert!(hook.contents.contains(
            "func BeforeCreate(ctx *lazuli.Ctx, input CreateCustomer) (CreateCustomer, error)"
        ));
    }

    #[test]
    fn extension_declarations_emit_starter_stubs() {
        let mut feature = base_feature("customer");
        feature.extensions.push(extension(
            "risk_score",
            ExtensionContract::Function {
                input: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                }),
                output: TypeRef::Builtin(BuiltinType::Integer),
            },
        ));
        let module = module_with_features(vec![feature]);

        let files = emit_handler_stubs(&module, &BTreeSet::new());

        let risk = files
            .iter()
            .find(|file| file.path == "customer/handlers/risk_score.go")
            .expect("risk_score declaration stub emitted");
        assert!(
            risk.contents
                .contains("//   Site: customer.extensions.fn.risk_score")
        );
    }

    #[test]
    fn unresolved_signature_falls_back_to_any() {
        let mut feature = base_feature("customer");
        feature.uses.push("@fn.unknown".to_owned());
        let module = module_with_features(vec![feature]);

        let files = emit_handler_stubs(&module, &BTreeSet::new());
        let file = &files[0];

        assert!(
            file.contents
                .contains("func Unknown(ctx *lazuli.Ctx, input any) (any, error)")
        );
        assert!(
            file.contents
                .contains("return zero[any](), errors.New(\"unknown not yet implemented\")")
        );
    }

    #[test]
    fn many_and_unresolved_types_render_without_extra_imports() {
        assert_eq!(
            go_type_for_stub(&TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Id)))),
            "[]lazuli.ID"
        );
        assert_eq!(
            go_type_for_stub(&TypeRef::Unresolved(
                "@cap.Hashed(algorithm:argon2id)".to_owned()
            )),
            "lazuli.HashedRef"
        );
    }

    #[test]
    fn sanitises_unsafe_path_and_function_name() {
        assert_eq!(
            handler_path("customer", "../risk.score"),
            "customer/handlers/risk_score.go"
        );
        assert_eq!(exported_func_name("123_score"), "Handler123Score");
    }

    #[test]
    fn extracts_multiple_handler_refs_from_text() {
        let refs = extract_handler_refs(
            "score = @fn.risk_score(target) then @hook.before_create and @client.cell",
        );

        assert_eq!(
            refs,
            vec![
                HandlerRef {
                    namespace: HandlerNamespace::Fn,
                    name: "risk_score".to_owned(),
                },
                HandlerRef {
                    namespace: HandlerNamespace::Hook,
                    name: "before_create".to_owned(),
                },
            ]
        );
    }

    #[allow(dead_code)]
    fn _typed_slot_compiles(_: TypedSlot) {}
}
