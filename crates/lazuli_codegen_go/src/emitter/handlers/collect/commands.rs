//! Command + Query walkers — the two heaviest IR shapes for handler
//! discovery.
//!
//! `collect_command_refs` covers every site where a command can carry
//! a `@fn.X` / `@hook.X` reference (route slot resolvers, target args,
//! lets, declarative effects, audit subjects, approvals, invalidates,
//! external calls, tests, the explicit `handler @fn.X` clause).
//!
//! `collect_command_handler_stub` lifts the user-side stub for
//! commands whose generated `Effect:
//! lazuli.ReturnsFromRegistry[I, O]("<f>.<n>")` will look the function
//! up at dispatch. The input/output type strings come from the IR via
//! `super::super::command::*` helpers so the stub matches what
//! `emitter::command` already emits.
//!
//! `collect_query_refs` covers list / lookup / SQL queries —
//! parameters, scope predicates, filters, cache keys, lookup-key
//! paths and equality expressions, and the query-`returns` type for
//! SQL queries.

use std::collections::BTreeMap;

use lazuli_ir::{Command, CommandEffect, CommandInput, Query};

use super::super::paths::path_name_for;
use super::super::types::go_type_for_stub;
use super::super::{HandlerNamespace, HandlerStub, SignatureMap, StubKey};
use super::refs::{
    collect_expr_refs, collect_optional_text_handler_ref, collect_path_refs, collect_policy_ref,
    collect_predicate_refs, collect_test_block_refs, collect_text_handler_refs, collect_type_ref,
};

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

pub(super) fn collect_command_refs(
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

pub(super) fn collect_command_effect_refs(
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
        // W4 GAP-REORDER-01 — reorder carries no expression operands.
        CommandEffect::Deletes(_) | CommandEffect::Reorders(_) | CommandEffect::None => {}
        CommandEffect::Returns(effect) => {
            collect_type_ref(&effect.return_type, feature, site, signatures, stubs);
        }
    }
}

pub(super) fn collect_query_refs(
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
        // query.compose: W3/W5 — handler-stub collection over the composite
        // read lands with the compose analyzer + codegen cells.
        Query::Compose(_) => {}
    }
}

pub(super) fn collect_cache_refs(
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
