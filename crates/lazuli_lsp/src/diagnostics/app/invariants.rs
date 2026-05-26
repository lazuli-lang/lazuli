//! Whole-app invariants — facts-to-diagnostics flush.
//!
//! Called by `app_operational_contract_diagnostics` once the `app`
//! block ends (next top-level header or end-of-file). Emits warnings
//! for missing top-level children (`uses`, `targets`, `environments`,
//! `runtime`, `deploy`), the services/architecture/communication
//! pair-up, deploy gates, runtime-unit verbs, and service ownership.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::simple_canonical_diagnostic;

use super::AppOperationalFacts;

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
