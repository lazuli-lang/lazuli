//! ENC-TEMPLATE-AXIS-001 — `encryption.key @key.<scope>` source template
//! is missing the required axis brace, or carries an axis the scope
//! forbids (e.g. `@key.app` with `{tenant_id}`).
//!
//! Severity: `error` (strict + production).
//! Reference: docs/proposals/encryption-vocab.md §Doctor diagnostics.

use std::path::{Path, PathBuf};

use lazuli_ir::{AppManifest, EncryptionKeyScope};

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    /// `@key.<scope>` reference, verbatim.
    pub scope: String,
    /// Source template literal, e.g. `"CRYPT_KEY_TENANT"` (missing
    /// `{tenant_id}`).
    pub template_literal: String,
    /// What's wrong: the axis the scope requires is missing, or a
    /// forbidden axis was found.
    pub reason: AxisMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxisMismatch {
    /// `@key.<scope>` requires the named axis but the template
    /// literal omits it.
    Missing(&'static str),
    /// `@key.app` forbids any axis (it's a global key) but the
    /// template carries one anyway.
    Forbidden,
}

impl Finding {
    pub const CODE: &'static str = "ENC-TEMPLATE-AXIS-001";

    pub fn message(&self) -> String {
        match &self.reason {
            AxisMismatch::Missing(axis) => format!(
                "`encryption.key {}` source template `{}` is missing required axis `{{{}}}`",
                self.scope, self.template_literal, axis
            ),
            AxisMismatch::Forbidden => format!(
                "`encryption.key {}` source template `{}` carries a template axis but `@key.app` is a global key — remove the brace expression",
                self.scope, self.template_literal
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

pub fn check(app: &AppManifest, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for binding in &app.encryption_bindings {
        let template = binding.source.template();
        let Some(scope) = EncryptionKeyScope::parse(&binding.scope) else {
            // ENC-KEY-SCOPE-UNKNOWN is a separate diagnostic (not in v0
            // catalog). Skip here so we don't double-report.
            continue;
        };
        match scope.required_axis() {
            None => {
                // `@key.app` — forbid axes.
                if !template.axes.is_empty() {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        scope: binding.scope.clone(),
                        template_literal: template.literal.clone(),
                        reason: AxisMismatch::Forbidden,
                    });
                }
            }
            Some(required) => {
                if !template.axes.contains(&required) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        scope: binding.scope.clone(),
                        template_literal: template.literal.clone(),
                        reason: AxisMismatch::Missing(required.as_str()),
                    });
                }
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::encryption::test_support::*;

    fn app_with_binding(scope: &str, template_literal: &str) -> AppManifest {
        let mut app = empty_app();
        app.encryption_bindings
            .push(make_binding(scope, template_literal));
        app
    }

    #[test]
    fn positive_tenant_scope_missing_axis_fires() {
        // `@key.tenant` requires `{tenant_id}` but the template is bare.
        let app = app_with_binding("@key.tenant", "CRYPT_KEY_TENANT");
        let findings = check(&app, Path::new("app.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope, "@key.tenant");
        assert!(matches!(findings[0].reason, AxisMismatch::Missing("tenant_id")));
        assert!(findings[0].message().contains("{tenant_id}"));
    }

    #[test]
    fn negative_tenant_with_axis_passes() {
        let app = app_with_binding("@key.tenant", "CRYPT_KEY_TENANT_{tenant_id}");
        assert!(check(&app, Path::new("app.lzi")).is_empty());
    }

    #[test]
    fn positive_app_scope_with_axis_fires() {
        let app = app_with_binding("@key.app", "CRYPT_KEY_APP_{tenant_id}");
        let findings = check(&app, Path::new("app.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].reason, AxisMismatch::Forbidden));
    }

    #[test]
    fn negative_app_scope_no_axis_passes() {
        let app = app_with_binding("@key.app", "CRYPT_KEY_APP");
        assert!(check(&app, Path::new("app.lzi")).is_empty());
    }
}
