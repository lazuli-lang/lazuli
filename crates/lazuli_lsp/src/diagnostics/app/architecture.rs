//! `architecture` / `communication` line validators for `app`.
//!
//! These two blocks declare the cross-service topology of a Lazuli
//! application:
//!
//! * `architecture` — `mode monolith|modular_monolith|microservices`
//!   plus the two service-boundary booleans.
//! * `communication` — internal sync transport, external transport,
//!   async transport, context-propagation set, timeout, retry policy.
//!
//! Both are intentionally narrow closed catalogs. Provider-specific
//! transport (HTTP/2, gRPC stub flags, broker partitions) is *not*
//! expressible here; it lives in adapters. The validators only check
//! the surface shape.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_quoted_lzx_literal, simple_canonical_diagnostic, split_items};

pub(crate) fn validate_app_architecture_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["mode", value]
            if matches!(*value, "monolith" | "modular_monolith" | "microservices") => {}
        ["service_ready", value] | ["enforce_service_boundaries", value]
            if matches!(*value, "true" | "false") => {}
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-architecture-contract",
            "architecture lines use `mode monolith|modular_monolith|microservices`, `service_ready true|false`, or `enforce_service_boundaries true|false`.",
        )),
    }
}

pub(crate) fn validate_app_communication_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["internal", "sync", value] if matches!(*value, "rpc" | "http" | "in_process") => {}
        ["external", value] if matches!(*value, "http") => {}
        ["async", value] if matches!(*value, "event_bus" | "in_process") => {}
        ["propagate", rest @ ..]
            if !rest.is_empty()
                && split_items(&rest.join(" ")).iter().all(|item| {
                    matches!(
                        item.as_str(),
                        "actor" | "tenant" | "trace_id" | "request_id" | "locale"
                    )
                }) => {}
        ["timeout", "default", value] if is_quoted_lzx_literal(value) => {}
        ["retry", "default", count, "backoff", strategy]
            if count.parse::<u32>().is_ok() && matches!(*strategy, "fixed" | "exponential") => {}
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-communication-contract",
            "communication lines use `internal sync rpc|http|in_process`, `external http`, `async event_bus|in_process`, `propagate ...`, `timeout default \"...\"`, or `retry default <n> backoff fixed|exponential`.",
        )),
    }
}
