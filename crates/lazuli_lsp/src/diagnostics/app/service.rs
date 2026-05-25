//! `services` block validators for `app`.
//!
//! The `services` block draws explicit service boundaries inside an
//! app. Each `service <name>` declares which features it `owns`, what
//! it `exposes` (queries/commands/apis/workflows/reports), and which
//! events it `publishes` / `consumes`. The fact-collecting struct
//! `AppServiceFacts` lives here because the per-service state
//! (`has_owns`) is only consumed by the service-scope validators.
//!
//! Provider-side topology (mesh, sidecar, ingress) is *not* expressed
//! here — those are runtime/adapter concerns.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, simple_canonical_diagnostic, split_items};

#[derive(Debug)]
pub(crate) struct AppServiceFacts {
    pub(crate) line_index: usize,
    pub(crate) line: String,
    pub(crate) name: String,
    pub(crate) has_owns: bool,
}

impl AppServiceFacts {
    pub(crate) fn new(line_index: usize, line: &str, name: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            name: name.to_owned(),
            has_owns: false,
        }
    }
}

pub(crate) fn validate_app_service_child(
    diagnostics: &mut Vec<Diagnostic>,
    service: &mut AppServiceFacts,
    current_service_child: &mut Option<&'static str>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if let Some(rest) = trimmed.strip_prefix("owns ") {
        service.has_owns = true;
        *current_service_child = None;
        if split_items(rest).is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-service-contract",
                "service ownership uses `owns feature_a, feature_b`.",
            ));
        }
        return;
    }

    if trimmed == "exposes" {
        *current_service_child = Some("exposes");
        return;
    }

    if let Some(rest) = trimmed
        .strip_prefix("publishes ")
        .or_else(|| trimmed.strip_prefix("consumes "))
    {
        *current_service_child = None;
        if split_items(rest).is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-service-contract",
                "service event edges use `publishes event.*` or `consumes feature.event_name`.",
            ));
        }
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "app-service-contract",
        "service children use `owns ...`, `exposes`, `publishes ...`, or `consumes ...`.",
    ));
}

pub(crate) fn validate_app_service_exposure_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2
        || !matches!(
            parts[0],
            "query" | "command" | "api" | "workflow" | "report"
        )
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-service-contract",
            "service exposures use `query|command|api|workflow|report <feature>.<kind>.<name>`.",
        ));
    }
}
