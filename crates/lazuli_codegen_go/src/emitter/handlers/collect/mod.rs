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

use lazuli_ir::{ExtensionContract, Feature, Module};

use super::types::go_type_for_stub;
use super::{HandlerNamespace, HandlerSignature, HandlerStub, SignatureMap, StubKey};

mod commands;
mod feature_walks;
mod refs;

use commands::{collect_command_refs, collect_query_refs};
#[cfg(test)]
pub(super) use refs::{HandlerRef, extract_handler_refs};
use refs::{
    collect_optional_text_handler_ref, collect_policy_ref, collect_predicate_refs,
    collect_test_block_refs, collect_text_handler_refs, collect_type_ref,
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

    // Larger IR sections live in `feature_walks.rs` — each helper
    // mirrors one major IR axis (jobs / webhooks / notifications /
    // event_groups / auth / extensions / agents). Order matters for
    // the `extensions` walker, which intentionally runs AFTER auth so a
    // concrete `auth.password.hash` site keeps its precise `Site:`
    // comment in the emitted stub.
    feature_walks::walk_jobs(feature, signatures, stubs);
    feature_walks::walk_webhooks(feature, signatures, stubs);
    feature_walks::walk_notifications(feature, signatures, stubs);
    feature_walks::walk_event_groups(feature, signatures, stubs);
    feature_walks::walk_auth(feature, signatures, stubs);
    feature_walks::walk_extensions(feature, signatures, stubs);
    feature_walks::walk_agents(feature, signatures, stubs);
}
