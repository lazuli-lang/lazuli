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
    /// All declared `from` states for this transition (fan-in / MULTI-SOURCE).
    /// `from[0]` is the primary state retained on the runtime `From` field for
    /// back-compat; the full set is emitted on `FromAny` for the membership
    /// guard. Always non-empty for a well-formed transition.
    pub(super) from: Vec<&'a str>,
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
            let from: Vec<&str> = transition.from.iter().map(String::as_str).collect();
            Some(TransitionAdvanceLiteral {
                from: if from.is_empty() { vec![""] } else { from },
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
        // `From` keeps the primary state for back-compat; `FromAny` carries
        // the full fan-in set the runtime membership guard checks against.
        let primary = transition.from.first().copied().unwrap_or("");
        let from_any = transition
            .from
            .iter()
            .map(|state| format!("\"{}\"", escape_string(state)))
            .collect::<Vec<_>>()
            .join(", ");
        p.line(&format!(
            "{{From: \"{}\", FromAny: []string{{{from_any}}}, To: \"{}\"}},",
            escape_string(primary),
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
        // Filter at line 131 guarantees lifecycle is Some.
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    //! Lifecycle tests — exercise the `lifecycle.New[<state>]` /
    //! `lifecycle.Apply(...)` projection done by `emit_lifecycle_machines`
    //! and `emit_transition_advances`. Lifted out of `file_emit.rs`
    //! (wave R8-2b) so the lifecycle concern owns its own behavioural
    //! tests.
    use super::super::super::printer::GoPrinter;
    use super::super::test_support::{
        base_command, base_feature, emit_with_customer_fallback as emit, local_qname,
        simple_resource,
    };
    use super::*;
    use lazuli_ir::{
        CommandEffect, CommandKind, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Resource, UpdateEffect,
    };

    fn lifecycle_resource(name: &str, field: &str, enum_name: &str) -> Resource {
        let mut resource = simple_resource(name);
        resource.lifecycle = Some(Lifecycle {
            discriminator_field: field.to_owned(),
            generated_enum: enum_name.to_owned(),
            states: vec![
                LifecycleState {
                    name: "scheduled".to_owned(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "publishing".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
                LifecycleState {
                    name: "published".to_owned(),
                    kind: LifecycleStateKind::Terminal,
                    span_ref: None,
                },
            ],
            transitions: vec![LifecycleTransition {
                name: "begin_publishing".to_owned(),
                from: vec!["scheduled".to_owned()],
                to: "publishing".to_owned(),
                policy: None,
                audit: None,
                timestamps: Some("publishing_at".to_owned()),
                emits: Vec::new(),
                requires: None,
                tests: None,
                previous_names: Vec::new(),
                span_ref: None,
            }],
            invariants: Vec::new(),
            invariant_handlers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        });
        resource
    }

    #[test]
    fn lifecycle_command_emits_apply_call() {
        let mut feature = base_feature("publication");
        feature.resources.push(lifecycle_resource(
            "Publication",
            "status",
            "PublicationStatus",
        ));
        let mut cmd = base_command("begin_publishing");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/lifecycle\""));
        assert!(out.contains("var publicationLifecycle = lifecycle.New[PublicationStatus]("));
        assert!(out.contains(
            "// lifecycle: newState, err := publicationLifecycle.Apply(ctx, current.Status, \"begin_publishing\")"
        ));
    }

    #[test]
    fn command_triggers_emit_transition_advances_in_order() {
        let mut feature = base_feature("publication");
        let mut resource = simple_resource("Publication");
        resource.lifecycle = Some(Lifecycle {
            discriminator_field: "lifecycle_state".to_owned(),
            generated_enum: "PublicationState".to_owned(),
            states: vec![
                LifecycleState {
                    name: "basic_details_pending".to_owned(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "address_pending".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
                LifecycleState {
                    name: "review_pending".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
            ],
            transitions: vec![
                LifecycleTransition {
                    name: "T1".to_owned(),
                    from: vec!["basic_details_pending".to_owned()],
                    to: "address_pending".to_owned(),
                    policy: None,
                    audit: None,
                    timestamps: None,
                    emits: Vec::new(),
                    requires: None,
                    tests: None,
                    previous_names: Vec::new(),
                    span_ref: None,
                },
                LifecycleTransition {
                    name: "T2".to_owned(),
                    from: vec!["address_pending".to_owned()],
                    to: "review_pending".to_owned(),
                    policy: None,
                    audit: None,
                    timestamps: None,
                    emits: Vec::new(),
                    requires: None,
                    tests: None,
                    previous_names: Vec::new(),
                    span_ref: None,
                },
            ],
            invariants: Vec::new(),
            invariant_handlers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        });
        feature.resources.push(resource);

        let mut cmd = base_command("advance_publication");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        let triggers = vec!["T1".to_owned(), "T2".to_owned()];

        let transitions = transition_advances_for_triggers(&feature, &cmd.effect, &triggers);
        let mut p = GoPrinter::new();
        emit_transition_advances(&mut p, &transitions);
        let out = p.finish();

        assert!(out.contains("Transitions: []lazuli.TransitionAdvance{"));
        let first = "{From: \"basic_details_pending\", FromAny: []string{\"basic_details_pending\"}, To: \"address_pending\"},";
        let second = "{From: \"address_pending\", FromAny: []string{\"address_pending\"}, To: \"review_pending\"},";
        assert!(out.contains(first), "first trigger pair missing:\n{out}");
        assert!(out.contains(second), "second trigger pair missing:\n{out}");
        assert!(
            out.find(first) < out.find(second),
            "trigger order should be preserved:\n{out}"
        );
    }

    #[test]
    fn multi_source_trigger_emits_all_from_states() {
        // GAP-R1: a transition with `from A, B` (fan-in) must emit BOTH
        // states on `FromAny`, with the primary on `From` for back-compat.
        let mut feature = base_feature("publication");
        let mut resource = simple_resource("Publication");
        resource.lifecycle = Some(Lifecycle {
            discriminator_field: "lifecycle_state".to_owned(),
            generated_enum: "PublicationState".to_owned(),
            states: vec![
                LifecycleState {
                    name: "draft".to_owned(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "scheduled".to_owned(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                },
                LifecycleState {
                    name: "published".to_owned(),
                    kind: LifecycleStateKind::Terminal,
                    span_ref: None,
                },
            ],
            transitions: vec![LifecycleTransition {
                name: "publish".to_owned(),
                from: vec!["draft".to_owned(), "scheduled".to_owned()],
                to: "published".to_owned(),
                policy: None,
                audit: None,
                timestamps: None,
                emits: Vec::new(),
                requires: None,
                tests: None,
                previous_names: Vec::new(),
                span_ref: None,
            }],
            invariants: Vec::new(),
            invariant_handlers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        });
        feature.resources.push(resource);

        let mut cmd = base_command("publish");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        let triggers = vec!["publish".to_owned()];

        let transitions = transition_advances_for_triggers(&feature, &cmd.effect, &triggers);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].from, vec!["draft", "scheduled"]);

        let mut p = GoPrinter::new();
        emit_transition_advances(&mut p, &transitions);
        let out = p.finish();

        let expected =
            "{From: \"draft\", FromAny: []string{\"draft\", \"scheduled\"}, To: \"published\"},";
        assert!(out.contains(expected), "fan-in from-set missing:\n{out}");
    }

    #[test]
    fn non_lifecycle_command_does_not_emit_apply_call() {
        let mut feature = base_feature("publication");
        feature.resources.push(lifecycle_resource(
            "Publication",
            "status",
            "PublicationStatus",
        ));
        let mut cmd = base_command("rename");
        cmd.kind = CommandKind::Update;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Publication"),
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(!out.contains(".Apply(ctx,"));
    }

    #[test]
    fn multi_lifecycle_resources_emit_per_resource_vars() {
        let mut feature = base_feature("publication");
        feature.resources.push(lifecycle_resource(
            "Publication",
            "status",
            "PublicationStatus",
        ));
        feature
            .resources
            .push(lifecycle_resource("Issue", "state", "IssueState"));
        feature.commands.push(base_command("noop"));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("var issueLifecycle = lifecycle.New[IssueState]("));
        assert!(out.contains("var publicationLifecycle = lifecycle.New[PublicationStatus]("));
        assert_eq!(out.matches("lifecycle.New[").count(), 2);
    }
}
