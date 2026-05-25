//! `--expand=targets` projection.
//!
//! Surfaces the `target` slot per command: either explicit (the
//! command authored `target <expression>` directly) or inferred (the
//! feature has a `query.lookup by_id` and the command mutates an
//! existing entity via `updates`/`deletes` with `route id: ID`).
//! Doctor cross-checks targets against the resolved query; this
//! projection is the inspect-side observable that records the source
//! authoring.

use super::super::InspectTarget;
use super::super::expand::{feature_has_id_lookup, leading_spaces};
use super::super::text_walkers::{command_blocks, command_name, command_needs_inferred_target};

pub(in crate::commands::inspect) fn inspect_targets(lines: &[String]) -> Vec<InspectTarget> {
    let mut targets = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let explicit = command.iter().find_map(|line| {
            if leading_spaces(line) == 4 {
                line.trim_start().strip_prefix("target ").map(str::to_owned)
            } else {
                None
            }
        });

        if let Some(target) = explicit {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target,
                origin: "explicit".to_owned(),
            });
        } else if has_id_lookup && command_needs_inferred_target(command) {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target: "query.by_id(id: route.id)".to_owned(),
                origin: "inferred from local route id and query.lookup by_id".to_owned(),
            });
        }
    }

    targets
}
