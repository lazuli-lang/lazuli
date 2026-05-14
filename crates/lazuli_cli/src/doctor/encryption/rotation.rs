//! ENC-ROTATION-001 — `encryption.key @key.<scope>` declares no
//! explicit `rotation` strategy for a scope where rotation matters
//! operationally (`@key.tenant`, `@key.user`, `@key.record`).
//! `@key.app` (global key) is exempt because rotating it is a
//! single-axis re-encrypt; the per-tenant / per-user cases need a
//! documented procedure.
//!
//! Severity: `warning` (strict + production).
//!
//! NOTE: v0 only legal `rotation` value is `manual`. The diagnostic
//! fires when the catalog default is applied silently — authors are
//! prompted to declare intent ("yes, manual is fine; my runbook
//! describes the procedure") rather than rely on implicit defaults.
//! The parser does not currently distinguish "absent" from "present
//! with value `manual`"; this rule is a placeholder hook for when
//! the parser adds that distinction (`Option<EncryptionRotation>`).
//! Today it produces no findings; the test exercises the closed-
//! catalog membership check and serves as scaffolding for the
//! follow-up.
//!
//! Reference: docs/proposals/encryption-vocab.md §Doctor diagnostics.

use std::path::{Path, PathBuf};

use lazuli_ir::{AppManifest, EncryptionKeyScope};

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub scope: String,
}

impl Finding {
    pub const CODE: &'static str = "ENC-ROTATION-001";

    pub fn message(&self) -> String {
        format!(
            "`encryption.key {}` declares no `rotation` strategy; defaulting to `manual` — document the rotation procedure in the capsule's runbook",
            self.scope
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Test whether a `@key.<scope>` reference is one of the cases where
/// rotation procedure documentation matters operationally. Public so
/// other tooling can adopt the same filter.
pub fn rotation_matters(scope_ref: &str) -> bool {
    match EncryptionKeyScope::parse(scope_ref) {
        Some(EncryptionKeyScope::Tenant)
        | Some(EncryptionKeyScope::User)
        | Some(EncryptionKeyScope::Record) => true,
        Some(EncryptionKeyScope::App) | None => false,
    }
}

/// Run ENC-ROTATION-001 on every binding. Until the parser
/// distinguishes "rotation absent" from "rotation manual", this
/// returns no findings; the function shape is committed so the
/// adapter pass and doctor-dispatch wire-up can plug in cleanly.
pub fn check(_app: &AppManifest, _path: &Path) -> Vec<Finding> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_matters_for_tenant_user_record() {
        assert!(rotation_matters("@key.tenant"));
        assert!(rotation_matters("@key.user"));
        assert!(rotation_matters("@key.record"));
    }

    #[test]
    fn rotation_does_not_matter_for_app_or_unknown() {
        assert!(!rotation_matters("@key.app"));
        assert!(!rotation_matters("@key.bogus"));
    }

    #[test]
    fn check_returns_no_findings_today() {
        // Placeholder check until the parser separates absent-vs-manual.
        let app = crate::doctor::encryption::test_support::empty_app();
        assert!(check(&app, Path::new("app.lzi")).is_empty());
    }
}
