//! `feature.refs` declared vs used namespace audit.
//!
//! Walks each feature once, accumulating the `refs` line + declared
//! namespaces + the set of namespaces actually referenced via
//! `@<namespace>.<name>` in the feature body, then emits
//! `refs-missing` / `refs-unused` warnings.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{feature_name, leading_spaces, namespace_references, simple_canonical_diagnostic};

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
