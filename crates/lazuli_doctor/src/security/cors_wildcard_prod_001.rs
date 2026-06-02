//! cors_wildcard_prod_001 — a wildcard (`"*"`) CORS allow-origin declared
//! for a production-targeted environment.
//!
//! ## Why this is a compile-time companion to the runtime guard
//!
//! The Go runtime already refuses to boot when `LAZULI_ENV` resolves to a
//! production environment AND the active CORS allowlist contains `"*"`:
//! `lazuli.NewCSRFGuard` returns `ErrCSRFWildcardProd`
//! (`runtime/go/lazuli/http_csrf.go`, guard code `CORS-WILDCARD-PROD-001`)
//! and `Mux()` panics rather than serve a credentialed-wildcard footgun. A
//! wildcard origin combined with credentialed (cookie) requests is invalid
//! per the CORS spec and disables origin isolation.
//!
//! That guard is excellent defense-in-depth, but it only fires at
//! boot/deploy — the footgun is shipped before anyone sees it. This rule
//! lifts the exact same contract to `lazuli check`/`doctor` so the wildcard
//! is caught at compile time, against the *declared* `cors allow_origins`
//! block, before deploy.
//!
//! ## Contract parity with the runtime guard
//!
//! The runtime keys off the environment *name*: `devSessionEnvAllowed`
//! (`runtime/go/lazuli/session.go`) treats only `dev` and `local` as dev
//! environments; every other name (`production`, `staging`, `prod`, …) is
//! production and the wildcard is a fatal error there. This rule mirrors
//! that exactly:
//!
//!   - `allow_origins <prod-env> "*"` (env NOT in `{dev, local}`)
//!     → **error** (`CORS-WILDCARD-PROD-001`): this WILL refuse to boot in
//!     that environment.
//!   - `allow_origins dev "*"` / `allow_origins local "*"`
//!     → **warning** (same code): the runtime allows `"*"` in dev but
//!     `slog.Warn`s; we mirror the warning with explicit "this will refuse
//!     to boot in production" messaging.
//!
//! Severity choice: the registry base severity is **error** — the defining
//! case is the production wildcard, which the runtime refuses outright. The
//! dev/local emission is a per-finding downgrade to warning, matching the
//! runtime's dev `slog.Warn`. (`from_code_prefix("CORS-…")` routes to
//! `Security`; CORS is an HTTP-edge security concern.)
//!
//! ## Graceful no-op
//!
//! Apps with no `cors` contract (e.g. pauta declares none) produce no
//! findings — the rule only inspects an authored `AppCors`.

use std::path::{Path, PathBuf};

use lazuli_ir::AppCors;

use crate::severity::DoctorSeverity;

/// Environment names the runtime treats as development (mirrors
/// `devSessionEnvAllowed` in `runtime/go/lazuli/session.go`). A wildcard
/// origin under any other environment name is a production-fatal config.
const DEV_ENVIRONMENTS: &[&str] = &["dev", "local"];

/// Returns `true` when `env` is a runtime-recognised dev environment.
/// Comparison is case-insensitive + trimmed to mirror the runtime's
/// `normalizeEnv` (`strings.ToLower(strings.TrimSpace(...))`).
fn is_dev_environment(env: &str) -> bool {
    let normalized = env.trim().to_ascii_lowercase();
    DEV_ENVIRONMENTS.contains(&normalized.as_str())
}

// ── output ──────────────────────────────────────────────────────────────────

/// One `cors allow_origins <env> "*"` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `cors` block lives in.
    pub path: PathBuf,
    /// The environment the wildcard was declared for.
    pub environment: String,
    /// `true` when `environment` is a production-targeted name (not
    /// `dev`/`local`) — drives the error vs warning severity split.
    pub is_production: bool,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user. Identical to the
    /// runtime guard code so a reader who hits the boot panic can map it
    /// straight back to the doctor rule (and vice-versa).
    pub const CODE: &'static str = "CORS-WILDCARD-PROD-001";

    /// Severity for this finding: `Error` for a production environment
    /// (the runtime refuses to boot), `Warning` for `dev`/`local` (the
    /// runtime warns but allows).
    pub fn severity(&self) -> DoctorSeverity {
        if self.is_production {
            DoctorSeverity::Error
        } else {
            DoctorSeverity::Warning
        }
    }

    /// Render the remediation message.
    ///
    /// ## Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use lazuli_doctor::security::cors_wildcard_prod_001::Finding;
    ///
    /// let prod = Finding {
    ///     path: PathBuf::from("app.lzi"),
    ///     environment: "production".into(),
    ///     is_production: true,
    /// };
    /// assert!(prod.message().contains("REFUSES to boot"));
    /// assert!(prod.message().contains("production"));
    /// ```
    pub fn message(&self) -> String {
        if self.is_production {
            format!(
                "`cors allow_origins {env} \"*\"` declares a wildcard origin for a production-targeted environment. \
A wildcard origin with credentialed (cookie) requests is invalid per the CORS spec and disables origin isolation, \
so the runtime REFUSES to boot when `LAZULI_ENV={env}` (guard CORS-WILDCARD-PROD-001). \
Replace `\"*\"` with an explicit origin allowlist for the `{env}` environment.",
                env = self.environment,
            )
        } else {
            format!(
                "`cors allow_origins {env} \"*\"` declares a wildcard origin. The runtime allows `\"*\"` in `{env}` \
(it warns and does NOT register `\"*\"` as a trusted cross-origin), but the SAME declaration in a \
production-targeted environment will REFUSE to boot (guard CORS-WILDCARD-PROD-001). \
Prefer an explicit origin allowlist so prod and dev stay consistent.",
                env = self.environment,
            )
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run cors_wildcard_prod_001 against an app's optional `AppCors`.
///
/// Returns one finding per `allow_origins <env> ...` rule that contains a
/// bare `"*"` origin. Production-targeted environments (env name not in
/// `{dev, local}`) yield an `is_production: true` finding (error); `dev` /
/// `local` yield a warning. Apps with no `cors` contract — `cors` is
/// `None` — yield no findings.
///
/// Only the bare `"*"` token triggers; a subdomain wildcard like
/// `https://*.example.com` is intentionally broader than a single URL but
/// is NOT the credentialed-wildcard footgun the runtime refuses, so it is
/// left to the existing `cors_origin_undocumented` cross-check.
///
/// ## Examples
///
/// ```
/// use std::path::Path;
/// use lazuli_ir::{AppCors, AppCorsOriginRule};
/// use lazuli_doctor::security::cors_wildcard_prod_001::check;
///
/// let cors = AppCors {
///     allow_origins: vec![AppCorsOriginRule {
///         environment: "production".into(),
///         origins: vec!["*".into()],
///     }],
///     ..Default::default()
/// };
/// let findings = check(Some(&cors), Path::new("app.lzi"));
/// assert_eq!(findings.len(), 1);
/// assert!(findings[0].is_production);
/// ```
pub fn check(cors: Option<&AppCors>, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(cors) = cors else {
        return findings;
    };
    for rule in &cors.allow_origins {
        if rule.origins.iter().any(|o| o == "*") {
            findings.push(Finding {
                path: path.to_path_buf(),
                environment: rule.environment.clone(),
                is_production: !is_dev_environment(&rule.environment),
            });
        }
    }
    findings
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::AppCorsOriginRule;

    use super::*;

    fn cors_with(rules: Vec<AppCorsOriginRule>) -> AppCors {
        AppCors {
            allow_origins: rules,
            ..Default::default()
        }
    }

    #[test]
    fn fires_error_on_wildcard_in_production() {
        let cors = cors_with(vec![AppCorsOriginRule {
            environment: "production".into(),
            origins: vec!["*".into()],
        }]);
        let findings = check(Some(&cors), Path::new("app.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].is_production);
        assert_eq!(findings[0].severity(), DoctorSeverity::Error);
        assert_eq!(Finding::CODE, "CORS-WILDCARD-PROD-001");
        assert!(findings[0].message().contains("REFUSES to boot"));
    }

    #[test]
    fn fires_error_on_wildcard_in_arbitrary_nondev_env() {
        // `staging` is not in {dev, local}, so the runtime treats it as
        // production — the wildcard is fatal there too.
        let cors = cors_with(vec![AppCorsOriginRule {
            environment: "staging".into(),
            origins: vec!["*".into()],
        }]);
        let findings = check(Some(&cors), Path::new("app.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].is_production);
        assert_eq!(findings[0].severity(), DoctorSeverity::Error);
    }

    #[test]
    fn warns_on_wildcard_in_dev_and_local() {
        for env in ["dev", "local", "  Local  ", "DEV"] {
            let cors = cors_with(vec![AppCorsOriginRule {
                environment: env.into(),
                origins: vec!["*".into()],
            }]);
            let findings = check(Some(&cors), Path::new("app.lzi"));
            assert_eq!(findings.len(), 1, "env {env:?} should fire once");
            assert!(!findings[0].is_production, "env {env:?} should be dev");
            assert_eq!(findings[0].severity(), DoctorSeverity::Warning);
            assert!(findings[0].message().contains("REFUSE to boot"));
        }
    }

    #[test]
    fn silent_on_explicit_origins_no_false_positive() {
        let cors = cors_with(vec![
            AppCorsOriginRule {
                environment: "production".into(),
                origins: vec!["https://app.example.com".into()],
            },
            AppCorsOriginRule {
                environment: "local".into(),
                origins: vec!["http://localhost:5173".into()],
            },
        ]);
        assert!(check(Some(&cors), Path::new("app.lzi")).is_empty());
    }

    #[test]
    fn silent_on_subdomain_wildcard_only() {
        // `https://*.example.com` is a subdomain wildcard, not the bare
        // credentialed-wildcard footgun; this rule leaves it alone.
        let cors = cors_with(vec![AppCorsOriginRule {
            environment: "production".into(),
            origins: vec!["https://*.example.com".into()],
        }]);
        assert!(check(Some(&cors), Path::new("app.lzi")).is_empty());
    }

    #[test]
    fn silent_when_no_cors_contract() {
        // pauta declares no `app.cors` — graceful no-op.
        assert!(check(None, Path::new("app.lzi")).is_empty());
    }

    #[test]
    fn one_finding_per_environment_rule() {
        let cors = cors_with(vec![
            AppCorsOriginRule {
                environment: "production".into(),
                origins: vec!["*".into()],
            },
            AppCorsOriginRule {
                environment: "local".into(),
                origins: vec!["*".into()],
            },
        ]);
        let findings = check(Some(&cors), Path::new("app.lzi"));
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.is_production));
        assert!(findings.iter().any(|f| !f.is_production));
    }
}
