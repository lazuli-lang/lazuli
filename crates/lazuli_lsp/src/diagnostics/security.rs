//! Security & data-handling discipline diagnostics.
//!
//! Three producers in one file because they all walk `feature` /
//! `domain` / `policies` blocks looking for the same vocabulary
//! (`@pii.*`, `@cap.Encrypted` / `Hashed` / `Token`, `retention`,
//! `write_window`, `policies fields <Resource>`):
//!
//! - `field-security-policy` — every `@pii.*` / `@cap.Encrypted` /
//!   `@cap.E2ee` / `@cap.Hashed` / `@cap.Token` field must be paired
//!   with a `policies fields <Resource>` block declaring both `read`
//!   and `write` (the `access:` symmetric shorthand satisfies both, since
//!   it desugars to `read:` + `write:` — see 0005 / `field-policy.md`).
//! - `retention-contract` — resources storing `@pii.*` fields must
//!   declare `retention <duration> then <delete|anonymize|archive>`
//!   (or inherit a feature default retention contract).
//! - `write-window-contract` — `write_window by <date-expr> within
//!   <window-ref>` shape validation.
//!
//! The producer functions are accompanied by fact-collector helpers
//! (`collect_sensitive_fields`, `collect_pii_resource_facts`,
//! `collect_retention_facts`, `collect_field_policy_facts`) that the
//! producers compose. Each helper preserves a stable line index so
//! diagnostics anchor at the offending field/resource line.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    feature_name, is_retention_duration_literal, leading_spaces, simple_canonical_diagnostic,
    typed_param,
};

#[derive(Debug)]
pub(crate) struct SensitiveFieldFacts {
    feature: String,
    resource: String,
    field: String,
    line_index: usize,
    line: String,
}

#[derive(Debug, Default)]
pub(crate) struct FieldPolicyFacts {
    read: bool,
    write: bool,
}

#[derive(Debug)]
pub(crate) struct PiiResourceFacts {
    feature: String,
    resource: String,
    line_index: usize,
    line: String,
}

#[derive(Debug, Default)]
pub(crate) struct RetentionFacts {
    feature_defaults: HashSet<String>,
    resources: HashSet<(String, String)>,
}

pub(crate) fn field_security_policy_diagnostics(source: &str) -> Vec<Diagnostic> {
    let sensitive_fields = collect_sensitive_fields(source);
    let field_policies = collect_field_policy_facts(source);
    let mut diagnostics = Vec::new();

    for field in sensitive_fields {
        let has_policy = field_policies
            .get(&(
                field.feature.clone(),
                field.resource.clone(),
                field.field.clone(),
            ))
            .is_some_and(|policy| policy.read && policy.write);

        if !has_policy {
            diagnostics.push(simple_canonical_diagnostic(
                field.line_index,
                &field.line,
                DiagnosticSeverity::WARNING,
                "field-security-policy",
                &format!(
                    "sensitive field `{}.{}` uses `@pii.*` or `@cap.*` and must declare field-level `read` and `write` policies under `policies fields {}`.",
                    field.resource, field.field, field.resource
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_sensitive_fields(source: &str) -> Vec<SensitiveFieldFacts> {
    let mut fields = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_resource = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_resource = None;
            }
            4 if current_top == Some("domain") => {
                current_resource = trimmed
                    .strip_prefix("resource ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
            }
            6 if current_top == Some("domain") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource) = current_resource.as_deref() else {
                    continue;
                };
                let Some((field, _)) = typed_param(trimmed) else {
                    continue;
                };
                if is_sensitive_field_line(line) {
                    fields.push(SensitiveFieldFacts {
                        feature: feature.to_owned(),
                        resource: resource.to_owned(),
                        field: field.to_owned(),
                        line_index,
                        line: line.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    fields
}

pub(crate) fn is_sensitive_field_line(line: &str) -> bool {
    line.contains("@pii.")
        || line.contains("@cap.Encrypted")
        || line.contains("@cap.E2ee")
        || line.contains("@cap.Hashed")
        || line.contains("@cap.Token")
}

pub(crate) fn retention_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let pii_resources = collect_pii_resource_facts(source);
    let retention = collect_retention_facts(source, &mut diagnostics);

    for resource in pii_resources {
        if !retention.feature_defaults.contains(&resource.feature)
            && !retention
                .resources
                .contains(&(resource.feature.clone(), resource.resource.clone()))
        {
            diagnostics.push(simple_canonical_diagnostic(
                resource.line_index,
                &resource.line,
                DiagnosticSeverity::WARNING,
                "retention-contract",
                &format!(
                    "resource `{}` stores `@pii.*` fields and should declare `retention <duration> then delete|anonymize|archive`, or inherit a feature default retention contract.",
                    resource.resource
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_retention_facts(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> RetentionFacts {
    let mut facts = RetentionFacts::default();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_resource = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_resource = None;
            }
            4 if current_top == Some("domain") => {
                current_resource = trimmed
                    .strip_prefix("resource ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
            }
            _ => {}
        }

        if !trimmed.starts_with("retention ") {
            continue;
        }

        if let Some(message) = retention_contract_error(trimmed) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "retention-contract",
                message,
            ));
            continue;
        }

        match (
            leading_spaces(line),
            current_top,
            current_feature.as_deref(),
        ) {
            (4, Some("defaults"), Some(feature)) => {
                facts.feature_defaults.insert(feature.to_owned());
            }
            (6, Some("domain"), Some(feature)) => {
                if let Some(resource) = current_resource.as_deref() {
                    facts
                        .resources
                        .insert((feature.to_owned(), resource.to_owned()));
                }
            }
            _ => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "retention-contract",
                    "`retention` belongs under `defaults` or as a resource child.",
                ));
            }
        }
    }

    facts
}

pub(crate) fn retention_contract_error(trimmed_line: &str) -> Option<&'static str> {
    let parts: Vec<_> = trimmed_line.split_whitespace().collect();
    if parts.len() != 4 || parts[2] != "then" {
        return Some(
            "retention contracts use `retention <duration|forever> then delete|anonymize|archive`.",
        );
    }

    if parts[1] != "forever" && !is_retention_duration_literal(parts[1]) {
        return Some(
            "retention duration should be `forever` or a positive duration such as `30d`, `24mo`, or `7y`.",
        );
    }

    if !matches!(parts[3], "delete" | "anonymize" | "archive") {
        return Some("retention action should be `delete`, `anonymize`, or `archive`.");
    }

    None
}

pub(crate) fn collect_pii_resource_facts(source: &str) -> Vec<PiiResourceFacts> {
    let mut resources = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_resource: Option<(String, usize, String)> = None;
    let mut current_resource_has_pii = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) <= 4 {
            if let Some((resource, resource_line_index, resource_line)) = current_resource.take()
                && current_resource_has_pii
                && let Some(feature) = current_feature.as_deref()
            {
                resources.push(PiiResourceFacts {
                    feature: feature.to_owned(),
                    resource,
                    line_index: resource_line_index,
                    line: resource_line,
                });
            }
            current_resource_has_pii = false;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
            }
            2 => current_top = trimmed.split_whitespace().next(),
            4 if current_top == Some("domain") => {
                current_resource = trimmed
                    .strip_prefix("resource ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(|resource| (resource.to_owned(), line_index, line.to_owned()));
            }
            6 if current_top == Some("domain") && current_resource.is_some() => {
                if line.contains("@pii.") {
                    current_resource_has_pii = true;
                }
            }
            _ => {}
        }
    }

    if let Some((resource, resource_line_index, resource_line)) = current_resource
        && current_resource_has_pii
        && let Some(feature) = current_feature
    {
        resources.push(PiiResourceFacts {
            feature,
            resource,
            line_index: resource_line_index,
            line: resource_line,
        });
    }

    resources
}

pub(crate) fn write_window_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("write_window ") else {
            continue;
        };

        if leading_spaces(line) != 4 {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "write-window-contract",
                "`write_window` belongs as a command child.",
            ));
            continue;
        }

        let parts: Vec<_> = rest.split_whitespace().collect();
        if parts.len() != 4 || parts[0] != "by" || parts[2] != "within" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "write-window-contract",
                "write-window guards use `write_window by <date-expression> within <window-reference>`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_field_policy_facts(
    source: &str,
) -> HashMap<(String, String, String), FieldPolicyFacts> {
    let mut policies = HashMap::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;
    let mut current_policy_resource: Option<String> = None;
    let mut current_policy_field: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            0 if trimmed.starts_with("feature ") => {
                current_feature = Some(feature_name(trimmed));
                current_top = None;
                current_policy_resource = None;
                current_policy_field = None;
            }
            2 => {
                current_top = trimmed.split_whitespace().next();
                current_policy_resource = None;
                current_policy_field = None;
            }
            4 if current_top == Some("policies") => {
                current_policy_resource = trimmed
                    .strip_prefix("fields ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
                current_policy_field = None;
            }
            6 if current_top == Some("policies") && current_policy_resource.is_some() => {
                current_policy_field = Some(trimmed.to_owned());
            }
            8 if current_top == Some("policies") => {
                let Some(feature) = current_feature.as_deref() else {
                    continue;
                };
                let Some(resource) = current_policy_resource.as_deref() else {
                    continue;
                };
                let Some(field) = current_policy_field.as_deref() else {
                    continue;
                };
                let entry = policies
                    .entry((feature.to_owned(), resource.to_owned(), field.to_owned()))
                    .or_insert_with(FieldPolicyFacts::default);
                // 0005 — `access: P` is the symmetric shorthand: it sets
                // BOTH read and write (desugars to `read: P` + `write: P`).
                // The text walker must treat it as satisfying both axes, or
                // a migrated symmetric field would falsely trip the
                // `field-security-policy` contract.
                if trimmed.starts_with("access:") {
                    entry.read = true;
                    entry.write = true;
                } else if trimmed.starts_with("read:") {
                    entry.read = true;
                } else if trimmed.starts_with("write:") {
                    entry.write = true;
                }
            }
            _ => {}
        }
    }

    policies
}

#[cfg(test)]
mod tests {
    include!("security_tests.rs");
}
