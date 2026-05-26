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

/// Resources with fewer post-filter fields than this don't trigger the
/// rule — small resources can't host a meaningful cluster signal.
pub const DEFAULT_MIN_RESOURCE_FIELDS: usize = 10;

/// Cluster must contain at least this many fields before it counts as
/// extractable. Below this threshold the cluster is treated as noise.
pub const DEFAULT_MIN_CLUSTER_FIELDS: usize = 4;

/// Tokens excluded from cluster matching. These are domain-universal name
/// fragments that would otherwise cluster spuriously (`*_id`, `*_at`,
/// `*_by`, etc.).
pub const DEFAULT_EXCLUDED_TOKENS: &[&str] = &[
    "id", "at", "by", "count", "total", "org", "tenant",
];

/// Position of the shared snake-case token inside the clustered field
/// names — drives the prose in the diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenPosition {
    /// Token sits at the front of every field in the cluster
    /// (e.g. `host_phone`, `host_email`, `host_avatar`).
    Leading,
    /// Token sits at the tail of every field in the cluster
    /// (e.g. `phone_host`, `email_host`, `avatar_host`).
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

/// One VOCAB-RESOURCE-WIDE-CLUSTER-001 finding — a resource whose
/// post-filter fields share a common leading/trailing snake-case token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file that hosts the resource.
    pub path: PathBuf,
    /// Feature owning the resource.
    pub feature: String,
    /// Resource whose fields cluster on a shared token.
    pub resource: String,
    /// Total post-filter authored field count on the resource.
    pub total_fields: usize,
    /// The shared snake-case token (e.g. `host`).
    pub token: String,
    /// Whether the shared token is at the head or tail of each field name.
    pub token_position: TokenPosition,
    /// Member field names that share the token.
    pub cluster_fields: Vec<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-RESOURCE-WIDE-CLUSTER-001";

    /// Render the diagnostic message naming the cluster and prompting
    /// for either record extraction or a documented `# doctor:allow`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_resource_wide_cluster_001::{Finding, TokenPosition};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "catalog".into(),
    ///     resource: "Property".into(),
    ///     total_fields: 12,
    ///     token: "host".into(),
    ///     token_position: TokenPosition::Leading,
    ///     cluster_fields: vec!["host_email".into(), "host_phone".into(), "host_avatar".into(), "host_bio".into()],
    /// };
    /// assert!(f.message().contains("extracting a `record`"));
    /// ```
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

/// Run VOCAB-RESOURCE-WIDE-CLUSTER-001 with default thresholds.
///
/// Delegates to [`check_with_config`] using the
/// `DEFAULT_MIN_RESOURCE_FIELDS` / `DEFAULT_MIN_CLUSTER_FIELDS` /
/// `DEFAULT_EXCLUDED_TOKENS` constants. Tests that vary the cluster
/// threshold call `check_with_config` directly.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_resource_wide_cluster_001::check;
/// use lazuli_ir::{Feature, Module};
///
/// let module: Module = unimplemented!();
/// let feature = &module.features[0];
/// let _ = check(feature, &module, Path::new("catalog.lzi"));
/// ```
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

/// Run VOCAB-RESOURCE-WIDE-CLUSTER-001 with caller-tuned thresholds.
///
/// Exposed so unit tests and downstream tooling can vary the field-count
/// / cluster-size cutoffs without touching the defaults.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_resource_wide_cluster_001::check_with_config;
/// use lazuli_ir::{Feature, Module};
///
/// let module: Module = unimplemented!();
/// let feature = &module.features[0];
/// let _ = check_with_config(feature, &module, Path::new("catalog.lzi"), 8, 3, &["id", "at"]);
/// ```
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
