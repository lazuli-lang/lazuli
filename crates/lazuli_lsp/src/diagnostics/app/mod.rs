//! Diagnostics for the `app` operational contract family.
//!
//! The top-level `app <Name>` block declares the operational shape of a
//! single deployable Lazuli app: what it uses (features), targets (web /
//! mobile / api), environments, runtime units, deploy policy,
//! architecture, services, communication, integrations, packs, and
//! capabilities. This module owns the file-local shape check on that
//! surface.
//!
//! ## Producers
//!
//! * [`app_operational_contract_diagnostics`] — single-pass dispatcher
//!   that walks the source, accumulates [`AppOperationalFacts`] for the
//!   open app block, and flushes block-level checks via
//!   [`app_operational_block_diagnostics`].
//! * [`app_operational_block_diagnostics`] — pure facts-to-diagnostics
//!   for whole-app invariants (must have uses, must have at least one
//!   target, deploy.migrations + deploy.rollback presence, services own
//!   resources, runtime units have a serves/runs verb and a
//!   healthcheck/readiness probe, etc.).
//!
//! ## Sub-modules
//!
//! Concern-shaped helpers live next door:
//!
//! * [`child`] — block-header recognition + scalar children
//! * [`target`] — `targets` / `urls` / `bindings` / `packs`
//! * [`env`] — `env` lines + `PUBLIC`/`EXPO_PUBLIC_` exposure rules
//! * [`integration`] — `integrations` + adapter-source provenance
//! * [`capability`] — `capabilities` line catalog
//! * [`architecture`] — `architecture` + `communication`
//! * [`service`] — `services` block + per-service facts
//! * [`runtime`] — `runtime unit` block + per-unit facts
//! * [`deploy`] — `deploy` block (mutates the parent facts)
//!
//! Everything is `pub(crate)` and re-exported from this module so the
//! crate-root `pub(crate) use diagnostics::app::*;` in `lib.rs`
//! continues to satisfy `crate::*` resolution in
//! `diagnostics/registry.rs` and `diagnostics/profile.rs`.

mod architecture;
mod capability;
mod child;
mod deploy;
mod env;
mod integration;
mod runtime;
mod service;
mod target;

// ABI: every `pub(crate)` helper from the sub-modules is re-exported
// here so the crate-root `pub(crate) use diagnostics::app::*;` in
// `lib.rs` keeps satisfying `crate::*` resolution from
// `diagnostics/registry.rs`, `diagnostics/profile.rs`, and any future
// consumer. The `#[allow(unused_imports)]` covers helpers that are
// currently only consumed via the lib-root glob (not from inside
// `app/mod.rs` itself).
#[allow(unused_imports)]
pub(crate) use architecture::{validate_app_architecture_line, validate_app_communication_line};
#[allow(unused_imports)]
pub(crate) use capability::validate_app_capability_line;
#[allow(unused_imports)]
pub(crate) use child::{
    app_child_block, command_name_if, is_app_scalar_child, named_block_name,
    validate_app_child_header, validate_app_scalar_child,
};
#[allow(unused_imports)]
pub(crate) use deploy::validate_app_deploy_line;
#[allow(unused_imports)]
pub(crate) use env::{
    has_public_token, parse_env_group_name, valid_env_declaration_parts, validate_app_env_line,
};
#[allow(unused_imports)]
pub(crate) use integration::{
    adapter_source_provenance, parse_app_integration_header, valid_path_segment,
    valid_pathish_tail, valid_plugin_tail, validate_app_integration_child,
    validate_app_integration_credential_line, validate_app_integration_header,
};
#[allow(unused_imports)]
pub(crate) use runtime::{validate_app_runtime_unit_child, AppRuntimeUnitFacts};
#[allow(unused_imports)]
pub(crate) use service::{
    validate_app_service_child, validate_app_service_exposure_line, AppServiceFacts,
};
#[allow(unused_imports)]
pub(crate) use target::{
    parse_app_binding_line, validate_app_binding_line, validate_app_pack_use_line,
    validate_app_target_line, validate_app_url_line, validate_registry_pack_child,
    validate_registry_pack_header,
};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

#[derive(Debug)]
pub(crate) struct AppOperationalFacts {
    pub(crate) line_index: usize,
    pub(crate) line: String,
    pub(crate) has_uses: bool,
    pub(crate) has_targets: bool,
    pub(crate) has_environments: bool,
    pub(crate) has_runtime: bool,
    pub(crate) has_deploy: bool,
    pub(crate) has_architecture: bool,
    pub(crate) has_services: bool,
    pub(crate) has_communication: bool,
    pub(crate) deploy_has_migrations: bool,
    pub(crate) deploy_has_rollback: bool,
    pub(crate) runtime_units: Vec<AppRuntimeUnitFacts>,
    pub(crate) services: Vec<AppServiceFacts>,
}

impl AppOperationalFacts {
    fn new(line_index: usize, line: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            has_uses: false,
            has_targets: false,
            has_environments: false,
            has_runtime: false,
            has_deploy: false,
            has_architecture: false,
            has_services: false,
            has_communication: false,
            deploy_has_migrations: false,
            deploy_has_rollback: false,
            runtime_units: Vec::new(),
            services: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppIntegrationFacts;

impl AppIntegrationFacts {
    fn new() -> Self {
        Self
    }
}

pub(crate) fn app_operational_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_app: Option<AppOperationalFacts> = None;
    let mut current_app_child: Option<&'static str> = None;
    let mut current_runtime_unit: Option<usize> = None;
    let mut current_service: Option<usize> = None;
    let mut current_service_child: Option<&'static str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration: Option<AppIntegrationFacts> = None;
    let mut current_integration_child: Option<&'static str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            if let Some(app) = current_app.take() {
                diagnostics.extend(app_operational_block_diagnostics(app));
            }
            current_app_child = None;
            current_runtime_unit = None;
            current_service = None;
            current_service_child = None;
            current_env_group = None;
            current_integration = None;
            current_integration_child = None;

            if trimmed.starts_with("app ") {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() != 2 {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "app-operational-contract",
                        "app manifests use `app <Name>` as the entrypoint header.",
                    ));
                }
                current_app = Some(AppOperationalFacts::new(line_index, line));
            }
            continue;
        }

        let Some(app) = current_app.as_mut() else {
            continue;
        };

        match leading_spaces(line) {
            2 => {
                current_runtime_unit = None;
                current_service = None;
                current_service_child = None;
                current_env_group = None;
                current_integration = None;
                current_integration_child = None;
                if let Some(child) = app_child_block(trimmed) {
                    current_app_child = Some(child);
                    match child {
                        "uses" => app.has_uses = true,
                        "targets" => app.has_targets = true,
                        "environments" => app.has_environments = true,
                        "architecture" => app.has_architecture = true,
                        "services" => app.has_services = true,
                        "communication" => app.has_communication = true,
                        "runtime" => app.has_runtime = true,
                        "deploy" => app.has_deploy = true,
                        _ => {}
                    }
                    validate_app_child_header(&mut diagnostics, line_index, line, trimmed);
                } else if is_app_scalar_child(trimmed) {
                    current_app_child = None;
                    validate_app_scalar_child(&mut diagnostics, line_index, line, trimmed);
                } else {
                    current_app_child = None;
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "app manifests own app/runtime contracts: use `uses`, `packs`, `bindings`, `targets`, `environments`, `urls`, `env`, `integrations`, `capabilities`, `architecture`, `services`, `communication`, `runtime`, or `deploy` blocks.",
                    ));
                }
            }
            4 => match current_app_child {
                Some("uses") => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.starts_with("feature ") {
                        let parts: Vec<_> = trimmed.split_whitespace().collect();
                        if parts.len() < 2 {
                            diagnostics.push(simple_canonical_diagnostic(
                                line_index,
                                line,
                                DiagnosticSeverity::WARNING,
                                "app-operational-contract",
                                "`uses` feature entries use `feature <name> [at \"./path.lzi\"]` or a feature name.",
                            ));
                        }
                    }
                }
                Some("packs") => validate_app_pack_use_line(&mut diagnostics, line_index, line),
                Some("bindings") => validate_app_binding_line(&mut diagnostics, line_index, line),
                Some("targets") => validate_app_target_line(&mut diagnostics, line_index, line),
                Some("environments") => {
                    if !is_identifier(trimmed) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-operational-contract",
                            "environment names should be identifiers such as `local`, `staging`, or `production`.",
                        ));
                    }
                }
                Some("urls") => validate_app_url_line(&mut diagnostics, line_index, line),
                Some("env") => {
                    if let Some(group) = parse_env_group_name(trimmed) {
                        current_env_group = Some(group.to_owned());
                    } else {
                        current_env_group = None;
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                }
                Some("capabilities") => {
                    validate_app_capability_line(&mut diagnostics, line_index, line)
                }
                Some("integrations") => {
                    validate_app_integration_header(&mut diagnostics, line_index, line, trimmed);
                    current_integration = Some(AppIntegrationFacts::new());
                    current_integration_child = None;
                }
                Some("architecture") => {
                    validate_app_architecture_line(&mut diagnostics, line_index, line, trimmed)
                }
                Some("services") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() != 2 || parts[0] != "service" || !is_identifier(parts[1]) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-service-contract",
                            "service boundaries use `service <name>` under `services`.",
                        ));
                        current_service = None;
                        current_service_child = None;
                    } else {
                        app.services
                            .push(AppServiceFacts::new(line_index, line, parts[1]));
                        current_service = app.services.len().checked_sub(1);
                        current_service_child = None;
                    }
                }
                Some("communication") => {
                    validate_app_communication_line(&mut diagnostics, line_index, line, trimmed)
                }
                Some("runtime") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() != 2 || parts[0] != "unit" || !is_identifier(parts[1]) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "app-runtime-contract",
                            "runtime units use `unit <name>` under `runtime`.",
                        ));
                        current_runtime_unit = None;
                    } else {
                        app.runtime_units
                            .push(AppRuntimeUnitFacts::new(line_index, line, parts[1]));
                        current_runtime_unit = app.runtime_units.len().checked_sub(1);
                    }
                }
                Some("deploy") => {
                    validate_app_deploy_line(&mut diagnostics, app, line_index, line, trimmed)
                }
                // Cut A.11 — `cors` children are handled by
                // `cors_contract_diagnostics`. Skip here to avoid the
                // "unknown app block" warning firing on
                // `allow_origins` / `allow_credentials` / `max_age`.
                Some("cors") => {}
                // Roadmap §1.10 — `headers` children are handled by
                // `headers_contract_diagnostics` (file-local shape) +
                // doctor's `headers-contract` (closed catalogs +
                // production-profile completeness). Skip here so the
                // "unknown app block" warning does not fire on
                // `csp` / `hsts` / `x_frame_options` etc.
                Some("headers") => {}
                // Roadmap §1.2 — `cookie` / `proxy` / `limits`
                // children are doctor-validated (cookie profile
                // children at indent 6; proxy/limits scalars at indent
                // 4). Skip the "unknown app block" warning here.
                Some("cookie") | Some("proxy") | Some("limits") => {}
                // Observability bucket cycle row 36 — `logging` /
                // `tracing` children are handled by
                // `app_logging_tracing_diagnostics` (doctor) and the
                // closed-catalog completion in
                // `observability_catalog_detail`. Skip the
                // "unknown app block" warning here.
                Some("logging") | Some("tracing") => {}
                // i18n bucket cycle — `locale` children
                // (`default`/`supported`/`fallback`) are validated by
                // `parse_app_manifest`; skip the "unknown app block"
                // warning here.
                Some("locale") => {}
                // ir-route-guards Cell PARSE-1 — app-level guard
                // defaults are validated by the parser/analyzer.
                Some("route_guard") => {}
                // Encryption bucket cycle — `encryption` children are
                // `key @key.<scope>` lines validated by doctor's
                // encryption_binding_diagnostics; skip the "unknown
                // app block" warning here.
                Some("encryption") => {}
                Some(_) | None => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "app-operational-contract",
                    "nested app manifest declarations must live under a known app block.",
                )),
            },
            6 => {
                if current_app_child == Some("env") {
                    if current_env_group.is_none() {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-env-contract",
                            "six-space env declarations must follow `group <name>` inside `env`.",
                        ));
                    } else {
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                } else if current_app_child == Some("integrations") {
                    if current_integration.is_none() {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-integration-contract",
                            "integration children must follow `<name>: <CapabilityType>` under `integrations`.",
                        ));
                        continue;
                    }

                    validate_app_integration_child(
                        &mut diagnostics,
                        &mut current_integration_child,
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("runtime") {
                    let Some(unit_index) = current_runtime_unit else {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-runtime-contract",
                            "runtime unit children must follow a `unit <name>` declaration.",
                        ));
                        continue;
                    };

                    validate_app_runtime_unit_child(
                        &mut diagnostics,
                        &mut app.runtime_units[unit_index],
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("services") {
                    let Some(service_index) = current_service else {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "app-service-contract",
                            "service boundary children must follow a `service <name>` declaration.",
                        ));
                        continue;
                    };
                    validate_app_service_child(
                        &mut diagnostics,
                        &mut app.services[service_index],
                        &mut current_service_child,
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("encryption") {
                    // Encryption bucket cycle — body of a
                    // `key @key.<scope>` block: `source <expr>`,
                    // `algorithm <name>`, `rotation <cadence>`.
                    // Doctor's `encryption_binding_diagnostics` owns
                    // the closed-catalog validation; the LSP just
                    // needs to NOT fire the generic six-space
                    // warning here.
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "six-space app manifest declarations are only valid inside `env group`, `integrations`, `runtime unit`, `services service`, or `encryption key` blocks.",
                    ));
                }
            }
            8 => {
                if current_app_child == Some("integrations")
                    && current_integration_child == Some("credentials")
                {
                    validate_app_integration_credential_line(
                        &mut diagnostics,
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("services")
                    && current_service_child == Some("exposes")
                {
                    validate_app_service_exposure_line(&mut diagnostics, line_index, line, trimmed);
                } else if current_app_child == Some("runtime")
                    && (trimmed.starts_with("source ")
                        || trimmed.starts_with("strategy ")
                        || trimmed.starts_with("fallback "))
                {
                    // i18n bucket cycle — `locale_negotiate` body lines
                    // sit at indent 8 under `runtime unit api`. The body
                    // grammar is `source <axis>` / `strategy <name>` /
                    // `fallback <tag>`; doctor validates the catalog.
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "app-operational-contract",
                        "eight-space app manifest declarations are only valid inside `integrations credentials`, `services service exposes`, or `runtime unit locale_negotiate` blocks.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-operational-contract",
                "app manifest declarations use two, four, six, or eight spaces of indentation.",
            )),
        }
    }

    if let Some(app) = current_app.take() {
        diagnostics.extend(app_operational_block_diagnostics(app));
    }

    diagnostics
}

pub(crate) fn app_operational_block_diagnostics(app: AppOperationalFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !app.has_uses {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "app manifests should declare `uses` so the entrypoint owns feature registration explicitly.",
        ));
    }

    if !app.has_targets {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::ERROR,
            "app-target-contract",
            "app manifests must declare `targets` so the runtime can materialize backend, web, and mobile outputs deterministically.",
        ));
    }

    if !app.has_environments {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "app manifests should declare `environments` so env, URLs, deploy gates, and runtime safety can be checked per environment.",
        ));
    }

    if app.has_services && !app.has_architecture {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-architecture-contract",
            "apps with `services` should declare `architecture` so boundaries are separated from deploy topology.",
        ));
    }

    if app.has_services && !app.has_communication {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-communication-contract",
            "apps with `services` should declare `communication` context propagation and sync/async intent.",
        ));
    }

    if !app.has_runtime {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-runtime-contract",
            "app manifests should declare `runtime` units such as `api`, `web`, `worker`, and `scheduler`.",
        ));
    } else if app.runtime_units.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-runtime-contract",
            "`runtime` should declare at least one `unit <name>`.",
        ));
    }

    if !app.has_deploy {
        diagnostics.push(simple_canonical_diagnostic(
            app.line_index,
            &app.line,
            DiagnosticSeverity::WARNING,
            "app-deploy-contract",
            "app manifests should declare `deploy` gates for migrations, rollback, and destructive changes without becoming provider-specific infra.",
        ));
    } else {
        if !app.deploy_has_migrations {
            diagnostics.push(simple_canonical_diagnostic(
                app.line_index,
                &app.line,
                DiagnosticSeverity::WARNING,
                "app-deploy-contract",
                "deploy contracts should declare a migrations policy.",
            ));
        }
        if !app.deploy_has_rollback {
            diagnostics.push(simple_canonical_diagnostic(
                app.line_index,
                &app.line,
                DiagnosticSeverity::WARNING,
                "app-deploy-contract",
                "deploy contracts should declare rollback behavior.",
            ));
        }
    }

    for unit in app.runtime_units {
        if !unit.has_serves_or_runs {
            diagnostics.push(simple_canonical_diagnostic(
                unit.line_index,
                &unit.line,
                DiagnosticSeverity::WARNING,
                "app-runtime-contract",
                "runtime units should declare what they `serves` or `runs`.",
            ));
        }
        if unit.name == "api" && !unit.has_healthcheck_or_readiness {
            diagnostics.push(simple_canonical_diagnostic(
                unit.line_index,
                &unit.line,
                DiagnosticSeverity::WARNING,
                "app-runtime-contract",
                "the `api` runtime unit should declare `healthcheck` and/or `readiness` paths for deploy safety.",
            ));
        }
    }

    for service in app.services {
        if !service.has_owns {
            diagnostics.push(simple_canonical_diagnostic(
                service.line_index,
                &service.line,
                DiagnosticSeverity::WARNING,
                "app-service-contract",
                &format!(
                    "service `{}` should declare `owns ...` so feature ownership is explicit.",
                    service.name
                ),
            ));
        }
    }

    diagnostics
}
