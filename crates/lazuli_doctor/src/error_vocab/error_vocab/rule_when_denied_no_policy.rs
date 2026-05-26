//! ERR-VOCAB-WHEN-DENIED-NO-POLICY — `when_denied` authored where nothing
//! can deny. Override would never fire.
//!
//! Two sites:
//!  1. Per-command `policy_when_denied: Some(_)` with `policy ==
//!     PolicyRef::None` and `policy_expr == None` — proposal §6.6 strict
//!     wording.
//!  2. Per-policy `PolicyCategory.when_denied: Some(_)` with empty
//!     `atoms` — the category declares no atoms so nothing can fail.
//!     Mirrors the task's fixture intent (a `policies.X:` line that has
//!     no policy expression on the right-hand side).
//!
//! Both forms are dead code; reporting them together keeps the rule's
//! surface coherent.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6
//! ERR-VOCAB-WHEN-DENIED-NO-POLICY.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PolicyRef, SpanRef};

/// One ERR-VOCAB-WHEN-DENIED-NO-POLICY finding — a `when_denied`
/// override was authored at a site where nothing can deny, so the
/// override is dead code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenDeniedNoPolicyFinding {
    /// Source `.lzi` file the offending site lives in.
    pub path: PathBuf,
    /// Feature owning the site.
    pub feature: String,
    /// Authoring site for the dead `when_denied`. Two shapes are
    /// reachable:
    ///  * `Command(<name>)` — per-command `policy_when_denied`.
    ///  * `Policy(<name>)`  — per-policy `PolicyCategory.when_denied`.
    pub site: WhenDeniedSite,
    /// Bare translation key the override referenced.
    pub key: String,
    /// Source span of the offending site for IDE squiggles.
    pub span: Option<SpanRef>,
}

/// Where the dead `when_denied` was authored — command-side override
/// vs policy-category override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenDeniedSite {
    /// Per-command `policy_when_denied` with no gating policy.
    Command(String),
    /// Per-policy `PolicyCategory.when_denied` with empty atom list.
    Policy(String),
}

impl WhenDeniedNoPolicyFinding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ERR-VOCAB-WHEN-DENIED-NO-POLICY";

    /// Render the per-site message — command vs policy-category dead
    /// code — each pointing at the canonical fix.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::error_vocab::error_vocab::rule_when_denied_no_policy::{
    ///     WhenDeniedNoPolicyFinding, WhenDeniedSite,
    /// };
    ///
    /// let f = WhenDeniedNoPolicyFinding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     site: WhenDeniedSite::Command("send_invoice".into()),
    ///     key: "denied".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("Remove the `when_denied` line"));
    /// ```
    pub fn message(&self) -> String {
        match &self.site {
            WhenDeniedSite::Command(command) => format!(
                "command `{}.{}` declares `when_denied @translation.{}` but has no `policy`. \
                 Remove the `when_denied` line, or add a `policy` to gate the command.",
                self.feature, command, self.key
            ),
            WhenDeniedSite::Policy(policy) => format!(
                "policy `{}.{}` declares `when_denied @translation.{}` but its atom list is \
                 empty. The override is dead code — declare at least one atom \
                 (e.g. `{}: @role.admin`) or remove the `when_denied` line.",
                self.feature, policy, self.key, policy
            ),
        }
    }
}

/// Run ERR-VOCAB-WHEN-DENIED-NO-POLICY over one feature.
///
/// Walks per-command and per-policy `when_denied` sites and emits one
/// finding per dead override.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::error_vocab::error_vocab::rule_when_denied_no_policy::check_when_denied_no_policy;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with `when_denied`");
/// let _ = check_when_denied_no_policy(&feature, Path::new("billing.lzi"));
/// ```
pub fn check_when_denied_no_policy(
    feature: &Feature,
    path: &Path,
) -> Vec<WhenDeniedNoPolicyFinding> {
    let mut findings = Vec::new();
    // Per-command site.
    for cmd in &feature.commands {
        let Some(reference) = &cmd.policy_when_denied else {
            continue;
        };
        if matches!(cmd.policy, PolicyRef::None) && cmd.policy_expr.is_none() {
            findings.push(WhenDeniedNoPolicyFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                site: WhenDeniedSite::Command(cmd.name.clone()),
                key: reference.key.clone(),
                span: reference.span_ref.or(cmd.span_ref),
            });
        }
    }
    // Per-policy site.
    for category in &feature.policies.categories {
        let Some(reference) = &category.when_denied else {
            continue;
        };
        if category.atoms.is_empty() {
            findings.push(WhenDeniedNoPolicyFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                site: WhenDeniedSite::Policy(category.name.clone()),
                key: reference.key.clone(),
                span: reference.span_ref,
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_code_is_stable() {
        assert_eq!(
            WhenDeniedNoPolicyFinding::CODE,
            "ERR-VOCAB-WHEN-DENIED-NO-POLICY"
        );
    }

    #[test]
    fn command_site_message_prompts_remove_or_add_policy() {
        let f = WhenDeniedNoPolicyFinding {
            path: PathBuf::from("billing.lzi"),
            feature: "billing".to_owned(),
            site: WhenDeniedSite::Command("send_invoice".to_owned()),
            key: "denied".to_owned(),
            span: None,
        };
        let msg = f.message();
        assert!(msg.contains("send_invoice"));
        assert!(msg.contains("Remove the `when_denied` line"));
    }

    #[test]
    fn policy_site_message_prompts_atom_or_remove() {
        let f = WhenDeniedNoPolicyFinding {
            path: PathBuf::from("billing.lzi"),
            feature: "billing".to_owned(),
            site: WhenDeniedSite::Policy("update".to_owned()),
            key: "denied".to_owned(),
            span: None,
        };
        let msg = f.message();
        assert!(msg.contains("update"));
        assert!(msg.contains("atom list is empty"));
    }
}
