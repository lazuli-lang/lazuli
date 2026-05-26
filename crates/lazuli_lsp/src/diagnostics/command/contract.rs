//! Per-command structural contract check.
//!
//! Walks the source once, accumulating [`CanonicalCommandFacts`] per
//! `command` block, and flushes via [`command_diagnostics`] when the
//! block ends. Covers policy declaration, route slot resolution,
//! short-input field inference, and `creates ... from input`
//! consumption. Pure parsers (`command_name`, `command_route_slot`,
//! `command_write_effect`, `command_short_input_fields`,
//! `route_references`, `input_references`) live here too because every
//! consumer is in this file.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::Diagnostic;

use crate::{
    CanonicalFeatureFacts, collect_canonical_feature_facts, feature_name, leading_spaces,
    typed_param,
};

use super::builders::{
    command_default_route_diagnostic, command_from_input_unconsumed_diagnostic,
    command_policy_diagnostic, command_route_diagnostic,
    command_short_input_ambiguous_resource_diagnostic, command_short_input_diagnostic,
    command_short_input_without_resource_diagnostic,
};

#[derive(Debug)]
pub(crate) struct CanonicalCommandFacts {
    feature_name: Option<String>,
    name: String,
    line_index: usize,
    line: String,
    route_slots: HashSet<String>,
    route_references: Vec<CommandRouteReference>,
    short_inputs: Vec<CommandShortInput>,
    typed_inputs: Vec<CommandShortInput>,
    input_inference_resources: Vec<String>,
    from_input_creates: Option<(String, usize, String)>,
    create_assignment_references: HashSet<String>,
    has_policy: bool,
    has_target: bool,
    has_write_effect: bool,
    needs_default_route_target: bool,
}

impl CanonicalCommandFacts {
    fn new(feature_name: Option<String>, name: String, line_index: usize, line: &str) -> Self {
        Self {
            feature_name,
            name,
            line_index,
            line: line.to_owned(),
            route_slots: HashSet::new(),
            route_references: Vec::new(),
            short_inputs: Vec::new(),
            typed_inputs: Vec::new(),
            input_inference_resources: Vec::new(),
            from_input_creates: None,
            create_assignment_references: HashSet::new(),
            has_policy: false,
            has_target: false,
            has_write_effect: false,
            needs_default_route_target: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct CommandRouteReference {
    name: String,
    line_index: usize,
    line: String,
}

#[derive(Debug)]
pub(crate) struct CommandShortInput {
    name: String,
    line_index: usize,
    line: String,
}

pub(crate) fn command_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let features = collect_canonical_feature_facts(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_command: Option<CanonicalCommandFacts> = None;
    let mut current_command_child: Option<&str> = None;
    let mut current_create_from_input = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command, &features));
            }

            current_feature = Some(feature_name(trimmed));
            current_command_child = None;
            current_create_from_input = false;
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command, &features));
            }

            current_command = Some(CanonicalCommandFacts::new(
                current_feature.clone(),
                command_name(trimmed),
                line_index,
                line,
            ));
            current_command_child = None;
            current_create_from_input = false;
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_diagnostics(command, &features));
            }
            current_command_child = None;
            current_create_from_input = false;
            continue;
        }

        let Some(command) = current_command.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if let Some(route_slot) = command_route_slot(trimmed) {
                command.route_slots.insert(route_slot.to_owned());
            }

            if trimmed.starts_with("policy ") {
                command.has_policy = true;
                current_command_child = None;
                current_create_from_input = false;
            } else if trimmed.starts_with("target ") {
                command.has_target = true;
                current_command_child = None;
                current_create_from_input = false;
            } else if let Some(input_fields) = command_short_input_fields(trimmed) {
                command
                    .short_inputs
                    .extend(
                        input_fields
                            .into_iter()
                            .map(|field_name| CommandShortInput {
                                name: field_name,
                                line_index,
                                line: line.to_owned(),
                            }),
                    );
                current_command_child = None;
                current_create_from_input = false;
            } else if trimmed == "input" {
                current_command_child = Some("input");
                current_create_from_input = false;
            } else if let Some((effect, resource_name)) = command_write_effect(trimmed) {
                command.has_write_effect = true;
                command.needs_default_route_target = matches!(effect, "updates" | "deletes");
                if matches!(effect, "creates" | "updates") {
                    command
                        .input_inference_resources
                        .push(resource_name.to_owned());
                }
                current_create_from_input = false;
                current_command_child = None;
                if effect == "creates" && trimmed.contains(" from input") {
                    command.from_input_creates =
                        Some((resource_name.to_owned(), line_index, line.to_owned()));
                    current_command_child = Some("creates");
                    current_create_from_input = true;
                }
            } else {
                current_command_child = None;
                current_create_from_input = false;
            }
        } else if leading_spaces(line) == 6 {
            if current_command_child == Some("input") {
                if let Some((name, _)) = typed_param(trimmed) {
                    command.typed_inputs.push(CommandShortInput {
                        name: name.to_owned(),
                        line_index,
                        line: line.to_owned(),
                    });
                }
            } else if current_command_child == Some("creates") && current_create_from_input {
                for input_name in input_references(line) {
                    command.create_assignment_references.insert(input_name);
                }
            }
        }

        for route_reference in route_references(line) {
            command.route_references.push(CommandRouteReference {
                name: route_reference,
                line_index,
                line: line.to_owned(),
            });
        }
    }

    if let Some(command) = current_command {
        diagnostics.extend(command_diagnostics(command, &features));
    }

    diagnostics
}

pub(crate) fn command_diagnostics(
    command: CanonicalCommandFacts,
    features: &HashMap<String, CanonicalFeatureFacts>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !command.has_policy {
        diagnostics.push(command_policy_diagnostic(
            command.line_index,
            &command.line,
            &command.name,
        ));
    }

    for reference in command.route_references {
        if !command.route_slots.contains(&reference.name) {
            diagnostics.push(command_route_diagnostic(
                reference.line_index,
                &reference.line,
                &command.name,
                &reference.name,
            ));
        }
    }

    if command.has_write_effect
        && command.needs_default_route_target
        && !command.has_target
        && !command.route_slots.contains("id")
    {
        diagnostics.push(command_default_route_diagnostic(
            command.line_index,
            &command.line,
            &command.name,
        ));
    }

    if !command.short_inputs.is_empty() {
        let local_feature = command
            .feature_name
            .as_deref()
            .and_then(|feature_name| features.get(feature_name));
        let inference_resources: Vec<_> = command
            .input_inference_resources
            .iter()
            .filter_map(|resource_name| {
                local_feature
                    .and_then(|feature| feature.resources.get(resource_name))
                    .map(|resource| (resource_name.as_str(), resource))
            })
            .collect();

        if command.input_inference_resources.len() > 1 || inference_resources.len() > 1 {
            for input in &command.short_inputs {
                diagnostics.push(command_short_input_ambiguous_resource_diagnostic(
                    input.line_index,
                    &input.line,
                    &command.name,
                    &input.name,
                ));
            }

            return diagnostics;
        }

        if inference_resources.is_empty() {
            for input in &command.short_inputs {
                diagnostics.push(command_short_input_without_resource_diagnostic(
                    input.line_index,
                    &input.line,
                    &command.name,
                    &input.name,
                ));
            }

            return diagnostics;
        }

        let (resource_name, resource) = inference_resources[0];

        for input in &command.short_inputs {
            if !resource.fields.contains(&input.name) {
                diagnostics.push(command_short_input_diagnostic(
                    input.line_index,
                    &input.line,
                    &command.name,
                    &input.name,
                    resource_name,
                ));
            }
        }
    }

    if let Some((resource_name, _, _)) = command.from_input_creates.as_ref() {
        let local_feature = command
            .feature_name
            .as_deref()
            .and_then(|feature_name| features.get(feature_name));
        let resource = local_feature.and_then(|feature| feature.resources.get(resource_name));
        let all_inputs = command
            .short_inputs
            .iter()
            .chain(command.typed_inputs.iter());

        if let Some(resource) = resource {
            for input in all_inputs {
                if !resource.fields.contains(&input.name)
                    && !command.create_assignment_references.contains(&input.name)
                {
                    diagnostics.push(command_from_input_unconsumed_diagnostic(
                        input.line_index,
                        &input.line,
                        &command.name,
                        &input.name,
                        resource_name,
                    ));
                }
            }
        }
    }

    diagnostics
}

pub(crate) fn command_name(trimmed_line: &str) -> String {
    trimmed_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("<anonymous>")
        .to_owned()
}

pub(crate) fn command_route_slot(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? != "route" {
        return None;
    }

    Some(parts.next()?.trim_end_matches(':'))
}

pub(crate) fn command_write_effect(trimmed_line: &str) -> Option<(&str, &str)> {
    let mut parts = trimmed_line.split_whitespace();
    let effect = parts.next()?;
    if matches!(effect, "creates" | "updates" | "deletes") {
        Some((effect, parts.next()?))
    } else {
        None
    }
}

pub(crate) fn command_short_input_fields(trimmed_line: &str) -> Option<Vec<String>> {
    let rest = trimmed_line.strip_prefix("input ")?;
    let fields: Vec<String> = rest
        .split(',')
        .map(str::trim)
        .filter(|field| {
            !field.is_empty()
                && !field.contains(':')
                && field
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(str::to_owned)
        .collect();

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub(crate) fn route_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("route.") {
        let after_prefix = &rest[start + "route.".len()..];
        let end = after_prefix
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(after_prefix.len());
        let name = &after_prefix[..end];

        if !name.is_empty() {
            references.push(name.to_owned());
        }

        rest = &after_prefix[end..];
    }

    references
}

pub(crate) fn input_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("input.") {
        let after_prefix = &rest[start + "input.".len()..];
        let end = after_prefix
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(after_prefix.len());
        let name = &after_prefix[..end];

        if !name.is_empty() {
            references.push(name.to_owned());
        }

        rest = &after_prefix[end..];
    }

    references
}
