//! Cell E3 — lifecycle machine emission for `Command` artifacts.
//!
//! Extracted from `command/mod.rs` as part of the rails-style split.
//! This module owns everything that maps `Command` → `lifecycle.Machine`
//! / `lifecycle.TransitionAdvance` Go code:
//!
//! - `LifecycleCommand` / `TransitionAdvanceLiteral` — IR-borrowed views
//!   that feed the `Creates`/`Updates` emitters.
//! - `lifecycle_transition_for` resolves a command-named transition on
//!   the target resource when the effect is `Updates`.
//! - `transition_advances_for_triggers` resolves declared
//!   `Command.triggers` into runtime advance literals.
//! - `emit_lifecycle_machines` emits the per-feature
//!   `var <resource>Lifecycle = lifecycle.New[...]` block at file
//!   scope, sorted by resource name for byte-stable output.
//!
//! Proposal references:
//! - §3.2 (`lazuli.Command[I,O]` value) — lifecycle transitions
//!   land on the `Command.Transitions` slot in the runtime envelope.
//! - LAZ-LIFECYCLE — `lifecycle.New` / `lifecycle.Transition` shape.

use lazuli_ir::{
    Command, CommandEffect, Feature, Lifecycle, LifecycleStateKind, LifecycleTransition, Resource,
};

use super::super::printer::GoPrinter;
use super::{escape_string, lower_camel, pascal_case};

pub(super) struct LifecycleCommand<'a> {
    pub(super) resource: &'a Resource,
    pub(super) lifecycle: &'a Lifecycle,
    pub(super) transition: &'a LifecycleTransition,
}

pub(super) struct TransitionAdvanceLiteral<'a> {
    pub(super) from: &'a str,
    pub(super) to: &'a str,
}

pub(super) fn command_trigger_names(command: &Command) -> &[String] {
    &command.triggers
}

pub(super) fn lifecycle_transition_for<'a>(
    feature: &'a Feature,
    command: &Command,
) -> Option<LifecycleCommand<'a>> {
    let CommandEffect::Updates(update) = &command.effect else {
        return None;
    };
    feature
        .resources
        .iter()
        .filter(|resource| resource.name == update.resource.name)
        .find_map(|resource| {
            let lifecycle = resource.lifecycle.as_ref()?;
            let transition = lifecycle
                .transitions
                .iter()
                .find(|transition| transition.name == command.name)?;
            Some(LifecycleCommand {
                resource,
                lifecycle,
                transition,
            })
        })
}

pub(super) fn transition_advances_for_triggers<'a>(
    feature: &'a Feature,
    effect: &CommandEffect,
    triggers: &'a [String],
) -> Vec<TransitionAdvanceLiteral<'a>> {
    if triggers.is_empty() {
        return Vec::new();
    }

    let resource_name = match effect {
        CommandEffect::Updates(update) => update.resource.name.as_str(),
        _ => return Vec::new(),
    };
    let Some(lifecycle) = feature
        .resources
        .iter()
        .find(|resource| resource.name == resource_name)
        .and_then(|resource| resource.lifecycle.as_ref())
    else {
        return Vec::new();
    };

    triggers
        .iter()
        .filter_map(|trigger| {
            let transition = lifecycle
                .transitions
                .iter()
                .find(|transition| transition.name == *trigger)?;
            Some(TransitionAdvanceLiteral {
                from: transition.from.first().map(String::as_str).unwrap_or(""),
                to: transition.to.as_str(),
            })
        })
        .collect()
}

pub(super) fn emit_transition_advances(
    p: &mut GoPrinter,
    transitions: &[TransitionAdvanceLiteral<'_>],
) {
    if transitions.is_empty() {
        return;
    }

    p.line("Transitions: []lazuli.TransitionAdvance{");
    p.indent();
    for transition in transitions {
        p.line(&format!(
            "{{From: \"{}\", To: \"{}\"}},",
            escape_string(transition.from),
            escape_string(transition.to)
        ));
    }
    p.dedent();
    p.line("},");
}

pub(super) fn emit_lifecycle_machines(p: &mut GoPrinter, feature: &Feature) -> bool {
    let mut lifecycles: Vec<&Resource> = feature
        .resources
        .iter()
        .filter(|resource| resource.lifecycle.is_some())
        .collect();
    lifecycles.sort_by(|a, b| a.name.cmp(&b.name));
    if lifecycles.is_empty() {
        return false;
    }

    for (idx, resource) in lifecycles.iter().enumerate() {
        if idx > 0 {
            p.blank();
        }
        let lifecycle = resource.lifecycle.as_ref().expect("filtered above");
        let enum_name = pascal_case(&lifecycle.generated_enum);
        let initial = initial_lifecycle_state(lifecycle)
            .map(|state| enum_variant_name(&enum_name, state))
            .unwrap_or_else(|| format!("{enum_name}(\"\")"));
        p.line(&format!(
            "var {} = lifecycle.New[{enum_name}]({initial}, []lifecycle.Transition[{enum_name}]{{",
            lifecycle_machine_var(resource)
        ));
        p.indent();
        for transition in &lifecycle.transitions {
            let from = transition
                .from
                .iter()
                .map(|state| format!("\"{}\"", escape_string(state)))
                .collect::<Vec<_>>()
                .join(", ");
            p.line(&format!(
                "{{Name: \"{}\", From: []string{{{from}}}, To: {}}},",
                escape_string(&transition.name),
                enum_variant_name(&enum_name, &transition.to)
            ));
        }
        p.dedent();
        p.line("})");
    }

    true
}

pub(super) fn lifecycle_machine_var(resource: &Resource) -> String {
    format!("{}Lifecycle", lower_camel(&resource.name))
}

pub(super) fn initial_lifecycle_state(lifecycle: &Lifecycle) -> Option<&str> {
    lifecycle
        .states
        .iter()
        .find(|state| matches!(state.kind, LifecycleStateKind::Initial))
        .or_else(|| lifecycle.states.first())
        .map(|state| state.name.as_str())
}

pub(super) fn enum_variant_name(enum_name: &str, variant: &str) -> String {
    format!("{}{}", enum_name, pascal_case(variant))
}
