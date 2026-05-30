//! `--expand=expose` projection.
//!
//! Materialises the per-feature unified HTTP route table by joining
//! every agent's `expose_http` block with every `api <name>` top-level
//! block declared on the feature. Each row carries the resolved
//! method/path/route slots/audience/rate-limit-override and a stable
//! `<feature>.<kind>.<name>` origin so cross-feature collation downstream
//! (doctor or external tools) composes cleanly.
//!
//! `api` rows without both a `method` and a `path` are skipped — the
//! contract is incomplete from a routing perspective and surfaces via
//! doctor; inspect remains a read-only projection.

use super::super::expand::leading_spaces;
use super::super::text_walkers::{
    direct_child_value, named_top_block_name, strip_quotes, top_level_blocks,
};
use super::super::{InspectAgent, InspectExposeEntry};

pub(in crate::commands::inspect) fn inspect_expose_projection(
    feature_name: &str,
    agents: &[InspectAgent],
    lines: &[String],
) -> Vec<InspectExposeEntry> {
    let mut entries: Vec<InspectExposeEntry> = Vec::new();

    for agent in agents {
        if let Some(expose) = agent.expose_http.as_ref() {
            entries.push(InspectExposeEntry {
                kind: "agent",
                origin: format!("{feature_name}.agent.{}", agent.name),
                method: expose.method.clone(),
                path: expose.path.clone(),
                route_slots: expose.route_slots.clone(),
                audience: expose.audience.clone(),
                rate_limit_override: expose.rate_limit_override.clone(),
            });
        }
    }

    for block in top_level_blocks(lines, "api ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let method = direct_child_value(block, "method ").map(|m| m.to_ascii_uppercase());
        let path = direct_child_value(block, "path ")
            .as_deref()
            .map(strip_quotes);
        let audience = direct_child_value(block, "audience ");
        let rate_limit_override = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);
        // Walk `route <name>:` children for slots.
        let mut route_slots: Vec<String> = Vec::new();
        let block_indent = block.first().map(|l| leading_spaces(l)).unwrap_or(0);
        let child_indent = block_indent + 2;
        for inner in block.iter().skip(1) {
            let trimmed = inner.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if leading_spaces(inner) != child_indent {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("route ")
                && let Some((slot, _)) = rest.split_once(':')
            {
                route_slots.push(slot.trim().to_owned());
            }
        }

        let (Some(method), Some(path)) = (method, path) else {
            continue;
        };
        entries.push(InspectExposeEntry {
            kind: "api",
            origin: format!("{feature_name}.api.{}", name),
            method,
            path,
            route_slots,
            audience,
            rate_limit_override,
        });
    }

    entries
}
