//! `--expand=policies` projection.
//!
//! Surfaces the per-subject policy assignment for commands, queries,
//! and workflow transitions. Each row carries the resolved atom list
//! (joined from the per-feature `policies` block), the policy text as
//! authored, the origin (`explicit`, `workflow.policy`), any
//! transition-level `requires` predicates, and the resolved
//! `when_denied` translation key chain.
//!
//! IR Error-Vocab (Cell PARSE-1) — `when_denied` resolution walks
//! the chain documented in `docs/proposals/policy-denial-vocab.md`:
//!
//! 1. Per-command override (caller-supplied on the `command <name>`).
//! 2. Per-policy default (declared inside the `policies` block).
//! 3. None (falls through to runtime envelope).
//!
//! The lookup tables are built once per call from the Tier 3 slice.

use std::collections::BTreeMap;

use super::super::{
    InspectPolicy, InspectPolicyRequirement, Tier3FeatureSlice,
};
use super::super::expand::leading_spaces;
use super::super::text_walkers::{
    command_blocks, command_name, direct_child_value, is_transition_line, query_blocks, query_name,
    resolve_policy_atoms, transition_name, transition_requires,
};

pub(in crate::commands::inspect) fn inspect_policies(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectPolicy> {
    let mut policies = Vec::new();

    // IR Error-Vocab (Cell PARSE-1) — build name -> when_denied
    // lookups so the text walker can attach the per-command override
    // (resolution-chain step 1) and the per-policy default
    // (resolution-chain step 2) onto each InspectPolicy row.
    let command_when_denied: BTreeMap<String, String> = tier3
        .map(|t| {
            t.commands
                .iter()
                .filter_map(|c| {
                    c.policy_when_denied
                        .as_ref()
                        .map(|k| (c.name.clone(), k.key.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    let policy_when_denied: BTreeMap<String, String> = tier3
        .map(|t| {
            t.policies
                .categories
                .iter()
                .filter_map(|cat| {
                    cat.when_denied
                        .as_ref()
                        .map(|k| (cat.name.clone(), k.key.clone()))
                })
                .collect()
        })
        .unwrap_or_default();

    // Helper — resolve the effective `when_denied` for a given policy
    // string. Walks the resolution chain: prefer the per-command
    // override (caller-supplied) over the per-policy category default.
    let resolve_when_denied = |policy_text: &str, override_key: Option<&str>| -> Option<String> {
        if let Some(k) = override_key {
            return Some(k.to_owned());
        }
        // The `policy_text` carries `@policy.<name>` for named
        // categories; strip the prefix to look up the category default.
        if let Some(name) = policy_text
            .trim()
            .strip_prefix("@policy.")
            .map(|s| s.split_whitespace().next().unwrap_or(""))
        {
            return policy_when_denied.get(name).cloned();
        }
        None
    };

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(command, "policy ") {
            let override_key = command_when_denied.get(name).map(String::as_str);
            let when_denied = resolve_when_denied(&policy, override_key);
            policies.push(InspectPolicy {
                subject: format!("command.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
                when_denied,
            });
        }
    }

    for query in query_blocks(lines) {
        let name = query_name(query[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(query, "policy ") {
            let when_denied = resolve_when_denied(&policy, None);
            policies.push(InspectPolicy {
                subject: format!("query.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
                when_denied,
            });
        }
    }

    let mut workflow_name = None;
    let mut workflow_policy = None;

    for line in lines {
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("workflow ") {
            workflow_name = trimmed.split_whitespace().nth(1).map(str::to_owned);
            workflow_policy = None;
        } else if leading_spaces(line) == 4 && workflow_name.is_some() {
            if let Some(policy) = trimmed.strip_prefix("policy ") {
                workflow_policy = Some(policy.to_owned());
            } else if is_transition_line(trimmed) {
                let transition = transition_name(trimmed).unwrap_or("unknown");
                let policy = workflow_policy.clone().unwrap_or_else(|| "none".to_owned());
                let mut requires = Vec::new();

                if let Some(required) = transition_requires(trimmed) {
                    requires.push(InspectPolicyRequirement {
                        atoms: resolve_policy_atoms(&required, policy_atoms),
                        policy: required,
                        origin: "transition.requires".to_owned(),
                    });
                }

                let when_denied = resolve_when_denied(&policy, None);
                policies.push(InspectPolicy {
                    subject: format!(
                        "workflow.{}.{}",
                        workflow_name.as_deref().unwrap_or("unknown"),
                        transition
                    ),
                    atoms: resolve_policy_atoms(&policy, policy_atoms),
                    policy,
                    origin: "workflow.policy".to_owned(),
                    requires,
                    when_denied,
                });
            }
        } else if leading_spaces(line) <= 2 {
            workflow_name = None;
            workflow_policy = None;
        }
    }

    policies
}
