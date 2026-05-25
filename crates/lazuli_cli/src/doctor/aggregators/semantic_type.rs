//! Semantic-type and cross-feature type aggregator.
//!
//! Combines two related families of cross-feature type diagnostics:
//!
//! * `semantic_type_unknown_diagnostics_for_*` — closed catalog
//!   `@semantic.<Name>` check. Catalog: `EMAIL, PHONE, URL, UUID,
//!   DATE, CURRENCY, MONEY, JSON, GEOPOINT`. Plugin-declared semantic
//!   aliases ride the `SEMANTIC-PLUGIN-001/002` rules in
//!   `aggregators::lazurite_manifest` instead.
//! * `cross_feature_type_unresolved_diagnostics` /
//!   `feature_uses_missing_diagnostics` — walks every `TypeRef::User`
//!   site, resolves owner via the `DoctorCrossFeatureTypeIndex` lookup
//!   built from per-feature records / enums / resources, and emits
//!   the `unresolved-type` / `uses-missing` diagnostics when the
//!   referenced type belongs to a feature the current one doesn't
//!   list under `uses`.
//!
//! Helpers in this module:
//!
//! * `DoctorCrossFeatureTypeIndex` — the workspace-wide
//!   `<TypeName> → <OwnerFeature>` map plus the ambiguity tracker.
//! * `policy_ref_surface_text` — render a `PolicyRef` to the surface
//!   text the retired `scan_block_for_policy` walker captured. Lives
//!   here because `populate_feature_symbols_from_ir`
//!   (in `facts::feature_ir`) is the sole consumer; the function ties
//!   the IR-driven walker to the policy-string format that
//!   `agent_tool_policy_diagnostics` matches against.
//! * `span_line` — generic `(span → line)` helper used by the
//!   row-29 `error_vocab` / row-31 `lifecycle_gate` / row-32
//!   `route_guard` aggregators. Kept here because the semantic-type
//!   pass is the heaviest in-source consumer.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{self as ir};

use crate::doctor::scanners::leading_spaces;
use crate::doctor::{
    DoctorDiagnostic, DoctorFile, DoctorSeverity, ResourceFact, Tier3FeatureFacts,
    line_col_for_offset,
};

/// Per-site evidence row for `feature_uses_missing_diagnostics` —
/// the path + line of the `TypeRef::User` mention that triggered the
/// missing-`uses` diagnostic. Aggregated keyed by `(consumer_feature,
/// referenced_type)` so a feature with multiple sites referencing the
/// same external type only fires once.
#[derive(Debug, Clone)]
struct MissingUsesRef {
    path: PathBuf,
    line: usize,
}

const SEMANTIC_TYPE_UNKNOWN_CODE: &str = "semantic_type_unknown";

const SEMANTIC_TYPE_CATALOG: &str =
    "EMAIL, PHONE, URL, UUID, DATE, CURRENCY, MONEY, JSON, GEOPOINT";

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

pub(crate) fn push_unknown_semantic_type(
    path: &Path,
    type_ref: &lazuli_ir::TypeRef,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let Some(name) = unknown_semantic_type_name(type_ref) {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: loc.0,
            column: loc.1,
            severity: DoctorSeverity::Error,
            code: SEMANTIC_TYPE_UNKNOWN_CODE.to_owned(),
            message: format!(
                "unknown @semantic type \"{name}\"; the closed catalog is {{{SEMANTIC_TYPE_CATALOG}}}."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

pub(crate) fn push_unknown_semantic_type_text(
    path: &Path,
    source: &str,
    type_text: &str,
    offset: usize,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let loc = line_col_for_offset(source, offset);
    for name in unknown_semantic_type_names_in_text(type_text) {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: loc.0,
            column: loc.1,
            severity: DoctorSeverity::Error,
            code: SEMANTIC_TYPE_UNKNOWN_CODE.to_owned(),
            message: format!(
                "unknown @semantic type \"{name}\"; the closed catalog is {{{SEMANTIC_TYPE_CATALOG}}}."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

pub(crate) fn unknown_semantic_type_name(type_ref: &lazuli_ir::TypeRef) -> Option<&str> {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(qname)
            if qname.name.starts_with("@semantic.")
                && !is_known_semantic_type_name(qname.name.as_str()) =>
        {
            Some(qname.name.as_str())
        }
        lazuli_ir::TypeRef::Many(inner) => unknown_semantic_type_name(inner),
        _ => None,
    }
}

pub(crate) fn unknown_semantic_type_names_in_text(type_text: &str) -> Vec<&str> {
    type_text
        .split(|ch: char| !(ch == '@' || ch == '.' || ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|token| token.starts_with("@semantic.") && !is_known_semantic_type_name(token))
        .collect()
}

pub(crate) fn is_known_semantic_type_name(name: &str) -> bool {
    let Some(short) = name.strip_prefix("@semantic.") else {
        return false;
    };
    matches!(
        short,
        "Email"
            | "Phone"
            | "URL"
            | "Url"
            | "UUID"
            | "Uuid"
            | "Date"
            | "Currency"
            | "Money"
            | "JSON"
            | "Json"
            | "GeoPoint"
    )
}

pub(crate) fn span_line_col(
    source: &str,
    span: Option<&lazuli_ir::SpanRef>,
) -> Option<(usize, usize)> {
    span.map(|span| line_col_for_offset(source, span.start))
}

pub(crate) fn query_line_col(source: &str, query: &lazuli_ir::Query) -> Option<(usize, usize)> {
    match query {
        lazuli_ir::Query::List(query) => span_line_col(source, query.span_ref.as_ref()),
        lazuli_ir::Query::Lookup(query) => span_line_col(source, query.span_ref.as_ref()),
        lazuli_ir::Query::Sql(query) => span_line_col(source, query.span_ref.as_ref()),
    }
}

pub(crate) fn find_nested_type_site_line(
    source: &str,
    parent_line: usize,
    site_name: &str,
) -> Option<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let parent_index = parent_line.checked_sub(1)?;
    let parent_indent = lines
        .get(parent_index)
        .map(|line| leading_spaces(line))
        .unwrap_or(0);
    let field_prefix = format!("{site_name}:");
    let route_prefix = format!("route {site_name}:");

    for (idx, line) in lines.iter().enumerate().skip(parent_index + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= parent_indent {
            break;
        }
        if trimmed.starts_with(&field_prefix) || trimmed.starts_with(&route_prefix) {
            let column = line
                .find(site_name)
                .map(|offset| offset + 1)
                .unwrap_or(indent + 1);
            return Some((idx + 1, column));
        }
    }

    None
}

/// Render a `PolicyRef` into the same surface text the retired
/// `scan_block_for_policy` walker captured verbatim from `policy <text>`
/// child lines. `PolicyRef::None` returns `None` so the IR-driven
/// populator skips empty entries the way the walker skipped missing
/// `policy` clauses.
pub(crate) fn policy_ref_surface_text(p: &ir::PolicyRef) -> Option<String> {
    match p {
        ir::PolicyRef::Local(name) => Some(format!("@policy.{name}")),
        ir::PolicyRef::Atom(atom) => Some(atom.clone()),
        ir::PolicyRef::External { feature, name } => Some(format!("{feature}.{name}")),
        ir::PolicyRef::Unresolved(text) => Some(text.clone()),
        ir::PolicyRef::None => None,
    }
}

pub(crate) fn cross_feature_type_unresolved_diagnostics(
    files: &[DoctorFile],
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> Vec<DoctorDiagnostic> {
    let declared_types = build_cross_feature_declared_type_index(tier3_facts, feature_resources);
    let mut diagnostics = Vec::new();
    let mut reported: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();

    for (feature_name, resources) in feature_resources {
        for (resource_name, resource) in resources {
            for (field_name, field) in &resource.fields {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &field.type_ref,
                    &resource.path,
                    field.line.max(resource.line).max(1),
                    format!("{feature_name}.{resource_name}.{field_name}"),
                );
            }
        }
    }

    for fact in tier3_facts {
        for record in &fact.records {
            for field in &record.fields {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &field.type_ref,
                    &fact.path,
                    span_line(files, &fact.path, field.span_ref, fact.feature_line),
                    format!("{}.{}.{}", fact.feature, record.name, field.name),
                );
            }
        }

        for command in &fact.commands {
            let command_line = fact
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, command.span_ref, fact.feature_line)
                });

            for slot in &command.route {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &slot.type_ref,
                    &fact.path,
                    command_line.max(1),
                    format!("{}.{}.route.{}", fact.feature, command.name, slot.name),
                );
            }

            if let ir::CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    push_unresolved_type_ref_diagnostic(
                        &mut diagnostics,
                        &mut reported,
                        &declared_types,
                        &slot.type_ref,
                        &fact.path,
                        command_line.max(1),
                        format!("{}.{}.input.{}", fact.feature, command.name, slot.name),
                    );
                }
            }

            if let ir::CommandEffect::Returns(returns) = &command.effect {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &returns.return_type,
                    &fact.path,
                    command_line.max(1),
                    format!("{}.{}.returns", fact.feature, command.name),
                );
            }
        }
    }

    diagnostics
}

pub(crate) fn build_cross_feature_declared_type_index(
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> BTreeSet<String> {
    let mut declared = BTreeSet::new();

    for resources in feature_resources.values() {
        declared.extend(resources.keys().cloned());
    }
    for fact in tier3_facts {
        declared.extend(fact.records.iter().map(|record| record.name.clone()));
        declared.extend(fact.enums.iter().map(|enum_decl| enum_decl.name.clone()));
    }

    declared
}

pub(crate) fn push_unresolved_type_ref_diagnostic(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    reported: &mut BTreeSet<(PathBuf, String, String)>,
    declared_types: &BTreeSet<String>,
    type_ref: &ir::TypeRef,
    path: &Path,
    line: usize,
    site: String,
) {
    let Some(name) = unresolved_bare_user_type_name(type_ref, declared_types) else {
        return;
    };
    if !reported.insert((path.to_path_buf(), site.clone(), name.to_owned())) {
        return;
    }

    diagnostics.push(DoctorDiagnostic {
        path: path.to_path_buf(),
        line,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "cross_feature_type_unresolved".to_owned(),
        message: format!(
            "type `{name}` referenced by `{site}` is not declared in any feature. Add a `resource`/`record`/`enum {name}` block, or check for a typo."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    });
}

pub(crate) fn unresolved_bare_user_type_name<'a>(
    type_ref: &'a ir::TypeRef,
    declared_types: &BTreeSet<String>,
) -> Option<&'a str> {
    match type_ref {
        ir::TypeRef::UserDefined(qname) | ir::TypeRef::EnumRef(qname)
            if qname.feature.is_none()
                && !declared_types.contains(&qname.name)
                && !is_trivial_type_ref_name(&qname.name) =>
        {
            Some(qname.name.as_str())
        }
        _ => None,
    }
}

pub(crate) fn is_trivial_type_ref_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.len() <= 1 || trimmed.starts_with('@')
}

pub(crate) fn span_line(
    files: &[DoctorFile],
    path: &Path,
    span_ref: Option<lazuli_ir::SpanRef>,
    fallback: usize,
) -> usize {
    span_ref
        .and_then(|span| {
            files
                .iter()
                .find(|file| file.path.as_path() == path)
                .map(|file| line_col_for_offset(&file.source, span.start).0)
        })
        .unwrap_or(fallback.max(1))
}

pub(crate) fn feature_uses_missing_diagnostics(
    files: &[DoctorFile],
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DoctorDiagnostic> {
    let type_owners = DoctorCrossFeatureTypeIndex::build(tier3_facts, feature_resources);
    let mut missing: BTreeMap<(String, String), MissingUsesRef> = BTreeMap::new();

    for (feature_name, resources) in feature_resources {
        for resource in resources.values() {
            for field in resource.fields.values() {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    feature_name,
                    &field.type_ref,
                    &resource.path,
                    field.line.max(resource.line).max(1),
                );
            }
        }
    }

    for fact in tier3_facts {
        for record in &fact.records {
            for field in &record.fields {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &field.type_ref,
                    &fact.path,
                    span_line(files, &fact.path, field.span_ref, fact.feature_line),
                );
            }
        }

        for command in &fact.commands {
            let command_line = fact
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, command.span_ref, fact.feature_line)
                })
                .max(1);

            for slot in &command.route {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &slot.type_ref,
                    &fact.path,
                    command_line,
                );
            }

            if let ir::CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    record_missing_uses_ref(
                        &mut missing,
                        &type_owners,
                        &fact.feature,
                        &slot.type_ref,
                        &fact.path,
                        command_line,
                    );
                }
            }

            if let ir::CommandEffect::Returns(returns) = &command.effect {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &returns.return_type,
                    &fact.path,
                    command_line,
                );
            }
        }

        for query in &fact.queries {
            let query_line = fact
                .query_lines
                .get(query.name())
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, query_span_ref(query), fact.feature_line)
                })
                .max(1);
            for slot in query_params(query) {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &slot.type_ref,
                    &fact.path,
                    query_line,
                );
            }
        }
    }

    missing
        .into_iter()
        .filter(|((feature, dependency), _)| {
            !feature_uses
                .get(feature)
                .map(|uses| uses.contains(dependency))
                .unwrap_or(false)
        })
        .map(|((feature, dependency), site)| DoctorDiagnostic {
            path: site.path,
            line: site.line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "feature_uses_missing".to_owned(),
            message: format!(
                "feature `{feature}` references types declared in feature `{dependency}` but does not declare `uses {dependency}` in its header. Add `uses {dependency}` to make the dependency explicit."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

pub(crate) fn record_missing_uses_ref(
    missing: &mut BTreeMap<(String, String), MissingUsesRef>,
    type_owners: &DoctorCrossFeatureTypeIndex,
    feature: &str,
    type_ref: &ir::TypeRef,
    path: &Path,
    line: usize,
) {
    let mut owners = BTreeSet::new();
    collect_cross_feature_type_ref_owners(&mut owners, type_owners, feature, type_ref);
    for owner in owners {
        missing
            .entry((feature.to_owned(), owner))
            .or_insert_with(|| MissingUsesRef {
                path: path.to_path_buf(),
                line: line.max(1),
            });
    }
}

pub(crate) fn collect_cross_feature_type_ref_owners(
    owners: &mut BTreeSet<String>,
    type_owners: &DoctorCrossFeatureTypeIndex,
    feature: &str,
    type_ref: &ir::TypeRef,
) {
    match type_ref {
        ir::TypeRef::UserDefined(qname) | ir::TypeRef::EnumRef(qname) => {
            if qname.name.starts_with('@') {
                return;
            }
            let owner = qname
                .feature
                .as_deref()
                .or_else(|| type_owners.owner(&qname.name));
            if let Some(owner) = owner {
                if owner != feature {
                    owners.insert(owner.to_owned());
                }
            }
        }
        ir::TypeRef::Many(inner) => {
            collect_cross_feature_type_ref_owners(owners, type_owners, feature, inner);
        }
        ir::TypeRef::Unresolved(name) => {
            // Command/query lowering still carries authored custom
            // types as `Unresolved`; the codegen cross-feature pass
            // resolves these names against the same owner index.
            if name.starts_with('@') {
                return;
            }
            if let Some(owner) = type_owners.owner(name) {
                if owner != feature {
                    owners.insert(owner.to_owned());
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn query_params(query: &ir::Query) -> &[ir::TypedSlot] {
    match query {
        ir::Query::List(query) => &query.params,
        ir::Query::Lookup(query) => &query.params,
        ir::Query::Sql(query) => &query.params,
    }
}

pub(crate) fn query_span_ref(query: &ir::Query) -> Option<ir::SpanRef> {
    match query {
        ir::Query::List(query) => query.span_ref,
        ir::Query::Lookup(query) => query.span_ref,
        ir::Query::Sql(query) => query.span_ref,
    }
}

#[derive(Debug, Clone, Default)]
struct DoctorCrossFeatureTypeIndex {
    map: BTreeMap<String, String>,
    ambiguous: BTreeMap<String, BTreeSet<String>>,
}

impl DoctorCrossFeatureTypeIndex {
    fn build(
        tier3_facts: &[Tier3FeatureFacts],
        feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    ) -> Self {
        let mut index = Self::default();

        for (feature, resources) in feature_resources {
            for resource_name in resources.keys() {
                index.register(resource_name, feature);
            }
        }
        for fact in tier3_facts {
            for record in &fact.records {
                index.register(&record.name, &fact.feature);
            }
            for enum_decl in &fact.enums {
                index.register(&enum_decl.name, &fact.feature);
            }
        }

        index
    }

    fn register(&mut self, name: &str, feature: &str) {
        if let Some(owners) = self.ambiguous.get_mut(name) {
            owners.insert(feature.to_owned());
            return;
        }

        match self.map.remove(name) {
            Some(existing) if existing == feature => {
                self.map.insert(name.to_owned(), existing);
            }
            Some(existing) => {
                let mut owners = BTreeSet::new();
                owners.insert(existing);
                owners.insert(feature.to_owned());
                self.ambiguous.insert(name.to_owned(), owners);
            }
            None => {
                self.map.insert(name.to_owned(), feature.to_owned());
            }
        }
    }

    fn owner(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }
}
