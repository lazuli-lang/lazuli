//! Observability aggregator (bucket cycle rows 36 + 36b).
//!
//! Closed-catalog cross-checks on `app.logging`, `app.tracing`, and
//! `app.observability`. Six logging/tracing rules surface from
//! `logging_tracing_diagnostics`:
//!
//! - `app_logging_level_invalid_diagnostics`     — error
//! - `app_logging_format_invalid_diagnostics`    — error
//! - `app_logging_redact_unknown_diagnostics`    — error
//! - `app_logging_sample_rate_range_diagnostics` — error
//! - `app_tracing_sample_rate_range_diagnostics` — error
//! - `app_tracing_exporter_unbound_diagnostics`  — error
//!
//! Plus `OBSERVABILITY-SOURCE-001` (error_source closed catalog) and
//! `OBSERVABILITY-PANIC-001` (panic_recover outside `dev` is loud) in
//! `app_diagnostics`.
//!
//! Closed catalogs are deliberately small. New catalog entries require a
//! language cut. See `docs/proposals/bucket-observability-cycle.md`
//! §3.1 §3.2 for the rationale.

use std::path::PathBuf;

use lazuli_ir::{self as ir, AppManifest};

use crate::doctor::parsers::catalog_list;
use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity};

/// Closed catalog shared with `event.trace <name> level <level>` in
/// row 37. Mirrors `log/slog` level discipline.
const LOG_LEVEL_CATALOG: &[&str] = &["debug", "info", "warn", "error"];

/// Closed catalog for `app.logging.format`. JSON for production
/// pipelines, text for local development.
const LOG_FORMAT_CATALOG: &[&str] = &["json", "text"];

/// Closed catalog for `app.logging.redact`. `pii` consumes `@pii.*`
/// tags; `none` opts out entirely.
const LOG_REDACT_CATALOG: &[&str] = &["pii", "none"];

const OBSERVABILITY_ERROR_SOURCE_CATALOG: &[&str] = &["dev", "staging", "prod"];

pub(crate) fn logging_tracing_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let manifest_path = app_manifest.path.clone();

    if let Some(logging) = app_manifest.manifest.logging.as_ref() {
        if let Some(level) = logging.level.as_deref() {
            if !LOG_LEVEL_CATALOG.contains(&level) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_level_invalid_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.level {level}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_LEVEL_CATALOG),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
        if let Some(format) = logging.format.as_deref() {
            if !LOG_FORMAT_CATALOG.contains(&format) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_format_invalid_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.format {format}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_FORMAT_CATALOG),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
        if let Some(redact) = logging.redact.as_deref() {
            if !LOG_REDACT_CATALOG.contains(&redact) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_redact_unknown_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.redact {redact}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_REDACT_CATALOG),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
        if let Some(rate) = logging.sample_rate {
            if !(0.0..=1.0).contains(&rate) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_sample_rate_range_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.sample_rate {rate}` must be a float in `[0.0, 1.0]`. Use `1.0` for full capture and `0.0` to disable."
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

    if let Some(tracing) = app_manifest.manifest.tracing.as_ref() {
        if let Some(rate) = tracing.sample_rate {
            if !(0.0..=1.0).contains(&rate) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_tracing_sample_rate_range_diagnostics".to_owned(),
                    message: format!(
                        "`app.tracing.sample_rate {rate}` must be a float in `[0.0, 1.0]`. Use `1.0` for full capture and `0.0` to disable."
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
        if let Some(exporter) = tracing.exporter.as_deref() {
            // The exporter slot must resolve to a `registry.capabilities
            // <name> tracing` entry (declared as the `name`, valued as
            // `tracing`) or to an integration. We accept any
            // `AppCapability` whose value is `tracing` *or* whose name
            // matches the exporter literal.
            let resolved = registry
                .map(|reg| {
                    reg.capabilities
                        .iter()
                        .any(|cap| cap.name == exporter && cap.value == "tracing")
                })
                .unwrap_or(false);
            if !resolved {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_tracing_exporter_unbound_diagnostics".to_owned(),
                    message: format!(
                        "`app.tracing.exporter {exporter}` does not resolve to a `registry.capabilities` entry of kind `tracing`. Declare it in `registry.capabilities`, or remove the line to let the runtime pick a default.",
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

pub(crate) fn app_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    diagnostics.extend(
        check_observability_source_tokens(&app_manifest.manifest)
            .into_iter()
            .map(|mut diagnostic| {
                diagnostic.path = app_manifest.path.clone();
                diagnostic
            }),
    );
    diagnostics.extend(
        check_observability_panic_recover(&app_manifest.manifest)
            .into_iter()
            .map(|mut diagnostic| {
                diagnostic.path = app_manifest.path.clone();
                diagnostic
            }),
    );
    diagnostics
}

/// OBSERVABILITY-SOURCE-001 — error_source token outside closed catalog.
/// Allowed values: "dev", "staging", "prod".
fn check_observability_source_tokens(app: &AppManifest) -> Vec<DoctorDiagnostic> {
    let Some(observability) = app.observability.as_ref() else {
        return Vec::new();
    };
    observability
        .error_source
        .iter()
        .filter(|token| !OBSERVABILITY_ERROR_SOURCE_CATALOG.contains(&token.as_str()))
        .map(|token| DoctorDiagnostic {
            path: PathBuf::new(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "OBSERVABILITY-SOURCE-001".to_owned(),
            message: format!(
                "`app.observability.error_source {token}` is not in the closed catalog. Allowed values: {}.",
                catalog_list(OBSERVABILITY_ERROR_SOURCE_CATALOG),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

/// OBSERVABILITY-PANIC-001 — panic_recover=false outside `dev` environment.
/// Loud opt-out for prod; require explicit override.
fn check_observability_panic_recover(app: &AppManifest) -> Vec<DoctorDiagnostic> {
    let Some(observability) = app.observability.as_ref() else {
        return Vec::new();
    };
    if observability.panic_recover {
        return Vec::new();
    }
    let has_non_dev = app.environments.is_empty()
        || app
            .environments
            .iter()
            .any(|environment| environment != "dev");
    if !has_non_dev {
        return Vec::new();
    }
    vec![DoctorDiagnostic {
        path: PathBuf::new(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "OBSERVABILITY-PANIC-001".to_owned(),
        message: "`app.observability.panic_recover false` disables the runtime panic guard outside `dev`. Keep recovery enabled for staging/prod unless this is an explicit debug override.".to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}
