//! `validates @validator.<name>` call-site syntax.
//!
//! Canonical form is `validates @validator.<name>`; legacy
//! `validate ...` and scoped `validates field ... @validator.<name>` /
//! `validates resource @validator.<name>` forms warn because the
//! validator's `Validator[<scope>]` type already names the scope.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::simple_canonical_diagnostic;

pub(crate) fn validation_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("validate ") && !trimmed.starts_with("validate @validator.") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "validators are referenced through `validates @validator.<name>`; the scope (field or resource) is declared by the validator's `Validator[<scope>]` type in `extensions`.",
            ));
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("validates ") else {
            continue;
        };

        let argument = rest.trim();

        // Canonical: `validates @validator.<name>`
        if argument.starts_with("@validator.") {
            continue;
        }

        // Legacy with explicit scope: `validates field <name> @validator.<name>`
        // or `validates resource @validator.<name>`. Both forms still parse but
        // warn — the validator's `Validator[<scope>]` type already carries the
        // scope, so repeating it at the call site is redundant.
        let (legacy_form, target) = if let Some(field_rest) = argument.strip_prefix("field ") {
            let target = field_rest.split_whitespace().nth(1).unwrap_or("");
            ("legacy-scoped-field", target)
        } else if let Some(resource_rest) = argument.strip_prefix("resource") {
            ("legacy-scoped-resource", resource_rest.trim())
        } else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "validators are referenced through `validates @validator.<name>`; the scope (field or resource) is declared by the validator's `Validator[<scope>]` type in `extensions`.",
            ));
            continue;
        };

        if target.starts_with('"') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "inline `\"./path.go\"` validator references are legacy. Declare the validator under `extensions.validator <name>: Validator[<scope>] at \"./path.go\"` and reference it as `validates @validator.<name>`.",
            ));
        } else if target.starts_with("@validator.") {
            // Legacy scope keyword present but otherwise canonical — warn that
            // the scope is redundant.
            let hint = match legacy_form {
                "legacy-scoped-field" => {
                    "drop the `field <name>` prefix; the validator's `Validator[<scope>]` type already names the field."
                }
                _ => {
                    "drop the `resource` prefix; the validator's `Validator[<scope>]` type already targets the resource."
                }
            };
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                &format!("`validates @validator.<name>` is the canonical form: {hint}"),
            ));
        } else if !target.is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "validation-syntax",
                "validator references should use the `@validator.<name>` namespace. Declare the validator under `extensions.validator <name>` first.",
            ));
        }
    }

    diagnostics
}
