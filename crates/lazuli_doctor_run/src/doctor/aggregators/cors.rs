//! CORS contract aggregator (Cut A.11).
//!
//! Cross-checks the `cors` block in `app.lzi` against the declared
//! `environments` and `urls` so:
//!
//! - `cors allow_origins <env> ...` references an environment in
//!   `app.environments` (`cors_unknown_environment_diagnostics`, error).
//! - Each non-wildcard origin matches a `url` declared for the same
//!   environment (`cors_origin_undocumented_diagnostics`, warning).
//! - `allow_origins ... "*"` is not combined with `allow_credentials true`
//!   (`cors_credentials_wildcard_conflict_diagnostics`, error — CORS spec).
//! - `allow_origins <prod-env> "*"` (a wildcard origin in a production-
//!   targeted environment) is flagged by `CORS-WILDCARD-PROD-001` — the
//!   compile-time companion to the runtime `Mux()` boot refusal. Dispatched
//!   from the pure rule in `lazuli_doctor::security::cors_wildcard_prod_001`.
//!
//! Extracted from `doctor/mod.rs` so the CORS surface has a named
//! aggregator file alongside other Cut-A.11+ surfaces.

use std::collections::BTreeSet;

use crate::doctor::parsers::{environments_summary, same_origin};
use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity};

pub(crate) fn diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(cors) = app_manifest.manifest.cors.as_ref() else {
        return diagnostics;
    };

    let environments: BTreeSet<&str> = app_manifest
        .manifest
        .environments
        .iter()
        .map(String::as_str)
        .collect();
    let declared_urls: Vec<&lazuli_ir::AppUrl> = app_manifest.manifest.urls.iter().collect();

    let mut has_wildcard = false;
    for rule in &cors.allow_origins {
        // Environment must be declared in `app.environments`.
        if !environments.contains(rule.environment.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "cors_unknown_environment_diagnostics".to_owned(),
                message: format!(
                    "`cors allow_origins {} ...` references an environment that is not in `app.environments` ({}).",
                    rule.environment,
                    environments_summary(&environments),
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for origin in &rule.origins {
            if origin == "*" {
                has_wildcard = true;
                continue;
            }
            // Wildcards in subdomain (`https://*.example.com`) skip
            // url-match — they're intentionally broader than any
            // single URL declaration.
            if origin.contains("*") {
                continue;
            }
            // Compare against declared urls in the same environment.
            // The match is prefix-based: a declared URL
            // `https://app.example.com` allows the origin
            // `https://app.example.com` (exact) — query string and
            // path differences are tolerated by the CORS layer, so
            // we compare scheme+host.
            let documented = declared_urls
                .iter()
                .filter(|u| u.environment == rule.environment)
                .any(|u| same_origin(&u.url, origin));
            if !documented {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "cors_origin_undocumented_diagnostics".to_owned(),
                    message: format!(
                        "`cors allow_origins {env} \"{origin}\"` does not match any `url <target> {env} ...` declaration. If the origin is a third-party caller, ignore; otherwise update `urls` so the source-of-truth stays consistent.",
                        env = rule.environment,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    // CORS-WILDCARD-PROD-001 — a wildcard origin in a production-targeted
    // environment. Compile-time companion to the runtime `Mux()` boot
    // refusal (`ErrCSRFWildcardProd`). The pure rule lives in
    // `lazuli_doctor::security::cors_wildcard_prod_001`; it returns one
    // finding per `allow_origins <env> "*"` rule with the env-keyed
    // error/warning split that mirrors the runtime's `devSessionEnvAllowed`.
    for finding in
        lazuli_doctor::security::cors_wildcard_prod_001::check(Some(cors), &app_manifest.path)
    {
        diagnostics.push(DoctorDiagnostic {
            path: finding.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::from(finding.severity()),
            code: lazuli_doctor::security::cors_wildcard_prod_001::Finding::CODE.to_owned(),
            message: finding.message(),
            category: Some(lazuli_doctor::RuleCategory::Security),
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // CORS spec forbids `allow_origins "*"` with `allow_credentials true`.
    if has_wildcard && cors.allow_credentials {
        diagnostics.push(DoctorDiagnostic {
            path: app_manifest.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "cors_credentials_wildcard_conflict_diagnostics".to_owned(),
            message: "`cors allow_origins ... \"*\"` cannot be combined with `allow_credentials true`. Per CORS spec, browsers reject the response. Either narrow the origin list or set `allow_credentials false`.".to_owned(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    diagnostics
}
