//! `--expand=tests` projection.
//!
//! Surfaces every `tests` block authored under a feature subject
//! (command, query, workflow transition, view anchor, rule) with the
//! assertions grouped by family (`authz`, `transition`, `predicate`,
//! `anchor`, `other`). Additionally, commands with a `policy` slot
//! gain auto-derived authz assertions (`permits <atoms>`, `forbids
//! actors outside <policy>`) so the tests projection mirrors the
//! closed-world authorization model without re-authoring it.
//!
//! The subject-stack walker tracks the current authoring scope via
//! indent so nested `tests` blocks resolve to the right subject —
//! `command.<name>`, `transition.<name>`, `view.<anchor>`, `rule.<name>`.

use std::collections::BTreeMap;

use super::super::{InspectTestAssertion, InspectTests};
use super::super::expand::leading_spaces;
use super::super::text_walkers::{
    command_blocks, command_name, direct_child_value, inspect_subject, resolve_policy_atoms,
    test_group,
};

pub(in crate::commands::inspect) fn inspect_tests(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
) -> Vec<InspectTests> {
    let mut tests = Vec::new();
    let mut subject_stack: Vec<(usize, String)> = Vec::new();
    let mut index = 0;

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let Some(policy) = direct_child_value(command, "policy ") else {
            continue;
        };
        let atoms = resolve_policy_atoms(&policy, policy_atoms);
        if atoms.is_empty() {
            continue;
        }
        let subject = format!("command.{name}");
        push_inspect_test_assertion(
            &mut tests,
            &subject,
            "authz",
            format!("permits {}", atoms.join(", ")),
            format!("generated from command policy {policy}"),
        );
        push_inspect_test_assertion(
            &mut tests,
            &subject,
            "authz",
            format!("forbids actors outside {policy}"),
            format!("generated from closed-world command policy {policy}"),
        );
    }

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        while subject_stack
            .last()
            .is_some_and(|(indent, _)| *indent >= leading)
        {
            subject_stack.pop();
        }

        if let Some(subject) = inspect_subject(trimmed) {
            subject_stack.push((leading, subject));
        }

        if trimmed == "tests" {
            let subject = subject_stack
                .last()
                .map(|(_, subject)| subject.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            let mut groups: BTreeMap<String, Vec<InspectTestAssertion>> = BTreeMap::new();
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > leading {
                let assertion = lines[child_index].trim_start();
                if !assertion.is_empty() {
                    groups
                        .entry(test_group(assertion).to_owned())
                        .or_default()
                        .push(InspectTestAssertion {
                            assertion: assertion.to_owned(),
                            origin: "authored".to_owned(),
                        });
                }
                child_index += 1;
            }

            merge_inspect_tests(&mut tests, InspectTests { subject, groups });
            index = child_index;
            continue;
        }

        index += 1;
    }

    tests
}

fn push_inspect_test_assertion(
    tests: &mut Vec<InspectTests>,
    subject: &str,
    group: &str,
    assertion: String,
    origin: String,
) {
    let Some(existing) = tests.iter_mut().find(|entry| entry.subject == subject) else {
        tests.push(InspectTests {
            subject: subject.to_owned(),
            groups: BTreeMap::from([(
                group.to_owned(),
                vec![InspectTestAssertion { assertion, origin }],
            )]),
        });
        return;
    };

    existing
        .groups
        .entry(group.to_owned())
        .or_default()
        .push(InspectTestAssertion { assertion, origin });
}

fn merge_inspect_tests(tests: &mut Vec<InspectTests>, incoming: InspectTests) {
    let Some(existing) = tests
        .iter_mut()
        .find(|entry| entry.subject == incoming.subject)
    else {
        tests.push(incoming);
        return;
    };

    for (group, assertions) in incoming.groups {
        existing.groups.entry(group).or_default().extend(assertions);
    }
}
