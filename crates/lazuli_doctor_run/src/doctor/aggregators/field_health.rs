//! Field & resource health diagnostics.
//!
//! Three Tier 4c lints from the naming-reconciliation proposal
//! (`docs/proposals/naming-reconciliation-2026-05-17.md` §4):
//!
//! - `field_derived_from_unresolved` — `derived from <expr>` references
//!   an identifier that doesn't resolve to a sibling field.
//! - `resource_unique_qualifier_unknown` — `unique <field> per
//!   <qualifier>` names a `<qualifier>` that is not a field on the
//!   same resource (and is not a known tenant axis).
//! - `resource_validates_path_unknown` — `validates field <field>
//!   [@validator.<name>]` either points at an unknown field or at an
//!   unregistered validator extension.
//!
//! All three are Warning severity by design: the runtime degrades
//! silently in each failure case; the Warning surfaces the gap at
//! design time so SQL composes the intended index / the validator
//! resolves through extensions / the typo gets caught before deploy.

use std::collections::BTreeSet;

use crate::doctor::helpers::line_col_for_offset_in_file;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// `field_derived_from_unresolved` — warn when a resource field's
/// `derived from <expr>` references identifiers that don't resolve
/// to siblings on the same resource. Closes the first of three
/// net-new Tier 4c doctor lints catalogued in the
/// naming-reconciliation proposal (`docs/proposals/naming-reconciliation-2026-05-17.md`).
///
/// The lint tokenises the expression text, drops keywords / operators
/// / numeric literals / string literals / dotted-path identifiers
/// (`other.field` — relation traversal is out of scope for v1), and
/// reports any remaining bare identifier that is not a sibling field
/// or a built-in (`ctx`, `now`, `true`, `false`, `nil`). Severity is
/// Warning — the runtime panics on resolution failure, but a Warning
/// at design time surfaces the typo before deploy.
pub(crate) fn field_derived_from_unresolved_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for feature in tier3_facts {
        for resource in &feature.resources {
            let sibling_names: BTreeSet<&str> =
                resource.fields.iter().map(|f| f.name.as_str()).collect();
            for field in &resource.fields {
                let Some(expr) = field.derived_from.as_deref() else {
                    continue;
                };
                let unresolved = collect_unresolved_field_refs(expr, &sibling_names);
                if unresolved.is_empty() {
                    continue;
                }
                let line = field
                    .span_ref
                    .as_ref()
                    .map(|s| line_col_for_offset_in_file(&feature.path, s.start).0)
                    .unwrap_or(feature.feature_line);
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "field_derived_from_unresolved".to_owned(),
                    message: format!(
                        "field `{}.{}` derived from `{}` references identifier(s) `{}` that don't resolve to a sibling field on resource `{}`.",
                        resource.name,
                        field.name,
                        expr,
                        unresolved.join("`, `"),
                        resource.name,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }
    diagnostics
}

/// Tokenise a `derived from` expression, drop operators / numerics /
/// string literals / dotted paths / keywords, and return identifiers
/// that don't resolve to any name in `siblings`. The check is
/// intentionally conservative — over-rejecting an identifier the
/// runtime would have accepted is a Warning, not an Error, so a
/// false positive nudges the author to rename / annotate rather than
/// blocking the commit.
pub(crate) fn collect_unresolved_field_refs(expr: &str, siblings: &BTreeSet<&str>) -> Vec<String> {
    // Strip string literals (single + double quoted) first so their
    // contents don't masquerade as identifiers.
    let mut buf = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();
    let mut in_string: Option<char> = None;
    while let Some(c) = chars.next() {
        match in_string {
            Some(quote) => {
                if c == quote {
                    in_string = None;
                    buf.push(' ');
                } else if c == '\\' {
                    chars.next();
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_string = Some(c);
                } else {
                    buf.push(c);
                }
            }
        }
    }
    // Replace non-identifier-char with whitespace so split tokenises
    // cleanly.
    let normalised: String = buf
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let keywords: &[&str] = &[
        "and", "or", "not", "true", "false", "nil", "null", "ctx", "now", "self", "target",
    ];

    let mut out: Vec<String> = Vec::new();
    for raw in normalised.split_whitespace() {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        // Drop dotted paths (relation traversal — v1 limit).
        if token.contains('.') {
            continue;
        }
        // Drop numeric literals.
        if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // Drop keywords.
        if keywords.contains(&token.to_ascii_lowercase().as_str()) {
            continue;
        }
        // Identifiers must start with letter / underscore.
        if !token
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            continue;
        }
        if siblings.contains(token) {
            continue;
        }
        if !out.iter().any(|s| s == token) {
            out.push(token.to_owned());
        }
    }
    out
}

/// `resource_unique_qualifier_unknown` — Tier 4c lint per the
/// naming-reconciliation proposal §4 row 1 (NEW producer). Warns when
/// a `unique <field> per <qualifier>` constraint names a `<qualifier>`
/// that is not a field on the same resource. The runtime ignores
/// unknown qualifiers silently; the lint surfaces the gap at design
/// time so SQL composes the intended composite unique index.
pub(crate) fn resource_unique_qualifier_unknown_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use lazuli_ir::Constraint;
    let mut diagnostics = Vec::new();
    for feature in tier3_facts {
        for resource in &feature.resources {
            let field_names: BTreeSet<&str> =
                resource.fields.iter().map(|f| f.name.as_str()).collect();
            for constraint in &resource.constraints {
                let Constraint::Unique(unique) = constraint else {
                    continue;
                };
                let Some(qualifier) = unique.per.as_deref() else {
                    continue;
                };
                // The qualifier itself may be a known tenant axis
                // (`org` / `team` — see `tenancy_axis_for`). Skip those
                // even when the resource doesn't declare a literal
                // `org` field; the runtime resolves them through the
                // feature's `defaults.tenancy`.
                if matches!(qualifier, "org" | "team" | "tenant") {
                    continue;
                }
                if field_names.contains(qualifier) {
                    continue;
                }
                let line = resource
                    .span_ref
                    .as_ref()
                    .map(|s| line_col_for_offset_in_file(&feature.path, s.start).0)
                    .unwrap_or(feature.feature_line);
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "resource_unique_qualifier_unknown".to_owned(),
                    message: format!(
                        "resource `{}` declares `unique {} per {}` but `{}` is not a sibling field. The runtime will silently ignore the qualifier and emit a non-tenant-scoped UNIQUE index.",
                        resource.name,
                        unique.fields.join(", "),
                        qualifier,
                        qualifier,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }
    diagnostics
}

/// `resource_validates_path_unknown` — Tier 4c lint per the
/// naming-reconciliation proposal §4 row 2 (NEW producer). Two checks
/// fire:
///
/// 1. `validates field <field> ...` — `<field>` must be a sibling on
///    the same resource.
/// 2. `validates field <field> @validator.<name>` — `<name>` must be
///    declared under `extensions` with the `Validator` contract.
///
/// The LSP proxy `validation-syntax` (`lazuli_lsp/src/lib.rs:5987`)
/// only catches malformed syntax; this lint is the cross-reference.
pub(crate) fn resource_validates_path_unknown_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use lazuli_ir::ExtensionContract;
    let mut diagnostics = Vec::new();
    for feature in tier3_facts {
        let validator_names: BTreeSet<&str> = feature
            .extensions
            .iter()
            .filter(|e| matches!(e.contract, ExtensionContract::Validator { .. }))
            .map(|e| e.name.as_str())
            .collect();

        for resource in &feature.resources {
            let field_names: BTreeSet<&str> =
                resource.fields.iter().map(|f| f.name.as_str()).collect();

            for v in &resource.validates {
                let line = resource
                    .span_ref
                    .as_ref()
                    .map(|s| line_col_for_offset_in_file(&feature.path, s.start).0)
                    .unwrap_or(feature.feature_line);

                // Check 1: field exists.
                if !field_names.contains(v.field.as_str()) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "resource_validates_path_unknown".to_owned(),
                        message: format!(
                            "resource `{}` declares `validates field {}` but `{}` is not a field on this resource. Available fields: {}.",
                            resource.name,
                            v.field,
                            v.field,
                            field_names
                                .iter()
                                .copied()
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                    continue;
                }

                // Check 2: @validator.<name> resolves through extensions.
                if let Some(rest) = v.path.path.strip_prefix("@validator.") {
                    let name = rest
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or(rest);
                    if !name.is_empty() && !validator_names.contains(name) {
                        let known: Vec<&str> = validator_names.iter().copied().collect();
                        diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Warning,
                            code: "resource_validates_path_unknown".to_owned(),
                            message: format!(
                                "resource `{}.{}` validates against `@validator.{}` but no `validator {}` is declared under the feature's `extensions` block. Declared validators: {}.",
                                resource.name,
                                v.field,
                                name,
                                name,
                                if known.is_empty() { "(none)".to_owned() } else { known.join(", ") },
                            ),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
            }
        }
    }
    diagnostics
}
