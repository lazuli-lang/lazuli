//! `command.rate_limit` contract.
//!
//! Public or mutating commands must declare an explicit `rate_limit
//! ...` value or an opted-out `rate_limit none` with a `reason "..."`
//! child. `@scope.public` (direct or transitively through
//! `@policy.<category>`) makes a command public for the purpose of
//! this check.

use std::collections::HashMap;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    command_write_effect, feature_name, leading_spaces, simple_canonical_diagnostic,
};

use super::namespace::policy_statement_ref;

#[derive(Debug)]
pub(crate) struct CommandSecurityFacts {
    feature: String,
    line_index: usize,
    line: String,
    policy: Option<String>,
    has_write_effect: bool,
    has_rate_limit: bool,
    rate_limit_none: Option<(usize, String)>,
    rate_limit_none_has_reason: bool,
}

pub(crate) fn command_rate_limit_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let policies = collect_policy_atom_map(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_command: Option<CommandSecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
            }
            current_feature = Some(feature_name(trimmed));
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
            }
            current_command = current_feature
                .as_ref()
                .map(|feature| CommandSecurityFacts {
                    feature: feature.clone(),
                    line_index,
                    line: line.to_owned(),
                    policy: None,
                    has_write_effect: false,
                    has_rate_limit: false,
                    rate_limit_none: None,
                    rate_limit_none_has_reason: false,
                });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
            }
            continue;
        }

        let Some(command) = current_command.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if let Some(policy) = policy_statement_ref(trimmed) {
                command.policy = Some(policy.to_owned());
            } else if trimmed == "rate_limit none" {
                command.has_rate_limit = true;
                command.rate_limit_none = Some((line_index, line.to_owned()));
            } else if trimmed.starts_with("rate_limit ") {
                command.has_rate_limit = true;
            } else if command_write_effect(trimmed).is_some() {
                command.has_write_effect = true;
            }
        } else if leading_spaces(line) == 6
            && command.rate_limit_none.is_some()
            && trimmed.starts_with("reason ")
        {
            command.rate_limit_none_has_reason = true;
        }
    }

    if let Some(command) = current_command {
        diagnostics.extend(command_rate_limit_diagnostics(command, &policies));
    }

    diagnostics
}

pub(crate) fn command_rate_limit_diagnostics(
    command: CommandSecurityFacts,
    policies: &HashMap<(String, String), Vec<String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let is_public = command
        .policy
        .as_deref()
        .is_some_and(|policy| policy_ref_is_public(&command.feature, policy, policies));

    if (is_public || command.has_write_effect) && !command.has_rate_limit {
        diagnostics.push(simple_canonical_diagnostic(
            command.line_index,
            &command.line,
            DiagnosticSeverity::WARNING,
            "command-rate-limit",
            "commands that are public or mutate state must declare a command-level `rate_limit` or `rate_limit none` with a `reason` child.",
        ));
    }

    if let Some((line_index, line)) = command.rate_limit_none {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "security-opt-out",
            "`rate_limit none` is an explicit security opt-out. Strict profile allows it for reviewed drafts; production profile treats it as a release blocker.",
        ));

        if !command.rate_limit_none_has_reason {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "security-opt-out-reason",
                "`rate_limit none` must include a `reason \"...\"` child.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn policy_ref_is_public(
    feature: &str,
    policy_ref: &str,
    policies: &HashMap<(String, String), Vec<String>>,
) -> bool {
    if policy_ref == "@scope.public" {
        return true;
    }

    let Some(category) = policy_ref.strip_prefix("@policy.") else {
        return false;
    };

    policies
        .get(&(feature.to_owned(), category.to_owned()))
        .is_some_and(|atoms| atoms.iter().any(|atom| atom == "@scope.public"))
}

pub(crate) fn collect_policy_atom_map(source: &str) -> HashMap<(String, String), Vec<String>> {
    let mut policies = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
            }
            2 => current_top = trimmed.split_whitespace().next(),
            4 if current_top == Some("policies") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some((name, atoms)) = trimmed.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty() || name == "fields" || name.contains(' ') {
                    continue;
                }
                policies.insert(
                    (feature.to_owned(), name.to_owned()),
                    atoms
                        .split(',')
                        .map(str::trim)
                        .filter(|atom| !atom.is_empty())
                        .map(str::to_owned)
                        .collect(),
                );
            }
            _ => {}
        }
    }

    policies
}
