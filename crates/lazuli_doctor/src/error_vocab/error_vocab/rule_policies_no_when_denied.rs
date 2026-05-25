//! ERR-VOCAB-001 — feature has `policies` block but no `when_denied`
//! anywhere and no `errors policy_denied` catch-all.
//!
//! Severity: warning. The runtime still produces a localized message
//! (built-in PT-BR / en-US floor), but the author has no domain-specific
//! override.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 ERR-VOCAB-001.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, SpanRef};

use super::catalogs::has_policy_denied_catchall;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliciesNoWhenDeniedFinding {
    pub path: PathBuf,
    pub feature: String,
    /// 1-based source line of the `policies` header; `None` when the
    /// feature carries no span (programmatic construction).
    pub policies_span: Option<SpanRef>,
}

impl PoliciesNoWhenDeniedFinding {
    pub const CODE: &'static str = "ERR-VOCAB-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` declares policies but no `when_denied` overrides. Commands gated by \
             these policies will fall back to the framework's built-in PT-BR/en-US text. Add \
             `when_denied @translation.<key>` to each policy, or declare `errors policy_denied \
             message @translation.<key>` once for the feature, to customize the human-readable \
             phrasing.",
            self.feature
        )
    }
}

/// Run ERR-VOCAB-001 over one lowered feature.
///
/// Fires when:
/// 1. The feature authored a `policies` block (`policies.span_ref.is_some()`),
/// 2. No named policy in that block has `when_denied`,
/// 3. The feature has no `errors policy_denied message @translation.<key>`
///    catch-all (`feature.errors.messages` contains no `code == "policy_denied"`).
pub fn check_policies_no_when_denied(
    feature: &Feature,
    path: &Path,
) -> Vec<PoliciesNoWhenDeniedFinding> {
    // Step 1: the `policies` block must have been authored explicitly.
    // `Policies::default()` carries `span_ref: None`, so this distinguishes
    // "absent" from "declared but empty".
    if feature.policies.span_ref.is_none() {
        return Vec::new();
    }
    // Step 2: at least one named policy must exist — empty `policies {}`
    // can't trigger because there's nothing to gate against.
    if feature.policies.categories.is_empty() {
        return Vec::new();
    }
    // Step 3: any `when_denied` anywhere silences the warning.
    let any_when_denied = feature
        .policies
        .categories
        .iter()
        .any(|c| c.when_denied.is_some());
    if any_when_denied {
        return Vec::new();
    }
    // Step 4: a feature-level `errors policy_denied` catch-all also silences.
    if has_policy_denied_catchall(feature.errors.as_ref()) {
        return Vec::new();
    }
    vec![PoliciesNoWhenDeniedFinding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        policies_span: feature.policies.span_ref,
    }]
}
