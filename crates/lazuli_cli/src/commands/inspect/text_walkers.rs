//! Shared text-walking primitives consumed by every per-axis projection.
//!
//! Phase L Tier 4 and later moved most of inspect's heavy lifting onto
//! the lifted IR (via the `Tier3FeatureSlice` map), but the projections
//! still walk the trimmed source lines for the facts that don't yet
//! survive lower-pass elaboration: route slots, transition predicates,
//! command effects, security markers, audit emit targets, et cetera.
//!
//! The helpers here are the indent-aware block partitioners
//! (`top_level_blocks`, `command_blocks`, `query_blocks`), the
//! direct-child accessors (`direct_child_value`, `direct_child_values`,
//! `block_scalar_value`, `block_prefixed_value`, `block_has_exact_line`),
//! the name extractors (`command_name`, `query_name`,
//! `named_top_block_name`, `field_name_from_typed_line`), the
//! line-classifier predicates (`is_transition_line`,
//! `view_anchor_line`, `transition_name`, `transition_requires`,
//! `inspect_subject`, `test_group`), the small expression helpers
//! (`strip_quotes`, `typed_declaration`, `trailing_scalar_value_after`,
//! `parse_event_list`, `qualify_event_ref`,
//! `emits_derived_effect`), the dependency-shape constructors
//! (`inspect_binding`, `inspect_dependency`,
//! `emits_dependencies`, `query_reference_dependencies`), the
//! name collectors (`collect_resource_names`, `collect_record_names`,
//! `collect_query_names`, `collect_command_names`,
//! `collect_workflow_summaries`, `collect_named_top_blocks`,
//! `collect_event_names`, `collect_surface_names`,
//! `collect_view_anchors`, `collect_extends_anchors`,
//! `collect_extensible_by_features`, `collect_job_and_webhook_names`),
//! the audit / policy / security shaping helpers (`parse_audit`,
//! `resolve_policy_atoms`, `security_markers`, `full_marker_reference`),
//! and the command/query body inspectors (`command_route_names`,
//! `command_input_names`, `command_needs_inferred_target`,
//! `query_param_names`, `query_kind`, `feature_has_id_lookup` via the
//! `expand` sibling).
//!
//! Every public-to-sibling-module helper is `pub(super)`; nothing
//! escapes the `inspect` subtree.

use std::collections::{BTreeMap, BTreeSet};

use super::expand::{
    collect_event_decls, is_identifier, leading_spaces, namespace_references, parse_ident_list,
};
use super::{
    InspectAudit, InspectBinding, InspectDependency, InspectWorkflowSummary,
};

// -----------------------------------------------------------------------------
// Block partitioners
// -----------------------------------------------------------------------------

pub(super) fn top_level_blocks<'a>(lines: &'a [String], prefix: &str) -> Vec<&'a [String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with(prefix) {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

pub(super) fn query_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 4 && lines[index].trim_start().starts_with("query.") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) <= 4 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

pub(super) fn command_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

// -----------------------------------------------------------------------------
// Name extractors
// -----------------------------------------------------------------------------

pub(super) fn query_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()?.starts_with("query.") {
        parts.next()
    } else {
        None
    }
}

pub(super) fn query_kind(block: &[String]) -> &'static str {
    let header = block[0].trim_start();
    let qualifier = header.strip_prefix("query.").unwrap_or("");
    match qualifier.split_whitespace().next().unwrap_or("") {
        "lookup" => "lookup",
        "sql" => "sql",
        _ => "list",
    }
}

pub(super) fn named_top_block_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_whitespace().nth(1)
}

pub(super) fn command_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "command" {
        parts.next()
    } else {
        None
    }
}

pub(super) fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

pub(super) fn field_name_from_typed_line(trimmed_line: &str) -> Option<&str> {
    let (head, _) = trimmed_line.split_once(':')?;
    let name = head.trim().split_whitespace().next()?;

    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Some(name)
    } else {
        None
    }
}

// -----------------------------------------------------------------------------
// Direct-child accessors
// -----------------------------------------------------------------------------

pub(super) fn direct_child_value(lines: &[String], prefix: &str) -> Option<String> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;

    lines.iter().find_map(|line| {
        if leading_spaces(line) == child_indent {
            line.trim_start().strip_prefix(prefix).map(str::to_owned)
        } else {
            None
        }
    })
}

pub(super) fn direct_child_values(lines: &[String], prefix: &str) -> Vec<String> {
    let Some(child_indent) = lines.first().map(|line| leading_spaces(line) + 2) else {
        return Vec::new();
    };

    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == child_indent {
                line.trim_start()
                    .strip_prefix(prefix)
                    .map(str::trim)
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn block_scalar_value<'a>(lines: &'a [String], keyword: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(keyword))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn block_prefixed_value<'a>(lines: &'a [String], prefix: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(prefix))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn block_has_exact_line(lines: &[String], expected: &str) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == expected)
}

// -----------------------------------------------------------------------------
// Small expression helpers
// -----------------------------------------------------------------------------

pub(super) fn strip_quotes(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

pub(super) fn typed_declaration(trimmed_line: &str) -> Option<(&str, &str)> {
    let (name, rest) = trimmed_line.split_once(':')?;
    let name = name.trim();
    let ty = rest.trim().split_whitespace().next()?;

    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

pub(super) fn trailing_scalar_value_after<'a>(
    trimmed_line: &'a str,
    keyword: &str,
) -> Option<&'a str> {
    let mut tokens = trimmed_line.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == keyword {
            return tokens.next();
        }
    }
    None
}

pub(super) fn parse_event_list(source: &str) -> Vec<String> {
    let first = source.split_whitespace().next().unwrap_or(source);
    first
        .split(',')
        .map(str::trim)
        .filter(|event| {
            !event.is_empty()
                && event
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
        })
        .map(str::to_owned)
        .collect()
}

pub(super) fn qualify_event_ref(feature: &str, event: &str) -> String {
    if event.contains('.') {
        event.to_owned()
    } else {
        format!("{feature}.{event}")
    }
}

pub(super) fn emits_derived_effect(emits_rest: &str) -> Option<&'static str> {
    let mut tokens = emits_rest.split_whitespace();
    tokens.next()?;
    if tokens.next()? != "from" {
        return None;
    }
    match tokens.next()? {
        "creates" => Some("creates"),
        "updates" => Some("updates"),
        "deletes" => Some("deletes"),
        _ => None,
    }
}

// -----------------------------------------------------------------------------
// Line classifier predicates
// -----------------------------------------------------------------------------

pub(super) fn transition_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_once(':')?.0.split_whitespace().next()
}

pub(super) fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((left, right)) = trimmed_line.split_once(':') else {
        return false;
    };

    !left.trim().is_empty() && right.contains("->")
}

pub(super) fn transition_requires(trimmed_line: &str) -> Option<String> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let (_, after_arrow) = rhs.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    tokens.next()?;

    while let Some(token) = tokens.next() {
        if token == "requires" {
            return tokens.next().map(str::to_owned);
        }
    }

    None
}

pub(super) fn inspect_subject(trimmed_line: &str) -> Option<String> {
    if let Some(name) = command_name(trimmed_line) {
        Some(format!("command.{name}"))
    } else if trimmed_line.starts_with("rule ") {
        Some(format!(
            "rule.{}",
            trimmed_line
                .trim_start_matches("rule ")
                .trim_matches('"')
                .to_owned()
        ))
    } else if view_anchor_line(trimmed_line) {
        trimmed_line
            .split(" id ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .map(|anchor| format!("view.{anchor}"))
            .or_else(|| Some("view.anchor".to_owned()))
    } else if is_transition_line(trimmed_line) {
        transition_name(trimmed_line).map(|name| format!("transition.{name}"))
    } else {
        None
    }
}

pub(super) fn view_anchor_line(trimmed_line: &str) -> bool {
    trimmed_line.starts_with("view ") && trimmed_line.contains(" id @anchor.")
}

pub(super) fn test_group(assertion: &str) -> &'static str {
    if assertion.starts_with("permits @")
        || assertion.starts_with("forbids @")
        || assertion.contains(" as @")
    {
        "authz"
    } else if assertion.contains(" from ") {
        "transition"
    } else if assertion.contains(" when ") {
        "predicate"
    } else if assertion.starts_with("accepted by ") || assertion.starts_with("rejected by ") {
        "anchor"
    } else {
        "other"
    }
}

// -----------------------------------------------------------------------------
// Dependency-shape constructors
// -----------------------------------------------------------------------------

pub(super) fn inspect_binding(
    name: impl Into<String>,
    origin: impl Into<String>,
    meaning: impl Into<String>,
) -> InspectBinding {
    InspectBinding {
        name: name.into(),
        origin: origin.into(),
        meaning: meaning.into(),
    }
}

pub(super) fn inspect_dependency(
    kind: impl Into<String>,
    from: impl Into<String>,
    to: impl Into<String>,
    origin: impl Into<String>,
) -> InspectDependency {
    InspectDependency {
        kind: kind.into(),
        from: from.into(),
        to: to.into(),
        origin: origin.into(),
    }
}

pub(super) fn emits_dependencies(
    feature: &str,
    subject: &str,
    lines: &[String],
) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        if let Some(events) = trimmed.strip_prefix("emits ") {
            let origin = if emits_derived_effect(events).is_some() {
                "emits.derived"
            } else {
                "emits"
            };
            for event in parse_event_list(events) {
                dependencies.push(inspect_dependency(
                    "emits_event",
                    subject,
                    qualify_event_ref(feature, &event),
                    origin,
                ));
            }
        } else if is_transition_line(trimmed) {
            if let Some(event) = trailing_scalar_value_after(trimmed, "emits") {
                dependencies.push(inspect_dependency(
                    "emits_event",
                    subject,
                    qualify_event_ref(feature, event),
                    "transition.emits",
                ));
            }
        }
    }

    dependencies
}

pub(super) fn query_reference_dependencies(
    subject: &str,
    lines: &[String],
) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        for prefix in ["target ", "source "] {
            if let Some(value) = trimmed.strip_prefix(prefix) {
                if let Some(query) = value
                    .split_once('(')
                    .map(|(query, _)| query)
                    .or_else(|| value.split_whitespace().next())
                    .filter(|query| query.contains("query."))
                {
                    dependencies.push(inspect_dependency(
                        "query_ref",
                        subject,
                        query.trim(),
                        prefix.trim(),
                    ));
                }
            }
        }
    }

    dependencies
}

// -----------------------------------------------------------------------------
// Command / query body inspectors
// -----------------------------------------------------------------------------

pub(super) fn command_needs_inferred_target(lines: &[String]) -> bool {
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    has_route_id && mutates_existing
}

pub(super) fn query_param_names(lines: &[String]) -> Vec<String> {
    let mut params = Vec::new();
    let mut in_params = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 6 {
            in_params = trimmed == "params";
            continue;
        }

        if in_params && leading_spaces(line) == 8 {
            if let Some((name, _)) = typed_declaration(trimmed) {
                params.push(name.to_owned());
            }
        } else if leading_spaces(line) <= 6 {
            in_params = false;
        }
    }

    if params.is_empty() {
        if let Some(key) = lines
            .first()
            .and_then(|line| line.trim_start().split(" by ").nth(1))
            .and_then(|rest| rest.split_once(':').map(|(name, _)| name.trim()))
            .filter(|name| !name.is_empty())
        {
            params.push(key.to_owned());
        }
    }

    params
}

pub(super) fn command_route_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == 4 {
                let trimmed = line.trim_start();
                let mut parts = trimmed.split_whitespace();
                if parts.next()? == "route" {
                    return parts
                        .next()
                        .map(|name| name.trim_end_matches(':').to_owned());
                }
            }
            None
        })
        .collect()
}

pub(super) fn command_input_names(lines: &[String]) -> Vec<String> {
    let mut inputs = Vec::new();
    let mut in_input = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 {
            in_input = trimmed == "input";

            if let Some(rest) = trimmed.strip_prefix("input ") {
                inputs.extend(parse_ident_list(rest));
            }
            continue;
        }

        if in_input && leading_spaces(line) == 6 {
            if let Some((name, _)) = typed_declaration(trimmed) {
                inputs.push(name.to_owned());
            }
        } else if leading_spaces(line) <= 4 {
            in_input = false;
        }
    }

    inputs
}

// -----------------------------------------------------------------------------
// Audit / policy / security shaping helpers
// -----------------------------------------------------------------------------

pub(super) fn parse_audit(lines: &[String], origin: &'static str) -> Option<InspectAudit> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;
    let audit_grandchild_indent = child_indent + 2;

    let mut hit_index: Option<usize> = None;
    let mut audit: Option<InspectAudit> = None;
    for (offset, line) in lines.iter().enumerate().skip(1) {
        if leading_spaces(line) != child_indent {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed == "audit" {
            audit = Some(InspectAudit {
                fields: Vec::new(),
                emit_to: None,
                origin,
            });
            hit_index = Some(offset);
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("audit ") {
            let rest = rest.trim();
            if rest == "none" {
                return None;
            }
            let fields: Vec<String> = rest
                .split(',')
                .map(|part| part.trim().to_owned())
                .filter(|part| !part.is_empty())
                .collect();
            audit = Some(InspectAudit {
                fields,
                emit_to: None,
                origin,
            });
            hit_index = Some(offset);
            break;
        }
    }

    // Observability bucket cycle row 37 — scan grandchildren of the
    // `audit` line for an `emit_to <target>` slot. The slot lives one
    // indent step deeper than `audit` and stops at the next
    // sibling-or-shallower line.
    if let (Some(start), Some(audit_value)) = (hit_index, audit.as_mut()) {
        for line in lines.iter().skip(start + 1) {
            let leading = leading_spaces(line);
            if leading <= child_indent {
                break;
            }
            if leading != audit_grandchild_indent {
                continue;
            }
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("emit_to ") {
                audit_value.emit_to = Some(rest.trim().to_owned());
                break;
            }
        }
    }

    audit
}

pub(super) fn resolve_policy_atoms(
    policy: &str,
    policies: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let policy = policy.strip_prefix("@policy.").unwrap_or(policy);
    policies
        .get(policy)
        .cloned()
        .unwrap_or_else(|| vec![policy.to_owned()])
}

pub(super) fn security_markers(line: &str) -> impl Iterator<Item = String> + '_ {
    namespace_references(line)
        .into_iter()
        .filter(|namespace| matches!(*namespace, "pii" | "cap" | "key"))
        .filter_map(|namespace| full_marker_reference(line, namespace))
}

fn full_marker_reference(line: &str, namespace: &str) -> Option<String> {
    let start = line.find(&format!("@{namespace}."))?;
    let after = &line[start..];
    let mut end = after
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'_' | b'.')))
        .unwrap_or(after.len());

    if after.as_bytes().get(end) == Some(&b'(') {
        end = after[end..]
            .find(')')
            .map(|relative| end + relative + 1)
            .unwrap_or(after.len());
    }

    Some(after[..end].to_owned())
}

// -----------------------------------------------------------------------------
// Name collectors
// -----------------------------------------------------------------------------

pub(super) fn collect_resource_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("resource ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_record_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("record ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_query_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("query.") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_command_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
                command_name(trimmed).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_workflow_summaries(lines: &[String]) -> Vec<InspectWorkflowSummary> {
    let mut workflows = Vec::new();
    let mut current: Option<InspectWorkflowSummary> = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            if let Some(workflow) = current.take() {
                workflows.push(workflow);
            }

            current = if trimmed.starts_with("workflow ") {
                trimmed
                    .split_whitespace()
                    .nth(1)
                    .map(|name| InspectWorkflowSummary {
                        name: name.to_owned(),
                        transitions: Vec::new(),
                    })
            } else {
                None
            };
            continue;
        }

        if leading_spaces(line) == 4 && is_transition_line(trimmed) {
            if let Some(workflow) = current.as_mut() {
                if let Some(transition) = transition_name(trimmed) {
                    workflow.transitions.push(transition.to_owned());
                }
            }
        }
    }

    if let Some(workflow) = current {
        workflows.push(workflow);
    }

    workflows
}

pub(super) fn collect_named_top_blocks(lines: &[String], keyword: &str) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with(keyword) {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_event_names(lines: &[String]) -> Vec<String> {
    collect_event_decls(lines)
        .into_iter()
        .map(|event| event.name)
        .collect()
}

pub(super) fn collect_surface_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("surface ") {
                let parts: Vec<_> = trimmed.split_whitespace().skip(1).collect();
                (!parts.is_empty()).then(|| parts.join("/"))
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_view_anchors(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let (_, anchor) = trimmed.split_once(" id @anchor.")?;
            let name = anchor.split_whitespace().next()?;
            Some(format!("@anchor.{name}"))
        })
        .collect()
}

pub(super) fn collect_extends_anchors(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("extends @anchor.") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn collect_extensible_by_features(lines: &[String]) -> Vec<String> {
    let mut features = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 6 && trimmed.starts_with("extensible_by ") {
            features.extend(
                trimmed
                    .trim_start_matches("extensible_by ")
                    .split(',')
                    .map(str::trim)
                    .filter(|feature| !feature.is_empty())
                    .map(str::to_owned),
            );
        }
    }

    features
}

pub(super) fn collect_job_and_webhook_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2
                && (trimmed.starts_with("job ") || trimmed.starts_with("webhook "))
            {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Refs (declared / used) collectors
// -----------------------------------------------------------------------------

pub(super) fn collect_declared_ref_groups(lines: &[String]) -> Vec<super::InspectRefGroup> {
    let mut groups = Vec::new();
    let mut in_refs = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_refs = trimmed == "refs";
            continue;
        }

        if !in_refs || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        let Some((group, namespaces)) = trimmed.split_once(':') else {
            continue;
        };

        groups.push(super::InspectRefGroup {
            group: group.trim().to_owned(),
            namespaces: namespaces
                .split(',')
                .map(str::trim)
                .filter(|namespace| namespace.starts_with('@') && !namespace.is_empty())
                .map(str::to_owned)
                .collect(),
            origin: "authored",
        });
    }

    groups
}

pub(super) fn collect_used_namespaces(lines: &[String]) -> BTreeSet<String> {
    let mut namespaces = BTreeSet::new();
    let mut current_top = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
        }

        if current_top == Some("refs") || trimmed.starts_with('#') {
            continue;
        }

        for namespace in namespace_references(line) {
            namespaces.insert(format!("@{namespace}"));
        }
    }

    namespaces
}
