//! MCP `tools/list` surface — closed catalog descriptors.
//!
//! The `TOOLS` constant pins the 8-tool catalog per
//! `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §4; growing
//! it requires a proposal + architect re-grade. `tool_descriptor`
//! emits the per-tool MCP descriptor (name, human-prose
//! description, JSON Schema inputSchema) consumed by every
//! conformant MCP client.

use serde_json::{Value, json};

/// Closed-catalog of MCP tool names. Length-pinned by
/// `closed_tool_catalog_is_exactly_8` test.
pub(super) const TOOLS: &[&str] = &[
    "inspect",
    "doctor",
    "features",
    "resources",
    "commands",
    "queries",
    "grammar",
    "docs",
];

pub(super) fn tools_list_result() -> Value {
    let tools: Vec<Value> = TOOLS.iter().map(|name| tool_descriptor(name)).collect();
    json!({ "tools": tools })
}

fn tool_descriptor(name: &str) -> Value {
    match name {
        "inspect" => json!({
            "name": "inspect",
            "description": "Return the typed IR projection of a Lazuli project (mirrors `lazuli inspect --format=json`). Use `expand` to opt into per-axis projections (`refs`, `commands`, `queries`, `resources`, `auth`, `storage`, `jobs`, `webhooks`, `notifications`, `caches`, `aggregates`, `apis`, `records`, `tools`, `expose`, `policies`, `tests`, `events`, `targets`, `defaults`, `security`, `summary`, `locators`, `dependencies`, `tracing`, `logging`, `http`, `event_groups`, `migrations`, `webhook_events`, or `all`).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .lzi file or project directory." },
                    "expand": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of expansion axes. Empty = default projection."
                    }
                },
                "required": ["path"]
            }
        }),
        "doctor" => json!({
            "name": "doctor",
            "description": "Run Lazuli's diagnostic pipeline against a project and return the diagnostic list as JSON (mirrors `lazuli doctor` without exit-coding).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .lzi file or project directory." },
                    "severity": {
                        "type": "string",
                        "enum": ["error", "warning"],
                        "description": "Filter to severities at or above this level. Omit for all."
                    }
                },
                "required": ["path"]
            }
        }),
        "features" => json!({
            "name": "features",
            "description": "List every feature in a Lazuli project with its source files and lifted IR slices.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .lzi file or project directory." }
                },
                "required": ["path"]
            }
        }),
        "resources" => json!({
            "name": "resources",
            "description": "List every declared resource across all features with its lifted shape.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .lzi file or project directory." }
                },
                "required": ["path"]
            }
        }),
        "commands" => json!({
            "name": "commands",
            "description": "List every command across all features with route, input, policy, and effects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .lzi file or project directory." }
                },
                "required": ["path"]
            }
        }),
        "queries" => json!({
            "name": "queries",
            "description": "List every query across all features with its filters and audience.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .lzi file or project directory." }
                },
                "required": ["path"]
            }
        }),
        "grammar" => json!({
            "name": "grammar",
            "description": "Return the canonical Lazuli grammar reference for the requested kind (default: lzi).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["lzi", "lzx", "app", "registry", "contract", "workspace"],
                        "description": "Which grammar reference to return. Default `lzi`."
                    }
                }
            }
        }),
        "docs" => json!({
            "name": "docs",
            "description": "Search Lazuli docs by keyword; returns matching doc names + snippets.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Keyword to search across docs." },
                    "limit": { "type": "integer", "description": "Max results (default 10)." }
                },
                "required": ["query"]
            }
        }),
        _ => json!({}),
    }
}
