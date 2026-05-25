//! `runtime` block validators for `app`.
//!
//! `runtime` enumerates the deployable processes (`api`, `web`,
//! `worker`, `scheduler`) and the contract each one carries: what it
//! `serves`/`runs`, its `healthcheck`/`readiness` paths, and the
//! `locale_negotiate` block (i18n bucket cycle). The fact struct
//! `AppRuntimeUnitFacts` collects whether `serves`/`runs` and a
//! probe path were declared, so the block-level diagnostic in
//! `app/mod.rs` can flag missing safety nets on the `api` unit.
//!
//! Provider-side runtime mechanics (replicas, autoscaling, mTLS) are
//! adapter/runtime concerns and stay out of this validator.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{simple_canonical_diagnostic, unquote_lzx_literal};

#[derive(Debug)]
pub(crate) struct AppRuntimeUnitFacts {
    pub(crate) line_index: usize,
    pub(crate) line: String,
    pub(crate) name: String,
    pub(crate) has_serves_or_runs: bool,
    pub(crate) has_healthcheck_or_readiness: bool,
}

impl AppRuntimeUnitFacts {
    pub(crate) fn new(line_index: usize, line: &str, name: &str) -> Self {
        Self {
            line_index,
            line: line.to_owned(),
            name: name.to_owned(),
            has_serves_or_runs: false,
            has_healthcheck_or_readiness: false,
        }
    }
}

pub(crate) fn validate_app_runtime_unit_child(
    diagnostics: &mut Vec<Diagnostic>,
    unit: &mut AppRuntimeUnitFacts,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if trimmed.starts_with("serves ") || trimmed.starts_with("runs ") {
        unit.has_serves_or_runs = true;
        return;
    }

    if let Some(path) = trimmed
        .strip_prefix("healthcheck ")
        .or_else(|| trimmed.strip_prefix("readiness "))
    {
        unit.has_healthcheck_or_readiness = true;
        let path = unquote_lzx_literal(path.trim());
        if !path.starts_with('/') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "app-runtime-contract",
                "runtime healthcheck/readiness paths should be absolute paths such as `\"/healthz\"`.",
            ));
        }
        return;
    }

    // i18n bucket cycle — `locale_negotiate` opens a child block whose
    // entries land at indent 8. The LSP file-local rule accepts the
    // header; doctor validates the body via the IR
    // (`locale_negotiate_source_invalid`, `_strategy_invalid`,
    // `app_locale_fallback_unknown_dest`).
    if trimmed == "locale_negotiate" {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "app-runtime-contract",
        "runtime unit children use `serves ...`, `runs ...`, `healthcheck \"...\"`, `readiness \"...\"`, or `locale_negotiate`.",
    ));
}
