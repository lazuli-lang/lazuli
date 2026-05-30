//! Pre-IR `@semantic.<Name>` catalog check — walks the syntax-level
//! `FeatureSkeleton` and emits a `semantic_type_unknown` diagnostic for
//! every authored type-text that references a name outside the closed
//! catalog. Runs before lowering so the diagnostic fires even when
//! later passes can't lift the type.
//!
//! Wave R7-3 extract.

use std::path::Path;

use super::shared::push_unknown_semantic_type_text;
use crate::doctor::DoctorDiagnostic;

pub(crate) fn semantic_type_unknown_diagnostics_for_syntax_feature(
    path: &Path,
    source: &str,
    feature: &lazuli_syntax::FeatureSkeleton,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for command in &feature.commands {
        for route in &command.route {
            push_unknown_semantic_type_text(
                path,
                source,
                &route.type_text,
                route.span.start,
                &mut diagnostics,
            );
        }
        if let lazuli_syntax::CommandInputDecl::Typed(slots) = &command.input {
            for slot in slots {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    &slot.type_text,
                    slot.span.start,
                    &mut diagnostics,
                );
            }
        }
        if let Some(returns) = command.returns.as_deref() {
            push_unknown_semantic_type_text(
                path,
                source,
                returns,
                command.span.start,
                &mut diagnostics,
            );
        }
        if let Some(handler) = command.handler.as_ref() {
            if let Some(returns) = handler.returns.as_deref() {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    returns,
                    command.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    for query in &feature.queries {
        match query {
            lazuli_syntax::QueryDecl::List(query) => {
                for param in &query.params {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &param.type_text,
                        param.span.start,
                        &mut diagnostics,
                    );
                }
            }
            lazuli_syntax::QueryDecl::Lookup(query) => {
                for key in &query.keys {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &key.type_text,
                        key.span.start,
                        &mut diagnostics,
                    );
                }
            }
            lazuli_syntax::QueryDecl::Sql(query) => {
                for param in &query.params {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &param.type_text,
                        param.span.start,
                        &mut diagnostics,
                    );
                }
                push_unknown_semantic_type_text(
                    path,
                    source,
                    &query.returns,
                    query.span.start,
                    &mut diagnostics,
                );
            }
            // query.compose: W2/W3 — params carry real type_text, so the
            // unknown-semantic-type text scan covers compose params + the
            // optional `returns` generated-record name.
            lazuli_syntax::QueryDecl::Compose(query) => {
                for param in &query.params {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &param.type_text,
                        param.span.start,
                        &mut diagnostics,
                    );
                }
                if let Some(returns) = &query.returns {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        returns,
                        query.span.start,
                        &mut diagnostics,
                    );
                }
            }
        }
    }

    for api in &feature.apis {
        push_unknown_semantic_type_text(
            path,
            source,
            &api.output,
            api.span.start,
            &mut diagnostics,
        );
    }

    for job in &feature.jobs {
        match &job.body {
            lazuli_syntax::JobBody::Handler(handler) => {
                if let Some(returns) = handler.returns.as_deref() {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        returns,
                        job.span.start,
                        &mut diagnostics,
                    );
                }
            }
            lazuli_syntax::JobBody::Declarative(_) => {}
            lazuli_syntax::JobBody::None => {}
        }
    }

    for webhook in &feature.webhooks {
        if let Some(handler) = webhook.handler.as_ref() {
            if let Some(returns) = handler.returns.as_deref() {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    returns,
                    webhook.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    for agent in &feature.agents {
        for slot in &agent.input {
            push_unknown_semantic_type_text(
                path,
                source,
                &slot.type_text,
                slot.span.start,
                &mut diagnostics,
            );
        }
        if let Some(output) = agent.output.as_ref() {
            match output {
                lazuli_syntax::AgentOutput::Stream(type_text)
                | lazuli_syntax::AgentOutput::Plain(type_text) => {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        type_text,
                        agent.span.start,
                        &mut diagnostics,
                    );
                }
                lazuli_syntax::AgentOutput::Discriminator(_) => {}
            }
        }
        if let Some(expose) = agent.expose.as_ref() {
            for slot in &expose.route_slots {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    &slot.type_text,
                    slot.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    diagnostics
}
