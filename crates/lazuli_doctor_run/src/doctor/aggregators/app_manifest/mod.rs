//! App-manifest aggregator — emits the family of `app.lzi` /
//! `workspace.lzi` cross-contract diagnostics:
//!
//!   * app_contract_diagnostics — top-level operational checks
//!   * app_route_redirect_diagnostics — redirect target validation
//!   * error_page_contract_diagnostics — closed-status + handler shape
//!   * workspace_contract_diagnostics — multi-app workspace consistency
//!
//! Plus shared helpers (`app_missing_contract_diagnostic`,
//! `error_page_line`, `event_pattern_covers`).
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4 and
//! re-split in R9 into the sibling files below.

use std::collections::BTreeSet;

use crate::doctor::{
    DoctorAppManifest, DoctorAppProfile, DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity,
    OperationalFacts, adapter_provenance_diagnostics, aggregators,
    app_binding_contract_diagnostics, app_has_any_capability, app_has_target, app_has_url,
    app_pack_contract_diagnostics, app_runtime_runs, app_runtime_serves,
    app_service_contract_diagnostics, enabled_pack_provided_features, operational_env_names,
    profile_contract_diagnostics,
};

mod error_pages;
mod routes;
mod workspace;

pub(crate) use error_pages::{error_page_contract_diagnostics, error_page_line};
pub(crate) use routes::app_route_redirect_diagnostics;
pub(crate) use workspace::{event_pattern_covers, workspace_contract_diagnostics};

pub(crate) fn app_contract_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else {
        if !profiles.is_empty() {
            return profiles
                .iter()
                .map(|profile| DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-APP-001".to_owned(),
                    message: format!(
                        "profile `{}` is declared, but no package app manifest was found.",
                        profile.profile.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                })
                .collect();
        }
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let manifest = &app.manifest;
    let env_names = operational_env_names(manifest, registry);
    let used_features: BTreeSet<_> = manifest.uses.iter().map(String::as_str).collect();
    let pack_features = enabled_pack_provided_features(manifest, registry);

    for feature in operational.features.values() {
        if !used_features.contains(feature.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-USES-001".to_owned(),
                message: format!(
                    "app manifest does not list local feature `{}` in `uses`; generated app registration may omit it.",
                    feature.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for used in &manifest.uses {
        if !operational.features.contains_key(used) && !pack_features.contains(used.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-USES-002".to_owned(),
                message: format!(
                    "app manifest lists `{used}` in `uses`, but no local feature with that name was found in this package."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    if !manifest.services.is_empty() {
        diagnostics.extend(app_service_contract_diagnostics(
            app,
            operational,
            &pack_features,
        ));
    }

    diagnostics.extend(adapter_provenance_diagnostics(app, registry, profiles));
    diagnostics.extend(app_pack_contract_diagnostics(app, registry));
    diagnostics.extend(app_binding_contract_diagnostics(app, registry, operational));
    diagnostics.extend(aggregators::external::external_call_contract_diagnostics(
        operational,
    ));
    diagnostics.extend(app_route_redirect_diagnostics(app, operational));
    diagnostics.extend(error_page_contract_diagnostics(app));
    diagnostics.extend(profile_contract_diagnostics(
        app,
        registry,
        profiles,
        operational,
    ));

    for env_ref in &operational.env_references {
        if !env_names.contains(env_ref.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: env_ref.path.clone(),
                line: env_ref.line,
                column: env_ref.column,
                severity: DoctorSeverity::Error,
                code: "APP-ENV-001".to_owned(),
                message: format!(
                    "environment reference `env.{}` is not declared in `app.lzi` or `registry.lzi` env.",
                    env_ref.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    if !operational.file_capabilities.is_empty()
        && !app_has_any_capability(manifest, registry, &["object_storage", "storage"])
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-CAP-001",
            "package uses `@cap.File`, but app/registry contract does not declare `object_storage` or `storage` capability.",
        ));
    }

    if !operational.jobs.is_empty() && !app_runtime_runs(manifest, "jobs") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-001",
            "package declares jobs, but app manifest runtime does not declare a unit that `runs jobs *`.",
        ));
    }

    if !operational.schedules.is_empty() && !app_runtime_runs(manifest, "schedules") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-002",
            "package declares scheduled jobs, but app manifest runtime does not declare a unit that `runs schedules *`.",
        ));
    }

    if !operational.webhooks.is_empty() && !app_runtime_serves(manifest, "webhooks") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-003",
            "package declares webhooks, but app manifest runtime does not declare a unit that `serves webhooks`.",
        ));
    }

    if !operational.apis.is_empty() && !app_runtime_serves(manifest, "apis") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-004",
            "package declares custom APIs, but app manifest runtime does not declare a unit that `serves apis`.",
        ));
    }

    if (!operational.web_routes.is_empty() || !operational.web_surfaces.is_empty())
        && !app_has_target(manifest, "web")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-TARGET-001",
            "package declares web routes/surfaces, but app manifest targets do not include `web <runtime>`.",
        ));
    }

    if (!operational.mobile_routes.is_empty() || !operational.mobile_surfaces.is_empty())
        && !app_has_target(manifest, "mobile")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-TARGET-002",
            "package declares mobile routes/surfaces, but app manifest targets do not include `mobile <runtime>`.",
        ));
    }

    if !operational.web_routes.is_empty() && !app_has_url(manifest, profiles, "web") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-001",
            "package declares web routes, but app manifest URLs do not include a `web` URL.",
        ));
    }

    if (!operational.webhooks.is_empty() || !operational.apis.is_empty())
        && !app_has_url(manifest, profiles, "api")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-002",
            "package declares webhooks or custom APIs, but app manifest URLs do not include an `api` URL.",
        ));
    }

    diagnostics
}

pub(crate) fn app_missing_contract_diagnostic(
    app: &DoctorAppManifest,
    code: &str,
    message: &str,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: code.to_owned(),
        message: message.to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}
