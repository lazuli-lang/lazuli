//! HTTP headers + secret-rotation aggregator (roadmap §1.10).
//!
//! Three closed-catalog rules on the security surface lifted from
//! user-territory into the language:
//!
//! - `headers-contract` — under `production` the app must declare
//!   `csp`, `hsts`, `x_frame_options`, `x_content_type_options`. Closed
//!   catalogs apply to `x_content_type_options` (`nosniff`),
//!   `x_frame_options` (`DENY` / `SAMEORIGIN` / `ALLOW-FROM <uri>`),
//!   `referrer_policy`, and `hsts.max_age > 0`. Outside production the
//!   missing-slot warning fires only when the author opted in by
//!   declaring any `app.headers` content.
//! - `secret-rotation-overlap-contract` — overlap > cadence is a
//!   contradictory profile (the runtime cannot finish a rollover
//!   before it has begun).
//! - `secret-rotation-binding-unknown` — `app.encryption.key
//!   @key.<scope> rotation_profile <name>` must reference a profile
//!   declared on `registry.secret_rotations`.
//!
//! Runtime wire-of-X (actual header emission, secret rollover
//! scheduling) stays for the follow-up cycle — these diagnostics
//! validate the declarative shape today.

use std::collections::BTreeSet;
use std::path::PathBuf;

use lazuli_doctor_config::DoctorProfile as SecurityProfile;
use lazuli_ir::{self as ir};

use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity};

/// Required headers under the `production` security profile. Other
/// profiles emit a warning instead of an error; the catalog itself
/// stays the same so the message reads consistently across profiles.
const HEADERS_REQUIRED_IN_PRODUCTION: &[&str] =
    &["csp", "hsts", "x_frame_options", "x_content_type_options"];

pub(crate) fn headers_diagnostics(
    app: Option<&DoctorAppManifest>,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };

    let headers = app_manifest.manifest.headers.as_ref();
    let manifest_path = app_manifest.path.clone();

    // Production-profile completeness gate. Two distinct behaviours
    // keyed off whether the author has opted in by declaring ANY
    // `app.headers` content:
    //
    // 1. Author opted in (headers block present): every profile flags
    //    missing required slots (Strict/Prototype as warning, Production
    //    as error). The intent signal is unambiguous.
    //
    // 2. No headers block at all: only Production fires. Strict and
    //    Prototype defer — existing fixtures + feature-port flows must
    //    keep passing on the default Strict profile.
    let severity = match security_profile {
        SecurityProfile::Production | SecurityProfile::IronHand => DoctorSeverity::Error,
        SecurityProfile::Strict | SecurityProfile::Prototype => DoctorSeverity::Warning,
    };
    let author_opted_in = headers.is_some();
    let production_gate = security_profile == SecurityProfile::Production;
    let mut missing: Vec<&'static str> = Vec::new();
    for required in HEADERS_REQUIRED_IN_PRODUCTION {
        let present = headers
            .map(|h| match *required {
                "csp" => h.csp.is_some(),
                "hsts" => h.hsts.is_some(),
                "x_frame_options" => h.x_frame_options.is_some(),
                "x_content_type_options" => h.x_content_type_options.is_some(),
                _ => true,
            })
            .unwrap_or(false);
        if !present {
            missing.push(*required);
        }
    }
    if !missing.is_empty() && (author_opted_in || production_gate) {
        diagnostics.push(DoctorDiagnostic {
            path: manifest_path.clone(),
            line: 1,
            column: 1,
            severity,
            code: "headers-contract".to_owned(),
            message: format!(
                "`app.headers` is missing the production-grade slots [{}]. Declare them under `app.lzi headers` so the runtime can emit the headers on every response.",
                missing.join(", "),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // Closed-catalog value checks. These are independent of the
    // profile — `nosniff` is the only legal X-Content-Type-Options
    // token anywhere, etc.
    if let Some(headers) = headers {
        if let Some(value) = headers.x_content_type_options.as_deref()
            && !ir::AppHeaders::is_x_content_type_options_known(value)
        {
            diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "headers-contract".to_owned(),
                    message: format!(
                        "`app.headers x_content_type_options {value}` is invalid — the only legal token is `nosniff`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
        }
        if let Some(value) = headers.x_frame_options.as_deref()
            && !ir::AppHeaders::is_x_frame_options_known(value)
        {
            diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "headers-contract".to_owned(),
                    message: format!(
                        "`app.headers x_frame_options {value}` is invalid — closed catalog is `DENY`, `SAMEORIGIN`, or `ALLOW-FROM <uri>`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
        }
        if let Some(value) = headers.referrer_policy.as_deref()
            && !ir::AppHeaders::is_referrer_policy_known(value)
        {
            diagnostics.push(DoctorDiagnostic {
                path: manifest_path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "headers-contract".to_owned(),
                message: format!(
                    "`app.headers referrer_policy {value}` is invalid — closed catalog is [{}].",
                    ir::AppHeaders::REFERRER_POLICY_CATALOG.join(", "),
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        if let Some(hsts) = headers.hsts.as_ref()
            && hsts.max_age == 0
        {
            diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity,
                    code: "headers-contract".to_owned(),
                    message: "`app.headers hsts max_age 0` disables HSTS — set a positive seconds value (typically 31536000 or higher) so the runtime can opt the browser into HTTPS-only.".to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
        }
    }

    diagnostics
}

pub(crate) fn secret_rotation_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // overlap > cadence — invalid profile. The path is the registry
    // file because the profile is authored there.
    if let Some(registry) = registry {
        for rotation in &registry.secret_rotations {
            let cadence_secs = ir::security_duration::duration_seconds(&rotation.cadence);
            let overlap_secs = ir::security_duration::duration_seconds(&rotation.overlap);
            if let (Some(cadence), Some(overlap)) = (cadence_secs, overlap_secs)
                && overlap > cadence
            {
                // We don't have a registry path on AppRegistry; use
                // the app path as a fallback because both files
                // live next to each other and the message names the
                // profile explicitly. If the app manifest is also
                // missing, fall through to a synthesized
                // `registry.lzi` path.
                let path = app
                    .map(|a| a.path.clone())
                    .unwrap_or_else(|| PathBuf::from("registry.lzi"));
                diagnostics.push(DoctorDiagnostic {
                        path,
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "secret-rotation-overlap-contract".to_owned(),
                        message: format!(
                            "`secret_rotation {name}` declares overlap `{overlap_lit}` longer than cadence `{cadence_lit}`. Overlap is the grace window during which old + new secrets both pass; it must be strictly shorter than the cadence between rolls.",
                            name = rotation.name,
                            overlap_lit = rotation.overlap,
                            cadence_lit = rotation.cadence,
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

    // `app.encryption.key @key.<scope> rotation_profile <name>`
    // referencing an undeclared profile.
    if let Some(app_manifest) = app {
        let declared: BTreeSet<&str> = registry
            .map(|r| r.secret_rotations.iter().map(|s| s.name.as_str()).collect())
            .unwrap_or_default();
        for binding in &app_manifest.manifest.encryption_bindings {
            let Some(profile) = binding.rotation_profile.as_deref() else {
                continue;
            };
            if !declared.contains(profile) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "secret-rotation-binding-unknown".to_owned(),
                    message: format!(
                        "`encryption.key {scope} rotation_profile {profile}` references no `secret_rotation {profile}` entry in `registry.lzi`. Declare the profile or remove the reference.",
                        scope = binding.scope,
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

    diagnostics
}
