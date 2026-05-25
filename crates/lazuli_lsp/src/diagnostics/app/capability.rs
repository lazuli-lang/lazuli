//! `capabilities` block line validator for `app`.
//!
//! `capabilities` is the app's catalog of abstract dependencies
//! (`database postgres`, `queue background_jobs`, `mailer transactional`)
//! that the registry / runtime then binds to a concrete adapter. The
//! point of the line-shape check is to ensure the *kind* slot
//! (`database`, `queue`, `object_storage`, ...) is drawn from the
//! closed catalog Lazuli understands, and that exactly one *name*
//! follows. Anything beyond that — driver, version, region — belongs
//! in the adapter binding, not the contract.
//!
//! When the catalog grows, this is the single line-shape gate; the
//! doctor-side closed-catalog check lives in
//! `diagnostics/registry.rs`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::simple_canonical_diagnostic;

pub(crate) fn validate_app_capability_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2
        || !matches!(
            parts[0],
            "database"
                | "queue"
                | "object_storage"
                | "mailer"
                | "event_bus"
                | "tracing"
                | "search"
                | "cache"
                | "storage"
                | "secret_provider"
                | "integration"
                | "secret_provider"
                | "payment_gateway"
                | "credit_bureau"
        )
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-capability-contract",
            "app capabilities declare intent such as `database postgres`, `queue background_jobs`, `object_storage files`, or `integration crm`; providers stay in adapters under `@runtime/<name>`.",
        ));
    }
}
