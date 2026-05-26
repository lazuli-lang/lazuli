//! The `app <Name>` operational-contract dispatcher.
//!
//! Walks the source once, accumulates [`AppOperationalFacts`] per open
//! `app` block, and flushes whole-app invariants via
//! [`app_operational_block_diagnostics`] when the block ends or a new
//! one starts. Per-line shape checks are delegated to the sibling
//! sub-modules (`child`, `target`, `env`, `integration`, `capability`,
//! `architecture`, `service`, `runtime`, `deploy`) via the
//! crate-private re-exports in `app/mod.rs`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

use super::{
    app_child_block, app_operational_block_diagnostics, is_app_scalar_child, parse_env_group_name,
    validate_app_architecture_line, validate_app_binding_line, validate_app_capability_line,
    validate_app_child_header, validate_app_communication_line, validate_app_deploy_line,
    validate_app_env_line, validate_app_integration_child,
    validate_app_integration_credential_line, validate_app_integration_header,
    validate_app_pack_use_line, validate_app_runtime_unit_child, validate_app_scalar_child,
    validate_app_service_child, validate_app_service_exposure_line, validate_app_target_line,
    validate_app_url_line, AppIntegrationFacts, AppOperationalFacts, AppRuntimeUnitFacts,
    AppServiceFacts,
};

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
