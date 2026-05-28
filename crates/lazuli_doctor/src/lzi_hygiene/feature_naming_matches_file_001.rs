//! `LZI-FEATURE-NAMING-MATCHES-FILE-001` — flag `.lzi` files whose
//! basename doesn't appear as one of the declared feature names.
//!
//! Rails' "file name organizes, header decides" convention adapted to
//! Lazuli (see `docs/design-principles.md` "File Name Organizes, Header
//! Decides"). Catches drift when a file is renamed without updating its
//! `feature <Name>` header, or vice versa.
//!
//! ## When the rule fires
//!
//! For each `.lzi` file with at least one `feature <Name>` declaration:
//! - extract the file's stem (`features/billing/billing.lzi` → `billing`)
//! - extract every declared feature's name + lowercase + normalize
//!   dashes to underscores
//! - require at least one declared feature name match the stem
//!
//! Files with zero feature declarations (e.g. `app.lzi`) are already
//! filtered by the walker via [`crate::lzi_hygiene::walker::is_exempt_path`].
//!
//! ## Example fire
//!
//! - File `features/billing/payments.lzi` declares only `feature
//!   subscription` — stem `payments` doesn't appear in `{subscription}`.
//!   Fires.
//!
//! ## Example silent
//!
//! - File `features/billing/billing.lzi` declares `feature billing` and
//!   `feature billing_admin` — stem `billing` matches the primary
//!   feature exactly. Silent.
//! - File `features/customer/customer.lzi` declares `feature customer`,
//!   `feature customer_auth`, `feature customer_tags` (the
//!   `examples/full-capsule/full-capsule.lzi` shape, renamed to
//!   `customer.lzi`) — stem `customer` matches the first.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.

use std::path::{Path, PathBuf};

use lazuli_syntax::parse_feature_skeletons;

use crate::lzi_hygiene::walker::LziSourceFile;

/// One `.lzi` file whose basename doesn't appear in its declared
/// feature names.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// File stem (basename without `.lzi` extension).
    pub file_stem: String,
    /// Names of features declared in the file.
    pub feature_names: Vec<String>,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "LZI-FEATURE-NAMING-MATCHES-FILE-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::feature_naming_matches_file_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("features/billing/payments.lzi"),
    ///     file_stem: "payments".to_string(),
    ///     feature_names: vec!["subscription".to_string()],
    /// };
    /// assert!(f.message().contains("payments"));
    /// assert!(f.message().contains("subscription"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}: file stem `{}` does not match any declared feature name \
             ({:?}). Rails convention: `<feature>.lzi` declares `feature \
             <Feature>` as its anchor. Rename the file to match the \
             primary feature, or rename the feature to match the file.",
            self.path.display(),
            self.file_stem,
            self.feature_names,
        )
    }
}

/// Run the rule against the pre-walked `.lzi` source files. Returns one
/// finding per file whose stem doesn't appear in its declared features.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::feature_naming_matches_file_001::check;
/// use lazuli_doctor::lzi_hygiene::walker::LziSourceFile;
/// use std::path::PathBuf;
///
/// let f = LziSourceFile {
///     relative_path: PathBuf::from("features/billing/payments.lzi"),
///     absolute_path: PathBuf::from("/abs/features/billing/payments.lzi"),
///     source: "feature subscription\n".to_string(),
///     loc_count: 1,
/// };
/// // Stem `payments` doesn't appear in `[subscription]` → fires.
/// assert_eq!(check(&[f]).len(), 1);
/// ```
pub fn check(files: &[LziSourceFile]) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files {
        let stem = match file_stem(&file.relative_path) {
            Some(s) => s,
            None => continue,
        };
        // Parse-error files surface elsewhere; silently skip here so
        // the rule never double-reports a known-broken file.
        let features = match parse_feature_skeletons(&file.source) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if features.is_empty() {
            // Zero-feature files were filtered by the walker for the
            // canonical top-level cases; any survivor here is an
            // unusual file (e.g. comment-only). Don't fire.
            continue;
        }
        let names: Vec<String> = features.iter().map(|f| f.name.clone()).collect();
        let stem_norm = normalize(&stem);
        let any_match = names
            .iter()
            .any(|n| normalize(n) == stem_norm || normalize(n).starts_with(&stem_norm));
        if !any_match {
            out.push(Finding {
                path: file.relative_path.clone(),
                file_stem: stem,
                feature_names: names,
            });
        }
    }
    out
}

fn file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_owned())
}

fn normalize(s: &str) -> String {
    s.to_ascii_lowercase().replace('-', "_")
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
    fn stem_matches_single_feature_silent() {
        let f = file(
            "features/billing/billing.lzi",
            "feature billing\n  description \"\"\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn stem_matches_one_of_many_features_silent() {
        let f = file(
            "features/customer/customer.lzi",
            "feature customer\nfeature customer_auth\nfeature customer_tags\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn stem_matches_via_prefix_silent() {
        // The customer.lzi file is renamed to customer-core.lzi but
        // still declares `customer` as primary — the stem `customer-core`
        // prefix-matches `customer_core` after normalization, which
        // doesn't match `customer`. So this case actually fires; we
        // require true match OR prefix-match where the feature name is
        // the prefix of the stem.
        // Conversely, when stem = "customer" and feature = "customer_auth",
        // stem.starts_with(feature) is false; feature.starts_with(stem)
        // is true — the rule accepts that.
        let f = file(
            "features/customer/customer.lzi",
            "feature customer_auth\nfeature customer_billing\n",
        );
        // stem = "customer"; both features start with "customer" after
        // normalization → match via starts_with.
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn stem_unrelated_to_features_fires() {
        let f = file(
            "features/billing/payments.lzi",
            "feature subscription\nfeature invoice\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file_stem, "payments");
        assert_eq!(findings[0].feature_names.len(), 2);
    }

    #[test]
    fn dash_underscore_difference_silent() {
        let f = file(
            "features/customer-auth/customer-auth.lzi",
            "feature customer_auth\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn case_mismatch_silent() {
        let f = file("features/Billing/Billing.lzi", "feature billing\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn parse_error_silent_not_panic() {
        // If the parser bails, the rule must not panic. Other rules
        // surface the parse error separately.
        let f = file("features/x/x.lzi", "this is not lzi syntax\n");
        let _ = check(&[f]); // must not panic
    }

    #[test]
    fn message_includes_stem_and_features() {
        let f = file("features/billing/payments.lzi", "feature subscription\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("payments"));
        assert!(msg.contains("subscription"));
    }
}
