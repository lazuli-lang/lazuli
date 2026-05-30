//! Namespaced-atom check for `policy <expr>` statements + the
//! `policies` dictionary atom catalog.
//!
//! Commands and workflows must reference feature-local policy
//! categories with `@policy.*`; atoms in the `policies` dictionary
//! must be namespaced (`@role.*`, `@scope.*`, `@actor.*`).
//!
//! Re-exports helpers (`policy_statement_ref`, `is_namespaced_atom`,
//! `collect_policy_categories`, `policy_atoms_from_dictionary_line`)
//! consumed by sibling catalogs through the
//! `pub(crate) use diagnostics::policy::*;` re-export.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{feature_name, leading_spaces, simple_canonical_diagnostic};

pub(crate) fn policy_namespace_diagnostics(source: &str) -> Vec<Diagnostic> {
    let policy_categories = collect_policy_categories(source);
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
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
            _ => {}
        }

        if current_top == Some("policies") {
            for atom in policy_atoms_from_dictionary_line(trimmed) {
                if !is_namespaced_atom(atom) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "policy-atom-namespace",
                        "policy atoms should be namespaced by category, e.g. `@role.admin`, `@scope.same_org`, `@actor.system`, or `@scope.public`.",
                    ));
                    break;
                }
            }
        }

        let Some(policy_ref) = policy_statement_ref(trimmed) else {
            continue;
        };

        // RB.S6 — structured `policy <expr>` form. The first token may
        // be `authenticated`, `has_role`, `has_permission`, `not`, or
        // `(` — all valid expression heads. Skip the legacy single-atom
        // check; the parser already validated the expression shape.
        if matches!(
            policy_ref,
            "authenticated" | "has_role" | "has_permission" | "not"
        ) || policy_ref.starts_with('(')
        {
            continue;
        }

        let is_policy_category_ref = policy_ref.strip_prefix("@policy.").unwrap_or(policy_ref);

        let is_local_category = current_feature
            .as_ref()
            .and_then(|feature| policy_categories.get(feature))
            .is_some_and(|categories| categories.contains(is_policy_category_ref));

        // SPEC-07 — UNIFORM policy reference across ALL callables. The
        // former command/workflow-only `@policy.*` requirement is gone:
        // every callable (command, workflow, query, api, job, webhook,
        // escape_route, transition, view guard, route_guard, policy_for)
        // accepts the SAME grammar — `@policy.<category>`, a namespaced
        // atom (`@role.*`/`@scope.*`/`@actor.*`), or a structured policy
        // expression. The rules below no longer branch on `current_top`.
        if policy_ref.starts_with("@policy.") {
            if !is_local_category {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "policy-ref-namespace",
                    "`@policy.*` references should resolve to a feature-local policy category.",
                ));
            }
            continue;
        }

        if is_namespaced_atom(policy_ref) {
            continue;
        }

        if policy_ref.contains('.') && !policy_ref.starts_with('@') {
            continue;
        }

        if is_local_category {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "feature-local policy categories should be referenced with `@policy.*`, e.g. `policy @policy.create`, to distinguish them from built-in actors, roles, and scopes.",
            ));
            continue;
        }

        if !is_local_category {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "direct policy atoms should be namespaced, e.g. `policy @actor.system` or `policy @role.admin`. Feature-local policy categories use `@policy.*`, e.g. `policy @policy.create`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn collect_policy_categories(source: &str) -> HashMap<String, HashSet<String>> {
    let mut categories: HashMap<String, HashSet<String>> = HashMap::new();
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
                let Some(feature_name) = current_feature.as_deref() else {
                    continue;
                };
                let Some((name, _)) = trimmed.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty() || name == "fields" || name.contains(' ') {
                    continue;
                }
                categories
                    .entry(feature_name.to_owned())
                    .or_default()
                    .insert(name.to_owned());
            }
            _ => {}
        }
    }

    categories
}

pub(crate) fn policy_atoms_from_dictionary_line(trimmed_line: &str) -> Vec<&str> {
    let Some((_, rhs)) = trimmed_line.split_once(':') else {
        return Vec::new();
    };

    if rhs.trim_start().starts_with('"') {
        return Vec::new();
    }

    rhs.split(',')
        .map(str::trim)
        .filter(|atom| !atom.is_empty())
        .collect()
}

pub(crate) fn policy_statement_ref(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "policy" {
        parts.next()
    } else {
        None
    }
}

pub(crate) fn is_namespaced_atom(atom: &str) -> bool {
    matches!(
        atom.strip_prefix('@').and_then(|rest| rest.split_once('.')),
        Some(("role" | "scope" | "actor", name)) if !name.is_empty()
    )
}
