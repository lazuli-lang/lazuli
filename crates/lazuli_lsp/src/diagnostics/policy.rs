//! Diagnostics for the policy/scope/refs family.
//!
//! The closed-namespace policy vocabulary (`@policy.*`, `@role.*`,
//! `@scope.*`, `@actor.*`) is one of the highest-stakes surfaces in
//! Lazuli — it's the input to every code-generation step and to every
//! audit. This module owns every file-local check on that vocabulary:
//!
//! | Producer | Concern |
//! |---|---|
//! | [`refs_block_diagnostics`] | `feature.refs` must list exactly the namespaces the feature uses — extra → unused warning, missing → unused warning. |
//! | [`policy_namespace_diagnostics`] | every `policy <expr>` statement targets a namespaced atom (`@role.*`, `@scope.*`, `@actor.*`) or a feature-local `@policy.<category>`. Commands/workflows must use `@policy.*`. |
//! | [`scope_override_policy_diagnostics`] | `scope override` inside a query block requires an explicit `policy` and a `reason` child. |
//! | [`command_rate_limit_contract_diagnostics`] | public or mutating commands must declare `rate_limit ...` or explicit `rate_limit none` with a `reason`. |
//!
//! Helpers exposed at `crate::*` for use by other catalog modules
//! (`policy_statement_ref`, `is_namespaced_atom`, `collect_policy_atom_map`,
//! `policy_ref_is_public`, etc.) live here too — their consumers are all
//! policy-adjacent.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    command_write_effect, feature_name, leading_spaces, namespace_references,
    simple_canonical_diagnostic,
};

#[derive(Debug, Default)]
pub(crate) struct FeatureRefsFacts {
    name: String,
    refs_line: Option<(usize, String)>,
    declared: HashSet<String>,
    used: HashSet<String>,
}

pub(crate) fn refs_block_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current: Option<FeatureRefsFacts> = None;
    let mut current_top: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(facts) = current.take() {
                diagnostics.extend(refs_facts_diagnostics(facts));
            }
            current = Some(FeatureRefsFacts {
                name: feature_name(trimmed),
                ..FeatureRefsFacts::default()
            });
            current_top = None;
            continue;
        }

        let Some(facts) = current.as_mut() else {
            continue;
        };

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
            if current_top == Some("refs") {
                facts.refs_line = Some((line_index, line.to_owned()));
            }
            continue;
        }

        if current_top == Some("refs") && leading_spaces(line) == 4 {
            if let Some((_, namespaces)) = trimmed.split_once(':') {
                for namespace in namespaces
                    .split(',')
                    .map(str::trim)
                    .filter_map(|namespace| namespace.strip_prefix('@'))
                {
                    facts.declared.insert(namespace.to_owned());
                }
            }
            continue;
        }

        for namespace in namespace_references(line) {
            facts.used.insert(namespace.to_owned());
        }
    }

    if let Some(facts) = current {
        diagnostics.extend(refs_facts_diagnostics(facts));
    }

    diagnostics
}

pub(crate) fn refs_facts_diagnostics(facts: FeatureRefsFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let Some((line_index, line)) = facts.refs_line else {
        return diagnostics;
    };

    let mut missing: Vec<_> = facts.used.difference(&facts.declared).cloned().collect();
    let mut unused: Vec<_> = facts.declared.difference(&facts.used).cloned().collect();
    missing.sort();
    unused.sort();

    if !missing.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "refs-missing",
            &format!(
                "refs for feature `{}` is missing used namespaces: {}.",
                facts.name,
                missing
                    .iter()
                    .map(|namespace| format!("@{namespace}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    if !unused.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "refs-unused",
            &format!(
                "refs for feature `{}` declares unused namespaces: {}.",
                facts.name,
                unused
                    .iter()
                    .map(|namespace| format!("@{namespace}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    diagnostics
}

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

        if matches!(current_top, Some("command" | "workflow"))
            && !policy_ref.starts_with("@policy.")
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "policy-ref-namespace",
                "commands and workflows should reference feature-local policy categories with `@policy.*`; put `@role.*`, `@scope.*`, or `@actor.*` atoms in the `policies` dictionary.",
            ));
            continue;
        }

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

#[derive(Debug)]
pub(crate) struct QuerySecurityFacts {
    line_index: usize,
    line: String,
    has_policy: bool,
    has_scope_override: bool,
    has_scope_override_reason: bool,
}

pub(crate) fn scope_override_policy_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<QuerySecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("query.") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(query_scope_override_diagnostics(query));
            }
            current_query = Some(QuerySecurityFacts {
                line_index,
                line: line.to_owned(),
                has_policy: false,
                has_scope_override: false,
                has_scope_override_reason: false,
            });
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if let Some(query) = current_query.take() {
                diagnostics.extend(query_scope_override_diagnostics(query));
            }
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 6 && trimmed.starts_with("policy ") {
            query.has_policy = true;
        } else if leading_spaces(line) == 6 && trimmed.starts_with("scope override") {
            query.has_scope_override = true;
        } else if leading_spaces(line) == 8 && trimmed.starts_with("reason ") {
            query.has_scope_override_reason = true;
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(query_scope_override_diagnostics(query));
    }

    diagnostics
}

pub(crate) fn query_scope_override_diagnostics(query: QuerySecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if query.has_scope_override && !query.has_policy {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "scope-override-policy",
            "`scope override` replaces inherited tenant/soft-delete safety scope; the query must declare an explicit `policy @policy.*`.",
        ));
    }

    if query.has_scope_override && !query.has_scope_override_reason {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "scope-override-reason",
            "`scope override` should include a `reason \"...\"` child explaining why inherited tenant/soft-delete scope is replaced.",
        ));
    }

    diagnostics
}

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
