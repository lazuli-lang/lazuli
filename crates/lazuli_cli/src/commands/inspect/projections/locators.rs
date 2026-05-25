//! `--expand=locators` projection.
//!
//! Surfaces the execution-context bindings available inside each
//! locatable construct on a feature:
//!
//! - `command.<name>` — `ctx.*`, `route.<slot>` for each `route <slot>:
//!   <type>`, `input.<field>` for each declared input field, and the
//!   `target` binding (either explicit via `target ...` or inferred
//!   when the feature has a `query.lookup by_id` and the command
//!   mutates an existing entity).
//! - `query.<name>` — `ctx.*` plus `params.<key>` for every declared
//!   query parameter. The locator `kind` carries the resolved
//!   `query.<list|lookup|sql|view>` shape from `query_kind`.
//! - `job.<name>` — `ctx.*` plus `envelope.*` / `payload.*` (event
//!   trigger) or `schedule.*` (schedule trigger), with the locator
//!   `kind` set to `event_job` / `schedule_job` / `job` accordingly.
//! - `webhook.<name>` — `ctx.*` plus `payload.*` for inbound bodies.
//! - `rule.<name>` — `ctx.*` plus `self` (the rule's target snapshot).
//!
//! Each binding records its origin and meaning so an LLM reading the
//! projection cold can author downstream logic against the right
//! identifiers.

use super::super::InspectLocators;
use super::super::expand::feature_has_id_lookup;
use super::super::text_walkers::{
    command_blocks, command_input_names, command_name, command_needs_inferred_target,
    command_route_names, direct_child_value, inspect_binding, named_top_block_name, query_blocks,
    query_kind, query_name, query_param_names, top_level_blocks,
};

pub(in crate::commands::inspect) fn inspect_locators(lines: &[String]) -> Vec<InspectLocators> {
    let mut locators = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for block in query_blocks(lines) {
        let name = query_name(block[0].trim_start()).unwrap_or("unknown");
        let inferred = query_kind(block);
        let mut bindings = vec![inspect_binding(
            "ctx.*",
            "runtime",
            "request and tenant execution context",
        )];

        for param in query_param_names(block) {
            bindings.push(inspect_binding(
                format!("params.{param}"),
                "query.params",
                "read argument declared by this query",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("query.{name}"),
            kind: format!("query.{inferred}"),
            bindings,
        });
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let mut bindings = vec![inspect_binding(
            "ctx.*",
            "runtime",
            "request and tenant execution context",
        )];

        for route in command_route_names(block) {
            bindings.push(inspect_binding(
                format!("route.{route}"),
                "command.route",
                "path or caller-context locator declared by this command",
            ));
        }

        for input in command_input_names(block) {
            bindings.push(inspect_binding(
                format!("input.{input}"),
                "command.input",
                "submitted command body field",
            ));
        }

        if let Some(target) = direct_child_value(block, "target ") {
            bindings.push(inspect_binding(
                "target",
                format!("explicit target {target}"),
                "entity loaded before declarative command effects",
            ));
        } else if has_id_lookup && command_needs_inferred_target(block) {
            bindings.push(inspect_binding(
                "target",
                "inferred local query.by_id(id: route.id)",
                "entity loaded before declarative command effects",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("command.{name}"),
            kind: "command".to_owned(),
            bindings,
        });
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let trigger = direct_child_value(block, "trigger ");
        let mut bindings = vec![inspect_binding("ctx.*", "runtime", "job execution context")];
        let kind = if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("event "))
        {
            bindings.push(inspect_binding(
                "envelope.*",
                "event trigger",
                "event-bus metadata such as envelope.id",
            ));
            bindings.push(inspect_binding(
                "payload.*",
                "event trigger",
                "producer event payload fields",
            ));
            "event_job"
        } else if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("schedule "))
        {
            bindings.push(inspect_binding(
                "schedule.*",
                "schedule trigger",
                "scheduler metadata such as run time",
            ));
            "schedule_job"
        } else {
            "job"
        };

        if let Some(target) = direct_child_value(block, "target ") {
            bindings.push(inspect_binding(
                "target",
                format!("explicit target {target}"),
                "entity loaded before declarative job effects",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("job.{name}"),
            kind: kind.to_owned(),
            bindings,
        });
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        locators.push(InspectLocators {
            subject: format!("webhook.{name}"),
            kind: "webhook".to_owned(),
            bindings: vec![
                inspect_binding(
                    "payload.*",
                    "webhook payload",
                    "verified inbound request body fields",
                ),
                inspect_binding("ctx.*", "runtime", "webhook execution context"),
            ],
        });
    }

    for block in top_level_blocks(lines, "rule ") {
        let name = block[0]
            .trim_start()
            .trim_start_matches("rule ")
            .trim_matches('"');
        locators.push(InspectLocators {
            subject: format!("rule.{name}"),
            kind: "rule".to_owned(),
            bindings: vec![
                inspect_binding(
                    "self",
                    "rule target snapshot",
                    "resource snapshot evaluated by the rule predicate",
                ),
                inspect_binding("ctx.*", "runtime", "request and tenant execution context"),
            ],
        });
    }

    locators
}
