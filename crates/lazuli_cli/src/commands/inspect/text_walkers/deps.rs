//! Dependency-shape constructors and command/query body inspectors.
//!
//! `inspect_binding` and `inspect_dependency` are the typed
//! constructors every projection uses to materialize
//! `InspectBinding` / `InspectDependency` rows. They exist so call
//! sites never reach for the struct literal directly — keeps the
//! tagging discipline (origin strings, ordering) in one place.
//!
//! `emits_dependencies` and `query_reference_dependencies` are the
//! two recurring graph extractors: the first walks `emits …` lines
//! (top-level and inside `transition ... emits X` tails); the second
//! walks `target query.x` / `source query.x` slots inside command
//! and webhook bodies.
//!
//! `command_needs_inferred_target`, `query_param_names`,
//! `command_route_names`, and `command_input_names` shape the body
//! of a command or query block — surfacing what input / route slot /
//! params it declares.

use crate::commands::inspect::expand::{leading_spaces, parse_ident_list};
use crate::commands::inspect::{InspectBinding, InspectDependency};

use super::accessors::{
    emits_derived_effect, parse_event_list, qualify_event_ref, trailing_scalar_value_after,
    typed_declaration,
};
use super::predicates::is_transition_line;

pub(in crate::commands::inspect) fn inspect_binding(
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

pub(in crate::commands::inspect) fn inspect_dependency(
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

pub(in crate::commands::inspect) fn emits_dependencies(
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
        } else if is_transition_line(trimmed)
            && let Some(event) = trailing_scalar_value_after(trimmed, "emits")
        {
            dependencies.push(inspect_dependency(
                "emits_event",
                subject,
                qualify_event_ref(feature, event),
                "transition.emits",
            ));
        }
    }

    dependencies
}

pub(in crate::commands::inspect) fn query_reference_dependencies(
    subject: &str,
    lines: &[String],
) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        for prefix in ["target ", "source "] {
            if let Some(value) = trimmed.strip_prefix(prefix)
                && let Some(query) = value
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

    dependencies
}

pub(in crate::commands::inspect) fn command_needs_inferred_target(lines: &[String]) -> bool {
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

pub(in crate::commands::inspect) fn query_param_names(lines: &[String]) -> Vec<String> {
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

    if params.is_empty()
        && let Some(key) = lines
            .first()
            .and_then(|line| line.trim_start().split(" by ").nth(1))
            .and_then(|rest| rest.split_once(':').map(|(name, _)| name.trim()))
            .filter(|name| !name.is_empty())
    {
        params.push(key.to_owned());
    }

    params
}

pub(in crate::commands::inspect) fn command_route_names(lines: &[String]) -> Vec<String> {
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

pub(in crate::commands::inspect) fn command_input_names(lines: &[String]) -> Vec<String> {
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
