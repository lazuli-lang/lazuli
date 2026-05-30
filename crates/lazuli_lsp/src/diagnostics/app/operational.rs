//! The `app <Name>` operational-contract dispatcher.
//!
//! Walks the source once, accumulates [`AppOperationalFacts`] per open
//! `app` block, and flushes whole-app invariants via
//! [`app_operational_block_diagnostics`] when the block ends or a new
//! one starts. Per-line shape checks are delegated to the sibling
//! sub-modules (`child`, `target`, `env`, `integration`, `capability`,
//! `architecture`, `service`, `runtime`, `deploy`) via the
//! crate-private re-exports in `app/mod.rs`.

use lazuli_keywords::manifest_child_keys;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::{
    AppIntegrationFacts, AppOperationalFacts, AppRuntimeUnitFacts, AppServiceFacts,
    app_child_block, app_operational_block_diagnostics, is_app_scalar_child, parse_env_group_name,
    validate_app_architecture_line, validate_app_binding_line, validate_app_capability_line,
    validate_app_child_header, validate_app_communication_line, validate_app_deploy_line,
    validate_app_env_line, validate_app_integration_child,
    validate_app_integration_credential_line, validate_app_integration_header,
    validate_app_pack_use_line, validate_app_runtime_unit_child, validate_app_scalar_child,
    validate_app_service_child, validate_app_service_exposure_line, validate_app_target_line,
    validate_app_url_line,
};
use crate::diagnostics::canonical_kinds::closest_kind;
use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

/// App-manifest block headers whose indent-4 (and indent-6 body) child
/// keys the walker validates against the closed catalog
/// [`manifest_child_keys`] returns for them. This is the **single source**
/// the indent-4 / indent-6 match arms below build FROM: every block here is
/// dispatched into [`validate_app_block_child`]; the anti-drift gate
/// `every_manifest_block_has_child_key_validation` asserts (1) every block
/// here has ≥1 registry child row (so it never silently no-ops), and (2)
/// every registry context that maps to a block name via
/// `manifest_block_name` is listed here (so the registry and the walker can
/// never diverge).
///
/// Before BUG-1, each of these arms was an EMPTY skip (`Some("locale") =>
/// {}`), so a misspelled / unknown child key (`fallbacks` instead of
/// `fallback`) was silently dropped by the parser with no diagnostic at
/// `lazuli check` or `lazuli doctor` time. Routing them through the shared
/// helper turns an unknown child into an `app-block-child-contract` ERROR.
pub(crate) const VALIDATED_APP_BLOCKS: &[&str] = &[
    "locale",
    "cors",
    "headers",
    "cookie",
    "proxy",
    "limits",
    "logging",
    "tracing",
    "route_guard",
    "encryption",
    "error_page",
];

/// Validate one indent-4 / indent-6 child line under an app-manifest
/// `block` against the closed child-key catalog the registry carries for
/// that block ([`manifest_child_keys`]).
///
/// Skips lines that are not a plain `<head-key> ...` shape so the contract
/// never false-fires on the legitimate non-keyword body forms the parser
/// accepts:
///   * `@`-prefixed lines (`key @key.tenant` inside `encryption`);
///   * lines containing `:` (`pt-BR: en-US` fallback bodies);
///   * lines containing `=` (`x = y` style bindings).
/// This mirrors the head-token guard in
/// `app_unknown_kind_diagnostics` (`canonical_kinds/sections/blocks.rs`).
///
/// When the head token is not in the (non-empty) catalog it pushes an
/// `app-block-child-contract` ERROR, suggesting the closest catalog key
/// (`closest_kind(head, &catalog, 2)`) when one is within edit distance 2,
/// else listing the whole catalog. A block whose catalog is empty is a
/// no-op (the caller should never route such a block here — the gate
/// enforces that), so an un-cataloged block can never false-fire.
fn validate_app_block_child(
    diagnostics: &mut Vec<Diagnostic>,
    block: &str,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    // Non-keyword body shapes the parser accepts verbatim — never a
    // misspelled child key, so they must not fire the contract.
    if trimmed.starts_with('@') || trimmed.contains(':') || trimmed.contains('=') {
        return;
    }
    // Every caller passes a block from `VALIDATED_APP_BLOCKS` — the const
    // is the single source the dispatch match arms mirror, and the
    // anti-drift gate `every_manifest_block_has_child_key_validation`
    // asserts the two stay in lockstep with the registry.
    debug_assert!(
        VALIDATED_APP_BLOCKS.contains(&block),
        "validate_app_block_child called with unlisted block `{block}` — add it to VALIDATED_APP_BLOCKS"
    );
    let Some(head) = trimmed.split_whitespace().next() else {
        return;
    };

    let catalog: Vec<&'static str> = manifest_child_keys(block).collect();
    if catalog.is_empty() {
        // No closed catalog to validate against → cannot know what is
        // valid, so stay silent (and the gate forbids routing such a
        // block here in the first place).
        return;
    }
    if catalog.contains(&head) {
        return;
    }

    let message = match closest_kind(head, &catalog, 2) {
        Some(suggested) => {
            format!("unknown `{block}` child key `{head}`. Did you mean `{suggested}`?")
        }
        None => {
            let mut valid = catalog.clone();
            valid.sort_unstable();
            format!(
                "unknown `{block}` child key `{head}`. Valid: {}.",
                valid.join(" / ")
            )
        }
    };
    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::ERROR,
        "app-block-child-contract",
        &message,
    ));
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
                // BUG-1 — flat indent-4 child-key blocks. Each child
                // line IS a closed catalog key (`csp` / `allow_origins`
                // / `default` / `template` / …); an unknown / misspelled
                // head was previously dropped silently by the parser
                // with NO diagnostic. Route through the shared
                // `validate_app_block_child` helper so a typo
                // (`fallbacks` vs `fallback`) becomes an
                // `app-block-child-contract` ERROR. The block name fed
                // here is the same `state.current_child` literal the
                // parser dispatches on; the helper looks up the valid
                // child keys from the `lazuli_keywords` registry
                // (`manifest_child_keys`) so the catalog can never drift
                // from the parser. (`cors`/`headers`/`proxy`/`limits`/
                // `logging`/`tracing` shape catalogs are ALSO closed by
                // their respective doctor rules — this adds the
                // file-local unknown-child squiggle the doctor's
                // IR-based pass cannot see, because the dropped key is
                // gone before IR.)
                Some(
                    block @ ("cors" | "headers" | "proxy" | "limits" | "logging" | "tracing"
                    | "locale" | "route_guard" | "error_page"),
                ) => validate_app_block_child(&mut diagnostics, block, line_index, line, trimmed),
                // `cookie` / `encryption` open a per-profile / per-key
                // BINDING at indent 4 (`default` profile name /
                // `key @key.<scope>`), NOT a flat child key — the
                // validated child keys live at indent 6. Profile names
                // are open identifiers and `key @key.<scope>` carries an
                // `@`-reference, so neither is validated against a closed
                // catalog here; their indent-6 bodies are routed through
                // `validate_app_block_child` below.
                Some("cookie") | Some("encryption") => {}
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
                    // BUG-1 — body of a `key @key.<scope>` binding:
                    // `source <expr>`, `algorithm <name>`,
                    // `rotation <cadence>`, `rotation_profile <name>`.
                    // Validate the head against the `encryption` child
                    // catalog so a misspelled key (`algoritm`) becomes
                    // an `app-block-child-contract` ERROR instead of a
                    // silent drop.
                    validate_app_block_child(
                        &mut diagnostics,
                        "encryption",
                        line_index,
                        line,
                        trimmed,
                    );
                } else if current_app_child == Some("cookie") {
                    // BUG-1 — body of a `cookie` profile: `signed`,
                    // `secure`, `http_only`, `same_site`, `max_age`.
                    // These scalar keys live at indent 6 (the indent-4
                    // line is the open-identifier profile name). Validate
                    // the head so a typo (`secrue`) becomes an
                    // `app-block-child-contract` ERROR.
                    validate_app_block_child(&mut diagnostics, "cookie", line_index, line, trimmed);
                } else if current_app_child == Some("locale") {
                    // 2026-05-27 — `locale fallbacks` body: each line is
                    // `<from-locale>: <to-locale>` (e.g. `pt-BR: en-US`).
                    // The `:` infix is skipped by `validate_app_block_child`,
                    // so this body never false-fires; the actual `fallback`
                    // child-key typo is caught at indent 4 above.
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
