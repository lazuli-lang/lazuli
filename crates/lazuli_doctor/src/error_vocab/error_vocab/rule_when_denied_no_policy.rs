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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenDeniedNoPolicyFinding {
    pub path: PathBuf,
    pub feature: String,
    /// Authoring site for the dead `when_denied`. Two shapes are
    /// reachable:
    ///  * `Command(<name>)` — per-command `policy_when_denied`.
    ///  * `Policy(<name>)`  — per-policy `PolicyCategory.when_denied`.
    pub site: WhenDeniedSite,
    pub key: String,
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenDeniedSite {
    Command(String),
    Policy(String),
}

impl WhenDeniedNoPolicyFinding {
    pub const CODE: &'static str = "ERR-VOCAB-WHEN-DENIED-NO-POLICY";

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
