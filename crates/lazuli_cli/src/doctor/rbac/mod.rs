//! Doctor rules for the RBAC catalog vocab (`permission` / `role`).
//! One module per diagnostic code, mirroring the `report/` / `poller/`
//! layout. See `docs/proposals/rbac-catalog-vocab.md` §Doctor.
//!
//! Codes (5 required + 1 advisory in v0.1):
//! - RBAC-PERM-UNDECLARED-001   — `has_permission X:Y` references unknown perm
//! - RBAC-ROLE-UNDECLARED-001   — `has_role X` / `@role.X` references unknown role
//! - RBAC-CYCLE-001             — role inheritance cycle
//! - RBAC-PERM-UNUSED-001       — declared permission never granted (warning)
//! - RBAC-MISSING-POLICY-001    — feature mixes policy / no-policy commands
//! - RBAC-CATALOG-MISSING-001   — `@role.*` used without catalog (info)
//!
//! Analyzer-surfaced issues (`RbacIssue`) are re-emitted via this
//! module so the diagnostic surface is single-pass and the doctor
//! command renders a uniform code list.

use std::path::PathBuf;

pub use lazuli_analyzer::rbac::RbacIssue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub message: String,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

/// Convert an analyzer-emitted `RbacIssue` into a doctor `Finding`.
/// `path`/`line`/`column` come from the caller; the analyzer tracks
/// byte spans only and doctor resolves to file coordinates upstream.
pub fn to_finding(issue: &RbacIssue, path: &std::path::Path, line: usize) -> Finding {
    Finding {
        code: issue.code.to_string(),
        message: issue.message.clone(),
        path: path.to_path_buf(),
        line,
        column: 1,
    }
}
