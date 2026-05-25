//! Diagnostics for the `cache` block under `query.*` and the
//! `invalidates` block under `command`.
//!
//! - [`cache_contract_diagnostics`] walks the source and feeds the two
//!   downstream facts producers below.
//! - [`query_cache_diagnostics`] surfaces "cache requires `key` + `ttl`"
//!   on a single `cache` block.
//! - [`command_invalidation_diagnostics`] surfaces "invalidates requires
//!   at least one query target" on a single `invalidates` block.
//!
//! TTL values must be either a quoted prose string or a duration literal
//! recognised by `is_duration_literal` (`30s`, `10m`, `1h`, `7d`).
//! Invalidation entries must target queries (`<feature>.query.<name>`,
//! `query.<name>`, `query.*`, `<feature>.query.*`).

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    is_duration_literal, leading_spaces, simple_canonical_diagnostic, unquote_lzx_literal,
};

#[derive(Debug)]
pub(crate) struct QueryCacheFacts {
    line_index: usize,
    line: String,
    has_key: bool,
    has_ttl: bool,
}

#[derive(Debug)]
pub(crate) struct CommandInvalidationFacts {
    line_index: usize,
    line: String,
    entries: usize,
}

pub(crate) fn cache_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_query = false;
    let mut in_command = false;
    let mut current_cache: Option<QueryCacheFacts> = None;
    let mut current_invalidates: Option<CommandInvalidationFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) <= 4 {
            if let Some(cache) = current_cache.take() {
                diagnostics.extend(query_cache_diagnostics(cache));
            }
        }
        if leading_spaces(line) <= 4 {
            if let Some(invalidates) = current_invalidates.take() {
                diagnostics.extend(command_invalidation_diagnostics(invalidates));
            }
        }

        match leading_spaces(line) {
            2 => {
                in_command = trimmed.starts_with("command ");
                in_query = false;
            }
            4 => {
                in_query = trimmed.starts_with("query.");
                if !trimmed.starts_with("command ") {
                    in_command = in_command && !trimmed.starts_with("api ");
                }
            }
            _ => {}
        }

        if leading_spaces(line) == 6 && trimmed == "cache" {
            if !in_query {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-contract",
                    "`cache` belongs under a `query.*` declaration.",
                ));
                continue;
            }
            current_cache = Some(QueryCacheFacts {
                line_index,
                line: line.to_owned(),
                has_key: false,
                has_ttl: false,
            });
            continue;
        }

        if let Some(cache) = current_cache.as_mut()
            && leading_spaces(line) == 8
        {
            if trimmed.starts_with("key ") {
                cache.has_key = true;
            } else if let Some(ttl) = trimmed.strip_prefix("ttl ") {
                cache.has_ttl = true;
                let value = unquote_lzx_literal(ttl.trim());
                if !ttl.trim().starts_with('"') && !is_duration_literal(value) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "cache-contract",
                        "cache ttl should be quoted prose or a duration literal such as `30s`, `10m`, `1h`, or `7d`.",
                    ));
                }
            }
            continue;
        }

        if leading_spaces(line) == 4 && trimmed == "invalidates" {
            if !in_command {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-invalidation-contract",
                    "`invalidates` belongs as a command child.",
                ));
                continue;
            }
            current_invalidates = Some(CommandInvalidationFacts {
                line_index,
                line: line.to_owned(),
                entries: 0,
            });
            continue;
        }

        if let Some(invalidates) = current_invalidates.as_mut()
            && leading_spaces(line) == 6
        {
            // Accepted forms (per docs/invariants.md):
            //   <feature>.query.<name>              — fully qualified
            //   <feature>.query.<name>(<args>)      — fully qualified with args
            //   <feature>.query.*                   — feature-local wildcard
            //   query.<name>                        — same-feature short form
            //   query.*                             — same-feature wildcard
            let entry = trimmed.split_whitespace().next().unwrap_or("");
            let valid = entry.contains(".query.") || entry.starts_with("query.");
            if !valid {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "cache-invalidation-contract",
                    "cache invalidation entries should target queries: `<feature>.query.<name>`, `<feature>.query.*`, `query.<name>` (same feature), or `query.*` (same feature).",
                ));
            }
            invalidates.entries += 1;
        }
    }

    if let Some(cache) = current_cache {
        diagnostics.extend(query_cache_diagnostics(cache));
    }
    if let Some(invalidates) = current_invalidates {
        diagnostics.extend(command_invalidation_diagnostics(invalidates));
    }

    diagnostics
}

pub(crate) fn query_cache_diagnostics(cache: QueryCacheFacts) -> Vec<Diagnostic> {
    if cache.has_key && cache.has_ttl {
        return Vec::new();
    }

    let mut missing = Vec::new();
    if !cache.has_key {
        missing.push("key");
    }
    if !cache.has_ttl {
        missing.push("ttl");
    }

    vec![simple_canonical_diagnostic(
        cache.line_index,
        &cache.line,
        DiagnosticSeverity::WARNING,
        "cache-contract",
        &format!(
            "query cache contracts should declare {} so generated clients can share stable cache keys and stale-time behavior.",
            missing.join(", ")
        ),
    )]
}

pub(crate) fn command_invalidation_diagnostics(
    invalidates: CommandInvalidationFacts,
) -> Vec<Diagnostic> {
    if invalidates.entries > 0 {
        return Vec::new();
    }

    vec![simple_canonical_diagnostic(
        invalidates.line_index,
        &invalidates.line,
        DiagnosticSeverity::WARNING,
        "cache-invalidation-contract",
        "`invalidates` should list at least one query target.",
    )]
}
