//! Command-trigger diagnostics for resource lifecycle transitions.
//!
//! `Command.triggers` is being added by CTB-IR-1. Until that lands in this
//! worktree, the public IR projection reads the field through serde JSON.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{
    Command, CommandEffect, Feature, LifecycleTransition, QualifiedName, Resource, SpanRef,
};

/// Severity bucket for a lifecycle-transition [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Hard rejection — build fails.
    Error,
    /// Visible in diagnostics-only mode.
    Warning,
}

/// One lifecycle-transition finding (overlap, unknown trigger, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Severity bucket — drives whether the build blocks.
    pub severity: Severity,
    /// Human-readable message — already formatted, no interpolation.
    pub message: String,
    /// Source span for IDE underlining; may be `None`.
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CheckInput {
    features: Vec<FeatureFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct FeatureFacts {
    name: String,
    uses: Vec<String>,
    resources: Vec<ResourceFacts>,
    commands: Vec<CommandFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceFacts {
    name: String,
    lifecycle: Option<LifecycleFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleFacts {
    discriminator_field: String,
    transitions: Vec<TransitionFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionFacts {
    name: String,
    from: Vec<String>,
    to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandFacts {
    feature: String,
    name: String,
    triggers: Vec<String>,
    effect: CommandEffectFacts,
    span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandEffectFacts {
    Updates {
        resource: QualifiedName,
        assignments: Vec<String>,
    },
    Other,
}

/// Run the lifecycle-transition pass over a feature module.
///
/// Validates that every `Command.triggers` clause names a transition
/// declared on the command's target resource and that lifecycle
/// transitions don't claim overlapping `from`/`to` pairs.
///
/// ## Examples
///
/// ```
/// use lazuli_analyzer::checks::lifecycle_transition;
///
/// let diags = lifecycle_transition::check(&[]);
/// assert!(diags.is_empty());
/// ```
pub fn check(features: &[Feature]) -> Vec<Diagnostic> {
    check_input(&input_from_features(features))
}

fn check_input(input: &CheckInput) -> Vec<Diagnostic> {
    let index = Index::new(input);
    let mut out = Vec::new();

    check_overlaps(input, &mut out);

    for feature in &input.features {
        for command in &feature.commands {
            check_command(command, &index, &mut out);
        }
    }

    out
}

struct Index<'a> {
    features: BTreeMap<&'a str, &'a FeatureFacts>,
    resources: BTreeMap<(&'a str, &'a str), &'a ResourceFacts>,
}

impl<'a> Index<'a> {
    fn new(input: &'a CheckInput) -> Self {
        let mut features = BTreeMap::new();
        let mut resources = BTreeMap::new();
        for feature in &input.features {
            features.insert(feature.name.as_str(), feature);
            for resource in &feature.resources {
                resources.insert((feature.name.as_str(), resource.name.as_str()), resource);
            }
        }
        Self {
            features,
            resources,
        }
    }

    fn resource(&self, feature: &str, resource: &QualifiedName) -> Option<&'a ResourceFacts> {
        if let Some(owner) = resource.feature.as_deref() {
            return self
                .resources
                .get(&(owner, resource.name.as_str()))
                .copied();
        }
        for reachable in self.reachable(feature) {
            if let Some(resource) = self
                .resources
                .get(&(reachable.name.as_str(), resource.name.as_str()))
                .copied()
            {
                return Some(resource);
            }
        }
        None
    }

    fn reachable(&self, feature: &str) -> Vec<&'a FeatureFacts> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![feature.to_owned()];
        while let Some(name) = stack.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(feature) = self.features.get(name.as_str()).copied() else {
                continue;
            };
            for used in feature.uses.iter().rev() {
                stack.push(used.clone());
            }
            out.push(feature);
        }
        out
    }
}

fn check_overlaps(input: &CheckInput, out: &mut Vec<Diagnostic>) {
    for feature in &input.features {
        for (left_index, left) in feature.commands.iter().enumerate() {
            if left.triggers.is_empty() {
                continue;
            }
            let left_triggers: BTreeSet<&str> = left.triggers.iter().map(String::as_str).collect();
            for right in feature.commands.iter().skip(left_index + 1) {
                if right.triggers.is_empty() {
                    continue;
                }
                let shared: Vec<_> = right
                    .triggers
                    .iter()
                    .map(String::as_str)
                    .filter(|trigger| left_triggers.contains(trigger))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if shared.is_empty() {
                    continue;
                }
                out.push(diag(
                    "LIFECYCLE-TRANSITION-003",
                    Severity::Warning,
                    right.span.or(left.span),
                    format!(
                        "commands `{}` and `{}` in feature `{}` both trigger lifecycle transition(s): {}. Only one command should own each lifecycle transition.",
                        left.name,
                        right.name,
                        feature.name,
                        shared.join(", ")
                    ),
                ));
            }
        }
    }
}

fn check_command(command: &CommandFacts, index: &Index<'_>, out: &mut Vec<Diagnostic>) {
    if command.triggers.is_empty() {
        return;
    }

    let CommandEffectFacts::Updates {
        resource,
        assignments,
    } = &command.effect
    else {
        out.push(diag(
            "LIFECYCLE-TRANSITION-004",
            Severity::Error,
            command.span,
            format!(
                "command `{}` declares `triggers` but has no `updates` block to advance.",
                command.name
            ),
        ));
        return;
    };

    let resource_facts = index.resource(&command.feature, resource);
    let lifecycle = resource_facts.and_then(|resource| resource.lifecycle.as_ref());

    if assigns_lifecycle_field(assignments, lifecycle) {
        out.push(diag(
            "LIFECYCLE-TRANSITION-005",
            Severity::Error,
            command.span,
            format!(
                "command `{}` both declares `triggers` and assigns the lifecycle discriminator directly; runtime owns lifecycle advancement.",
                command.name
            ),
        ));
    }

    let Some(resource_facts) = resource_facts else {
        return;
    };
    let Some(lifecycle) = lifecycle else {
        out.push(diag(
            "LIFECYCLE-TRANSITION-002",
            Severity::Error,
            command.span,
            format!(
                "command `{}` declares `triggers`, but updates resource `{}` has no lifecycle block.",
                command.name, resource_facts.name
            ),
        ));
        return;
    };

    let transitions: BTreeMap<&str, &TransitionFacts> = lifecycle
        .transitions
        .iter()
        .map(|transition| (transition.name.as_str(), transition))
        .collect();

    let mut missing = BTreeSet::new();
    for trigger in &command.triggers {
        if !transitions.contains_key(trigger.as_str()) && missing.insert(trigger.as_str()) {
            out.push(diag(
                "LIFECYCLE-TRANSITION-001",
                Severity::Error,
                command.span,
                format!(
                    "command `{}` triggers `{}`, but resource `{}` declares no lifecycle transition with that name.",
                    command.name, trigger, resource_facts.name
                ),
            ));
        }
    }

    for pair in command.triggers.windows(2) {
        let Some(left) = transitions.get(pair[0].as_str()).copied() else {
            continue;
        };
        let Some(right) = transitions.get(pair[1].as_str()).copied() else {
            continue;
        };
        if right.from.iter().any(|state| state == &left.to) {
            continue;
        }
        out.push(diag(
            "LIFECYCLE-TRANSITION-006",
            Severity::Error,
            command.span,
            format!(
                "command `{}` triggers a non-contiguous lifecycle chain: `{}` ends at `{}` but `{}` starts from {}.",
                command.name,
                left.name,
                left.to,
                right.name,
                list_or_none(&right.from)
            ),
        ));
    }
}

fn assigns_lifecycle_field(assignments: &[String], lifecycle: Option<&LifecycleFacts>) -> bool {
    assignments.iter().any(|field| {
        field == "lifecycle_state"
            || lifecycle
                .map(|lifecycle| field == &lifecycle.discriminator_field)
                .unwrap_or(false)
    })
}

fn input_from_features(features: &[Feature]) -> CheckInput {
    CheckInput {
        features: features.iter().map(feature_facts).collect(),
    }
}

fn feature_facts(feature: &Feature) -> FeatureFacts {
    FeatureFacts {
        name: feature.name.clone(),
        uses: feature.uses.clone(),
        resources: feature.resources.iter().map(resource_facts).collect(),
        commands: feature
            .commands
            .iter()
            .map(|command| command_facts(&feature.name, command))
            .collect(),
    }
}

fn resource_facts(resource: &Resource) -> ResourceFacts {
    ResourceFacts {
        name: resource.name.clone(),
        lifecycle: resource.lifecycle.as_ref().map(|lifecycle| LifecycleFacts {
            discriminator_field: lifecycle.discriminator_field.clone(),
            transitions: lifecycle.transitions.iter().map(transition_facts).collect(),
        }),
    }
}

fn transition_facts(transition: &LifecycleTransition) -> TransitionFacts {
    TransitionFacts {
        name: transition.name.clone(),
        from: transition.from.clone(),
        to: transition.to.clone(),
    }
}

fn command_facts(feature: &str, command: &Command) -> CommandFacts {
    CommandFacts {
        feature: feature.to_owned(),
        name: command.name.clone(),
        triggers: command_triggers(command),
        effect: match &command.effect {
            CommandEffect::Updates(update) => CommandEffectFacts::Updates {
                resource: update.resource.clone(),
                assignments: update
                    .assignments
                    .iter()
                    .map(|assignment| assignment.field.clone())
                    .collect(),
            },
            _ => CommandEffectFacts::Other,
        },
        span: command.span_ref,
    }
}

fn command_triggers(command: &Command) -> Vec<String> {
    // FIXME: drop after CTB-IR-1 merge and read `command.triggers` directly.
    serde_json::to_value(command)
        .ok()
        .and_then(|value| {
            value
                .get("triggers")
                .and_then(|triggers| triggers.as_array())
                .map(|triggers| {
                    triggers
                        .iter()
                        .filter_map(|trigger| trigger.as_str().map(str::to_owned))
                        .collect()
                })
        })
        .unwrap_or_default()
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".to_owned()
    } else {
        values.join(", ")
    }
}

fn diag(
    code: &'static str,
    severity: Severity,
    span: Option<SpanRef>,
    message: String,
) -> Diagnostic {
    Diagnostic {
        code,
        severity,
        message,
        span,
    }
}

#[cfg(test)]
mod tests;
