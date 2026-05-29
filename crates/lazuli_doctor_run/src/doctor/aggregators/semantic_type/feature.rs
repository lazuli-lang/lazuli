//! IR-level `@semantic.<Name>` catalog check — walks the lowered
//! `lazuli_ir::Feature` (resources / records / events / commands /
//! queries / jobs / webhooks / apis / agents / extensions) and emits
//! `semantic_type_unknown` for every type-ref that names a `@semantic.*`
//! outside the closed catalog. The IR pass complements the syntax-level
//! pass in `syntax_feature.rs` so both pre-lowering type-texts and
//! post-lowering type-refs are covered.
//!
//! Wave R7-3 extract.

use std::path::Path;

use super::shared::{
    find_nested_type_site_line, push_unknown_semantic_type, query_line_col, span_line_col,
};
use crate::doctor::DoctorDiagnostic;

pub(crate) fn semantic_type_unknown_diagnostics_for_feature(
    path: &Path,
    source: &str,
    feature: &lazuli_ir::Feature,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let feature_loc = span_line_col(source, feature.span_ref.as_ref()).unwrap_or((1, 1));

    for resource in &feature.resources {
        let resource_loc = span_line_col(source, resource.span_ref.as_ref()).unwrap_or(feature_loc);
        for field in &resource.fields {
            let loc = span_line_col(source, field.span_ref.as_ref())
                .or_else(|| find_nested_type_site_line(source, resource_loc.0, &field.name))
                .unwrap_or(resource_loc);
            push_unknown_semantic_type(path, &field.type_ref, loc, &mut diagnostics);
        }
    }

    for record in &feature.records {
        let record_loc = span_line_col(source, record.span_ref.as_ref()).unwrap_or(feature_loc);
        for field in &record.fields {
            let loc = span_line_col(source, field.span_ref.as_ref())
                .or_else(|| find_nested_type_site_line(source, record_loc.0, &field.name))
                .unwrap_or(record_loc);
            push_unknown_semantic_type(path, &field.type_ref, loc, &mut diagnostics);
        }
    }

    for event in &feature.events {
        let event_loc = span_line_col(source, event.span_ref.as_ref()).unwrap_or(feature_loc);
        for field in &event.payload {
            let loc =
                find_nested_type_site_line(source, event_loc.0, &field.name).unwrap_or(event_loc);
            push_unknown_semantic_type(path, &field.type_ref, loc, &mut diagnostics);
        }
    }

    for command in &feature.commands {
        let command_loc = span_line_col(source, command.span_ref.as_ref()).unwrap_or(feature_loc);
        for slot in &command.route {
            let loc = find_nested_type_site_line(source, command_loc.0, &slot.name)
                .unwrap_or(command_loc);
            push_unknown_semantic_type(path, &slot.type_ref, loc, &mut diagnostics);
        }
        if let lazuli_ir::CommandInput::Typed(slots) = &command.input {
            check_typed_slots_for_unknown_semantics(
                path,
                source,
                slots,
                command_loc,
                &mut diagnostics,
            );
        }
        check_command_effect_for_unknown_semantics(
            path,
            &command.effect,
            command_loc,
            &mut diagnostics,
        );
    }

    for query in &feature.queries {
        let query_loc = query_line_col(source, query).unwrap_or(feature_loc);
        match query {
            lazuli_ir::Query::List(query) => {
                check_typed_slots_for_unknown_semantics(
                    path,
                    source,
                    &query.params,
                    query_loc,
                    &mut diagnostics,
                );
            }
            lazuli_ir::Query::Lookup(query) => {
                check_typed_slots_for_unknown_semantics(
                    path,
                    source,
                    &query.params,
                    query_loc,
                    &mut diagnostics,
                );
            }
            lazuli_ir::Query::Sql(query) => {
                check_typed_slots_for_unknown_semantics(
                    path,
                    source,
                    &query.params,
                    query_loc,
                    &mut diagnostics,
                );
                push_unknown_semantic_type(path, &query.returns, query_loc, &mut diagnostics);
            }
        }
    }

    for job in &feature.jobs {
        let job_loc = span_line_col(source, job.span_ref.as_ref()).unwrap_or(feature_loc);
        match &job.body {
            lazuli_ir::JobBody::Handler(handler) => {
                if let Some(returns) = handler.returns.as_ref() {
                    push_unknown_semantic_type(path, returns, job_loc, &mut diagnostics);
                }
            }
            lazuli_ir::JobBody::Declarative(body) => {
                check_command_effect_for_unknown_semantics(
                    path,
                    &body.effect,
                    job_loc,
                    &mut diagnostics,
                );
            }
        }
    }

    for webhook in &feature.webhooks {
        let webhook_loc = span_line_col(source, webhook.span_ref.as_ref()).unwrap_or(feature_loc);
        if let Some(returns) = webhook.returns.as_ref() {
            push_unknown_semantic_type(path, returns, webhook_loc, &mut diagnostics);
        }
    }

    for api in &feature.apis {
        let api_loc = span_line_col(source, api.span_ref.as_ref()).unwrap_or(feature_loc);
        push_unknown_semantic_type(path, &api.output, api_loc, &mut diagnostics);
    }

    for agent in &feature.agents {
        let agent_loc = span_line_col(source, agent.span_ref.as_ref()).unwrap_or(feature_loc);
        check_typed_slots_for_unknown_semantics(
            path,
            source,
            &agent.input,
            agent_loc,
            &mut diagnostics,
        );
        if let Some(output_type) = agent.output_type.as_ref() {
            push_unknown_semantic_type(path, output_type, agent_loc, &mut diagnostics);
        }
        if let Some(expose) = agent.expose_http.as_ref() {
            let expose_loc = span_line_col(source, expose.span_ref.as_ref()).unwrap_or(agent_loc);
            check_typed_slots_for_unknown_semantics(
                path,
                source,
                &expose.route_slots,
                expose_loc,
                &mut diagnostics,
            );
        }
    }

    for extension in &feature.extensions {
        let extension_loc =
            span_line_col(source, extension.span_ref.as_ref()).unwrap_or(feature_loc);
        check_extension_contract_for_unknown_semantics(
            path,
            &extension.contract,
            extension_loc,
            &mut diagnostics,
        );
    }

    diagnostics
}

pub(crate) fn check_typed_slots_for_unknown_semantics(
    path: &Path,
    source: &str,
    slots: &[lazuli_ir::TypedSlot],
    parent_loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    for slot in slots {
        let loc =
            find_nested_type_site_line(source, parent_loc.0, &slot.name).unwrap_or(parent_loc);
        push_unknown_semantic_type(path, &slot.type_ref, loc, diagnostics);
    }
}

pub(crate) fn check_command_effect_for_unknown_semantics(
    path: &Path,
    effect: &lazuli_ir::CommandEffect,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let lazuli_ir::CommandEffect::Returns(returns) = effect {
        push_unknown_semantic_type(path, &returns.return_type, loc, diagnostics);
    }
}

pub(crate) fn check_extension_contract_for_unknown_semantics(
    path: &Path,
    contract: &lazuli_ir::ExtensionContract,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    match contract {
        lazuli_ir::ExtensionContract::CellRenderer { type_arg }
        | lazuli_ir::ExtensionContract::ViewBlock { type_arg }
        | lazuli_ir::ExtensionContract::FormField { type_arg }
        | lazuli_ir::ExtensionContract::Hook { type_arg }
        | lazuli_ir::ExtensionContract::Validator { type_arg }
        | lazuli_ir::ExtensionContract::QueryModifier { type_arg }
        | lazuli_ir::ExtensionContract::IntegrationAdapter { type_arg } => {
            push_unknown_semantic_type(path, type_arg, loc, diagnostics);
        }
        lazuli_ir::ExtensionContract::Function { input, output } => {
            push_unknown_semantic_type(path, input, loc, diagnostics);
            push_unknown_semantic_type(path, output, loc, diagnostics);
        }
    }
}
