//! `LZI-FEATURE-COHESION-001` — flag `.lzi` files containing multiple
//! features that don't share a domain prefix.
//!
//! Architect-review-driven alternative to the rejected
//! `LZI-FEATURE-PER-FILE-001`. Lazuli `feature` is a namespacing /
//! grouping primitive — multi-feature files are fine WHEN the features
//! are cohesive (share an anchor / resource / domain prefix). The
//! arbitrary-bundling anti-pattern (unrelated features dumped in the
//! same file) is the real failure mode.
//!
//! ## How "cohesion" is detected (v1: prefix heuristic)
//!
//! For a file with N ≥ 2 declared features, compute the longest common
//! prefix of all feature names (case-insensitive, on the underscore-
//! separated token boundary). The features are considered cohesive iff
//! the prefix contains at least one non-empty token. Examples:
//!
//! - `[customer, customer_auth, customer_tags]` → prefix `customer` →
//!   cohesive (silent)
//! - `[billing, invoice, subscription]` → prefix `` (empty) → not
//!   cohesive (fires)
//! - `[user_login, user_signup]` → prefix `user` → cohesive (silent)
//!
//! Single-feature files are silent (no cohesion question).
//!
//! ## Default severity
//!
//! `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## v2 future
//!
//! Currently the rule only inspects feature names. v2 could parse each
//! feature's `resource` / `@policy.*` / `extends` declarations and look
//! for shared anchors as additional evidence of cohesion. Out of scope
//! for v1: name-prefix is a sufficient leading indicator.

use std::path::PathBuf;

use lazuli_syntax::parse_feature_skeletons;

use crate::lzi_hygiene::walker::LziSourceFile;

/// One `.lzi` file with multiple features lacking a common prefix.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// Names of declared features in the file.
    pub feature_names: Vec<String>,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "LZI-FEATURE-COHESION-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::feature_cohesion_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/mixed/mixed.lzi"),
    ///     feature_names: vec![
    ///         "billing".to_string(),
    ///         "invoice".to_string(),
    ///         "subscription".to_string(),
    ///     ],
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("mixed.lzi"));
    /// assert!(msg.contains("billing"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}: declares {} unrelated features {:?} with no shared name \
             prefix — Lazuli `feature` is a namespacing primitive, so \
             co-locating unrelated features fragments cold-read. Split \
             each into its own `.lzi`, OR rename them to share a domain \
             prefix (e.g. `acme_billing`, `acme_invoice`) if they really \
             do form one bounded context.",
            self.path.display(),
            self.feature_names.len(),
            self.feature_names,
        )
    }
}

/// Run the rule against the pre-walked `.lzi` source files. Returns one
/// finding per file whose declared features don't share a non-empty
/// common prefix.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::feature_cohesion_001::check;
/// use lazuli_doctor::lzi_hygiene::walker::LziSourceFile;
/// use std::path::PathBuf;
///
/// // Cohesive bundle (shared `customer` prefix) → silent.
/// let cohesive = LziSourceFile {
///     relative_path: PathBuf::from("features/customer/customer.lzi"),
///     absolute_path: PathBuf::from("/abs/features/customer/customer.lzi"),
///     source: "feature customer\nfeature customer_auth\n".to_string(),
///     loc_count: 2,
/// };
/// assert!(check(&[cohesive]).is_empty());
/// ```
pub fn check(files: &[LziSourceFile]) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files {
        let features = match parse_feature_skeletons(&file.source) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if features.len() < 2 {
            continue;
        }
        let names: Vec<String> = features.iter().map(|f| f.name.clone()).collect();
        if shares_token_prefix(&names) {
            continue;
        }
        out.push(Finding {
            path: file.relative_path.clone(),
            feature_names: names,
        });
    }
    out
}

/// Compute whether the names share at least one non-empty leading
/// underscore-separated token.
///
/// Case-insensitive; treats `-` and `_` as equivalent separators.
/// Returns `true` iff the longest common prefix (token-wise) contains
/// at least one non-empty token.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::feature_cohesion_001::shares_token_prefix;
///
/// assert!(shares_token_prefix(&[
///     "customer".to_string(),
///     "customer_auth".to_string(),
///     "customer_tags".to_string(),
/// ]));
/// assert!(!shares_token_prefix(&[
///     "billing".to_string(),
///     "invoice".to_string(),
/// ]));
/// assert!(shares_token_prefix(&[
///     "user-login".to_string(),
///     "user_signup".to_string(),
/// ]));
/// ```
pub fn shares_token_prefix(names: &[String]) -> bool {
    if names.len() < 2 {
        return true;
    }
    let token_lists: Vec<Vec<String>> = names
        .iter()
        .map(|n| {
            n.to_ascii_lowercase()
                .replace('-', "_")
                .split('_')
                .filter(|t| !t.is_empty())
                .map(|s| s.to_owned())
                .collect()
        })
        .collect();
    let first = &token_lists[0];
    if first.is_empty() {
        return false;
    }
    let first_tok = &first[0];
    token_lists[1..].iter().all(|toks| {
        toks.first().map(|t| t == first_tok).unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str, source: &str) -> LziSourceFile {
        let p = PathBuf::from(rel);
        LziSourceFile {
            relative_path: p.clone(),
            absolute_path: p,
            source: source.to_owned(),
            loc_count: source.lines().count(),
        }
    }

    #[test]
    fn single_feature_silent() {
        assert!(check(&[file("features/x/x.lzi", "feature billing\n")]).is_empty());
    }

    #[test]
    fn cohesive_prefix_silent() {
        let f = file(
            "features/customer/customer.lzi",
            "feature customer\nfeature customer_auth\nfeature customer_tags\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn arbitrary_bundling_fires() {
        let f = file(
            "features/mixed/mixed.lzi",
            "feature billing\nfeature invoice\nfeature subscription\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature_names.len(), 3);
    }

    #[test]
    fn dash_treated_as_underscore() {
        let f = file(
            "features/u/u.lzi",
            "feature user-login\nfeature user_signup\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn case_insensitive_prefix() {
        let f = file(
            "features/x/x.lzi",
            "feature CustomerCore\nfeature customercore_auth\n",
        );
        // First token differs: "customercore" vs "customercore" — same.
        // Actually identical after lowercasing. Silent.
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn two_features_no_shared_prefix_fires() {
        let f = file(
            "features/x/x.lzi",
            "feature billing\nfeature reports\n",
        );
        assert_eq!(check(&[f]).len(), 1);
    }

    #[test]
    fn parse_error_does_not_panic() {
        let f = file("features/x/x.lzi", "garbage that's not lzi\n");
        let _ = check(&[f]);
    }

    #[test]
    fn shares_token_prefix_basic() {
        assert!(shares_token_prefix(&[
            "customer".to_string(),
            "customer_auth".to_string(),
        ]));
        assert!(!shares_token_prefix(&[
            "billing".to_string(),
            "invoice".to_string(),
        ]));
        assert!(shares_token_prefix(&["solo".to_string()])); // < 2 → vacuously true
    }

    #[test]
    fn message_includes_path_and_feature_count() {
        let f = file(
            "features/mixed/mixed.lzi",
            "feature alpha\nfeature beta\n",
        );
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("mixed.lzi"));
        assert!(msg.contains("2"));
    }
}
