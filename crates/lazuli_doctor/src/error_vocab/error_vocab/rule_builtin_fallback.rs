//! ERR-VOCAB-003 — command policy resolves to a `PolicyCategory` without
//! `when_denied`, AND the feature has no `errors policy_denied` catch-all.
//! Net result: built-in floor text is what the client receives.
//!
//! Severity: warning. Mirrors ERR-VOCAB-001 but operates at the
//! command-resolution level (per-command awareness, not per-feature).
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 ERR-VOCAB-003.

use std::path::{Path, PathBuf};

use lazuli_ir::{Command, Feature, PolicyCategory, PolicyExpr, PolicyRef, SpanRef};

use super::catalogs::has_policy_denied_catchall;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinFallbackFinding {
    pub path: PathBuf,
    pub feature: String,
    pub command: String,
    pub policy: String,
    pub span: Option<SpanRef>,
}

impl BuiltinFallbackFinding {
    pub const CODE: &'static str = "ERR-VOCAB-003";

    pub fn message(&self) -> String {
        format!(
            "command `{}.{}` is gated by policy `{}` which has no `when_denied`, and feature `{}` \
             declares no `errors policy_denied message @translation.<key>` catch-all. On \
             `policy_denied`, the client will see the framework's built-in localized message \
             (good floor, but not domain-specific). Consider adding `when_denied \
             @translation.<key>` to the policy or a feature-level catch-all.",
            self.feature, self.command, self.policy, self.feature
        )
    }
}

/// Run ERR-VOCAB-003 over one feature. Per-command bypass: if the command
/// already authors its own `policy_when_denied`, the chain resolves at
/// step 1 and the warning is silent.
pub fn check_builtin_fallback(feature: &Feature, path: &Path) -> Vec<BuiltinFallbackFinding> {
    if has_policy_denied_catchall(feature.errors.as_ref()) {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for cmd in &feature.commands {
        // Step 1: command supplies its own override -> chain resolves at
        // step 1 of proposal §2.E.
        if cmd.policy_when_denied.is_some() {
            continue;
        }
        // The command must actually be gated by a policy that could deny
        // — otherwise `policy_denied` never fires.
        if !command_has_denyable_policy(cmd) {
            continue;
        }
        let Some((policy_label, category)) = resolve_local_policy_category(cmd, &feature.policies)
        else {
            // Unresolved or external/atom-only policies leave the
            // chain at step 2 to the runtime built-in floor without
            // warning. `ERR-VOCAB-002` covers unresolved key refs; the
            // built-in fallback for atom policies is by-design.
            continue;
        };
        // Step 2: the resolved category authors its own `when_denied`.
        if category.when_denied.is_some() {
            continue;
        }
        findings.push(BuiltinFallbackFinding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            command: cmd.name.clone(),
            policy: policy_label,
            span: cmd.span_ref,
        });
    }
    findings
}

fn command_has_denyable_policy(cmd: &Command) -> bool {
    if !matches!(cmd.policy, PolicyRef::None) {
        return true;
    }
    cmd.policy_expr.as_ref().is_some_and(policy_expr_can_deny)
}

/// `policy_denied` can only fire when the expression has at least one
/// denyable branch — atom refs, has_role, has_permission, authenticated
/// (anonymous actor fails). Boolean combinators recurse.
fn policy_expr_can_deny(expr: &PolicyExpr) -> bool {
    match expr {
        PolicyExpr::Authenticated
        | PolicyExpr::HasRole(_)
        | PolicyExpr::HasPermission(_)
        | PolicyExpr::Atom(_) => true,
        PolicyExpr::And(parts) | PolicyExpr::Or(parts) => parts.iter().any(policy_expr_can_deny),
        PolicyExpr::Not(inner) => policy_expr_can_deny(inner),
    }
}

/// Resolves `cmd.policy` to a `PolicyCategory` within this feature.
///
/// Two surface forms map onto a local category:
///  * `PolicyRef::Local(name)`            — bare `policy update` form.
///  * `PolicyRef::Atom("policy.<name>")`  — `policy @policy.update` form;
///    the `@policy.<name>` namespace is canonical for naming local
///    policy categories from outside the `policies` block.
///
/// External atoms (`@role.*`, `@scope.*`, `@actor.*`) and unresolved /
/// external refs return `None` — the rule can't fire for them because
/// the resolution chain bypasses local categories entirely.
fn resolve_local_policy_category<'a>(
    cmd: &'a Command,
    policies: &'a lazuli_ir::Policies,
) -> Option<(String, &'a PolicyCategory)> {
    let candidate = match &cmd.policy {
        PolicyRef::Local(name) => Some(name.clone()),
        PolicyRef::Atom(atom) => atom.strip_prefix("policy.").map(|name| name.to_owned()),
        _ => None,
    }?;
    policies
        .categories
        .iter()
        .find(|c| c.name == candidate)
        .map(|c| (format!("@policy.{}", candidate), c))
}
