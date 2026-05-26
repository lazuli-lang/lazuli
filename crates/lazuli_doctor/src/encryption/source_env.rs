//! ENC-SOURCE-ENV-001 — `encryption.key @key.<scope>` references
//! `env.<NAME>` but the env-var schema (in `registry.lzi` or `app.lzi`)
//! has no matching entry.
//!
//! Severity: `error` (strict + production).
//! Reference: docs/proposals/encryption-vocab.md §Doctor diagnostics.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{AppManifest, AppRegistry, EncryptionSource};

// ── output ────────────────────────────────────────────────────────────────────

/// One ENC-SOURCE-ENV-001 finding — an encryption key binding pulls
/// its material from `env.<NAME>` but the env-var schema (in
/// `registry.lzi` or `app.lzi`) declares no matching entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the binding was authored in.
    pub path: PathBuf,
    /// `@key.<scope>` reference, verbatim.
    pub scope: String,
    /// Resolved env-var name (template axes preserved verbatim).
    pub env_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ENC-SOURCE-ENV-001";

    /// Render the "env entry missing for encryption key" message.
    /// Includes the literal env var name (with axes like
    /// `{tenant_id}` preserved verbatim) so the author can paste it
    /// into `registry.lzi`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::encryption::source_env::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("app.lzi"),
    ///     scope: "@key.tenant".into(),
    ///     env_name: "CRYPT_KEY_TENANT_{tenant_id}".into(),
    /// };
    /// assert!(f.message().contains("CRYPT_KEY_TENANT_"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`encryption.key {}` references env var `{}` but no `env <NAME>` schema entry exists in `registry.lzi` (or `app.lzi`)",
            self.scope, self.env_name
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Cross-checks every `EncryptionSource::Env` binding source against
/// the env-var names declared in the app + registry env schemas.
///
/// Templates with `{tenant_id}` etc. axes are matched **on the
/// brace-bearing literal** verbatim: the registry pattern
/// `CRYPT_KEY_TENANT_{tenant_id}` must appear in the env schema to
/// satisfy this check.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::encryption::source_env::check;
/// use lazuli_ir::{AppManifest, AppRegistry};
///
/// let app: AppManifest = unimplemented!("load app manifest");
/// let _ = check(&app, None::<&AppRegistry>, Path::new("app.lzi"));
/// ```
pub fn check(
    app: &AppManifest,
    registry: Option<&AppRegistry>,
    path: &Path,
) -> Vec<Finding> {
    let mut declared: HashSet<&str> = HashSet::new();
    for env in &app.env {
        declared.insert(env.name.as_str());
    }
    if let Some(reg) = registry {
        for env in &reg.env {
            declared.insert(env.name.as_str());
        }
    }

    let mut findings = vec![];

    for binding in &app.encryption_bindings {
        let EncryptionSource::Env(template) = &binding.source else {
            continue;
        };
        if !declared.contains(template.literal.as_str()) {
            findings.push(Finding {
                path: path.to_path_buf(),
                scope: binding.scope.clone(),
                env_name: template.literal.clone(),
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::test_support::*;
    use lazuli_ir::AppEnvVar;

    fn env_entry(name: &str) -> AppEnvVar {
        AppEnvVar {
            group: None,
            scope: "server".into(),
            name: name.into(),
            type_name: "Secret".into(),
            requiredness: "required".into(),
            environments: vec![],
        }
    }

    #[test]
    fn positive_missing_env_entry_fires() {
        let mut app = empty_app();
        app.encryption_bindings
            .push(make_binding("@key.tenant", "CRYPT_KEY_TENANT_{tenant_id}"));
        let findings = check(&app, None, Path::new("app.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].env_name, "CRYPT_KEY_TENANT_{tenant_id}");
    }

    #[test]
    fn negative_env_entry_declared_passes() {
        let mut app = empty_app();
        app.encryption_bindings
            .push(make_binding("@key.tenant", "CRYPT_KEY_TENANT_{tenant_id}"));
        app.env.push(env_entry("CRYPT_KEY_TENANT_{tenant_id}"));
        assert!(check(&app, None, Path::new("app.lzi")).is_empty());
    }

    #[test]
    fn secrets_source_is_not_env_checked() {
        let mut app = empty_app();
        let mut binding = make_binding("@key.tenant", "ignored");
        // Forge a secrets-source binding directly.
        binding.source = lazuli_ir::EncryptionSource::Secrets(
            lazuli_ir::EncryptionTemplate::parse("vault_key"),
        );
        app.encryption_bindings.push(binding);
        // Even with no env entries, secrets-source bindings don't fire
        // this rule.
        assert!(check(&app, None, Path::new("app.lzi")).is_empty());
    }
}
