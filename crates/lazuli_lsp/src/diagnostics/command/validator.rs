//! Validator-binding consumption check for `command` blocks.
//!
//! Flags `let result = @validator.X` bindings that are never consumed
//! by a downstream `validate` or `requires`, so the command can continue
//! silently after a failed validator.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

#[derive(Debug)]
pub(crate) struct CommandValidatorFacts {
    validators: Vec<(String, usize, String)>,
    requirements: HashSet<String>,
    has_blocking_validate: bool,
}

pub(crate) fn command_validator_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_command: Option<CommandValidatorFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_validator_facts_diagnostics(command));
            }
            current_command = Some(CommandValidatorFacts {
                validators: Vec::new(),
                requirements: HashSet::new(),
                has_blocking_validate: false,
            });
            continue;
        }

        if leading_spaces(line) <= 2 && !trimmed.is_empty() {
            if let Some(command) = current_command.take() {
                diagnostics.extend(command_validator_facts_diagnostics(command));
            }
            continue;
        }

        let Some(command) = current_command.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4 {
            if trimmed.starts_with("validate @validator.") {
                command.has_blocking_validate = true;
            } else if let Some((binding, expression)) = trimmed
                .strip_prefix("let ")
                .and_then(|rest| rest.split_once('='))
            {
                if expression.trim().starts_with("@validator.") {
                    command.validators.push((
                        binding.trim().to_owned(),
                        line_index,
                        line.to_owned(),
                    ));
                }
            } else if let Some(requirement) = trimmed.strip_prefix("requires ") {
                command.requirements.insert(requirement.trim().to_owned());
            }
        }
    }

    if let Some(command) = current_command {
        diagnostics.extend(command_validator_facts_diagnostics(command));
    }

    diagnostics
}

pub(crate) fn command_validator_facts_diagnostics(
    command: CommandValidatorFacts,
) -> Vec<Diagnostic> {
    if command.has_blocking_validate {
        return Vec::new();
    }

    command
        .validators
        .into_iter()
        .filter(|(binding, _, _)| !command.requirements.contains(binding))
        .map(|(binding, line_index, line)| {
            simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "command-validator-result",
                &format!(
                    "validator result `{binding}` is computed but not required; use `validate @validator...` or `requires {binding}` so the command cannot continue after validation fails.",
                ),
            )
        })
        .collect()
}
