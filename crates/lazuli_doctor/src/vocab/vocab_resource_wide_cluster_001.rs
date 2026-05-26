//! VOCAB-RESOURCE-WIDE-CLUSTER-001 — single-resource name-token cluster.
//!
//! Per `docs/proposals/vocab-shadow-record-vo-extraction.md` v0.2 §4.2.
//! Fires when a resource has more than K post-filter authored fields AND
//! M or more of those fields share a common leading OR trailing snake-case
//! token. The trailing-token branch closes B4 from v0.1 review (suffix
//! gameability of the prefix-only detection).
//!
//! Each fire surfaces the LARGEST cluster per resource — subsequent rule
//! runs after the author refactors may surface the next-largest cluster.
//!
//! Severity: `warning` (strict), `info` (production).
//! Reference: docs/proposals/vocab-shadow-record-vo-extraction.md
//!            docs/next-checklist.md §VOCAB-RESOURCE-WIDE-CLUSTER-001

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Module};

use super::universal_columns::is_universal_column;

pub const DEFAULT_MIN_RESOURCE_FIELDS: usize = 10;
pub const DEFAULT_MIN_CLUSTER_FIELDS: usize = 4;

/// Tokens excluded from cluster matching. These are domain-universal name
/// fragments that would otherwise cluster spuriously (`*_id`, `*_at`,
/// `*_by`, etc.).
pub const DEFAULT_EXCLUDED_TOKENS: &[&str] = &[
    "id", "at", "by", "count", "total", "org", "tenant",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenPosition {
    Leading,
    Trailing,
}

impl TokenPosition {
    fn label(&self) -> &'static str {
        match self {
            TokenPosition::Leading => "leading",
            TokenPosition::Trailing => "trailing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    pub total_fields: usize,
    pub token: String,
    pub token_position: TokenPosition,
    pub cluster_fields: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-RESOURCE-WIDE-CLUSTER-001";

    pub fn message(&self) -> String {
        format!(
            "resource `{}` has {} authored fields and {} share {} token \
             `{}` ({}). Consider extracting a `record` for the cluster. If \
             the naming grouping is incidental, add \
             `# doctor:allow VOCAB-RESOURCE-WIDE-CLUSTER-001 — reason \"...\"` \
             on the resource.",
            self.resource,
            self.total_fields,
            self.cluster_fields.len(),
            self.token_position.label(),
            self.token,
            self.cluster_fields.join(", "),
        )
    }
}

pub fn check(feature: &Feature, module: &Module, path: &Path) -> Vec<Finding> {
    check_with_config(
        feature,
        module,
        path,
        DEFAULT_MIN_RESOURCE_FIELDS,
        DEFAULT_MIN_CLUSTER_FIELDS,
        DEFAULT_EXCLUDED_TOKENS,
    )
}

pub fn check_with_config(
    feature: &Feature,
    module: &Module,
    path: &Path,
    min_resource_fields: usize,
    min_cluster_fields: usize,
    excluded_tokens: &[&str],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for resource in &feature.resources {
        let post_filter_fields: Vec<&str> = resource
            .fields
            .iter()
            .filter(|f| !is_universal_column(f, &resource.name, feature, module))
            .map(|f| f.name.as_str())
            .collect();

        if post_filter_fields.len() <= min_resource_fields {
            continue;
        }

        // Aggregate leading + trailing token clusters in a single map keyed
        // by (position, token); pick the largest cluster overall.
        let mut clusters: HashMap<(TokenPosition, String), Vec<String>> = HashMap::new();
        for &name in &post_filter_fields {
            if let Some(token) = leading_token(name) {
                if is_eligible_token(&token, excluded_tokens) {
                    clusters
                        .entry((TokenPosition::Leading, token))
                        .or_default()
                        .push(name.to_owned());
                }
            }
            if let Some(token) = trailing_token(name) {
                if is_eligible_token(&token, excluded_tokens) {
                    clusters
                        .entry((TokenPosition::Trailing, token))
                        .or_default()
                        .push(name.to_owned());
                }
            }
        }

        let largest = clusters
            .into_iter()
            .filter(|(_, fields)| fields.len() >= min_cluster_fields)
            .max_by_key(|(_, fields)| fields.len());

        if let Some(((position, token), mut fields)) = largest {
            fields.sort();
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                resource: resource.name.clone(),
                total_fields: post_filter_fields.len(),
                token,
                token_position: position,
                cluster_fields: fields,
            });
        }
    }
    findings
}

fn leading_token(name: &str) -> Option<String> {
    let (head, rest) = name.split_once('_')?;
    if rest.is_empty() {
        return None;
    }
    Some(head.to_owned())
}

fn trailing_token(name: &str) -> Option<String> {
    let (rest, tail) = name.rsplit_once('_')?;
    if rest.is_empty() {
        return None;
    }
    Some(tail.to_owned())
}

fn is_eligible_token(token: &str, excluded: &[&str]) -> bool {
    if token.len() < 2 {
        return false;
    }
    !excluded.iter().any(|x| *x == token)
}

#[cfg(test)]
mod tests {
    include!("vocab_resource_wide_cluster_001_tests.rs");
}
