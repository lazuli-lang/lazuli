//! `--expand=dependencies` projection.
//!
//! Surfaces the graph of inter-feature and intra-feature edges declared
//! on a feature:
//!
//! - `uses <other_feature, ...>` — explicit cross-feature consumption
//!   declared at the feature top level.
//! - `extends @anchor.<name>` — anchor inheritance.
//! - `emits_event` — events authored on commands, workflows, jobs,
//!   webhooks (both bare `emits` and the `emits ... from <creates|updates|deletes>`
//!   derived form).
//! - `trigger_event` — events that bind a job to a producer event.
//! - `query_ref` — query references inside `target` / `source` slots.
//!
//! Edges are typed (`kind`) and carry an `origin` so consumers can
//! trace each one back to the authoring construct.

use super::super::InspectDependency;
use super::super::expand::leading_spaces;
use super::super::text_walkers::{
    command_blocks, command_name, direct_child_value, emits_dependencies, inspect_dependency,
    named_top_block_name, qualify_event_ref, query_reference_dependencies, top_level_blocks,
};

pub(in crate::commands::inspect) fn inspect_dependencies(
    lines: &[String],
) -> Vec<InspectDependency> {
    let feature = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown");
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 && trimmed.starts_with("uses ") {
            for target in
                super::super::expand::parse_ident_list(trimmed.trim_start_matches("uses "))
            {
                dependencies.push(inspect_dependency("uses", feature, target, "uses"));
            }
        } else if leading_spaces(line) == 2 && trimmed.starts_with("extends @anchor.") {
            if let Some(anchor) = trimmed.split_whitespace().nth(1) {
                dependencies.push(inspect_dependency(
                    "extends_anchor",
                    feature,
                    anchor,
                    "extends",
                ));
            }
        }
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.command.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
        dependencies.extend(query_reference_dependencies(&subject, block));
    }

    for block in top_level_blocks(lines, "workflow ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.workflow.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.job.{name}");
        if let Some(trigger) = direct_child_value(block, "trigger ") {
            if let Some(event) = trigger.strip_prefix("event ") {
                dependencies.push(inspect_dependency(
                    "trigger_event",
                    subject.clone(),
                    qualify_event_ref(feature, event.trim()),
                    "job.trigger",
                ));
            }
        }
        dependencies.extend(emits_dependencies(feature, &subject, block));
        dependencies.extend(query_reference_dependencies(&subject, block));
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.webhook.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
    }

    dependencies
}
