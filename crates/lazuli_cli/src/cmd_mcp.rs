//! `lazuli mcp` — Model Context Protocol server (read-only introspection).
//!
//! Runs a JSON-RPC 2.0 server over stdio per the MCP 2026-03-26 spec.
//! Exposes a **closed catalog** of 8 tools + 4 resource prefixes that
//! lift Lazuli's existing introspection surface (`inspect`, `doctor`,
//! grammar/docs) to MCP-aware agent clients (Claude Code, Cursor,
//! Codex, Continue).
//!
//! ## Discipline
//!
//! - **Closed catalog** — the `TOOLS` and `RESOURCE_PREFIXES` constants
//!   are pinned. Growing them requires a new proposal + architect
//!   re-grade per `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md`
//!   §4.
//! - **Read-only** — no tool writes a file, runs codegen, mutates
//!   state, or shells out. Write actions stay in the human-driven CLI.
//! - **Wire-thin** — hand-rolled JSON-RPC dispatch, no
//!   `jsonrpc-core` dependency; the MCP method surface is five verbs
//!   (`initialize`, `tools/list`, `tools/call`, `resources/list`,
//!   `resources/read`) plus `shutdown`.
//!
//! ## Wired internals (grep-verified)
//!
//! - `crate::inspect_json_value` — `main.rs:4342`. Returns the typed
//!   IR projection JSON used by tools 1, 3, 4, 5, 6 and resource 3.
//! - `crate::ExpandSet` / `crate::parse_expand_set` — `main.rs:483` /
//!   `main.rs:5177`. Drive the `expand` axis surface.
//! - `crate::doctor::doctor_diagnostics_json` — `doctor.rs` additive
//!   helper. Returns `Vec<diagnostic>` JSON without printing or bailing.

use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lazuli_lsp::SecurityProfile;
use serde_json::{Value, json};

use crate::{ExpandSet, InspectInclude, inspect_json_value, parse_expand_set};

/// MCP protocol revision the server speaks. Pinned per
/// `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §9.
const MCP_PROTOCOL_VERSION: &str = "2026-03-26";

/// Server identity returned by `initialize`.
const SERVER_NAME: &str = "lazuli-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Closed-catalog of MCP tool names. Length-pinned by
/// `closed_tool_catalog_is_exactly_8` test.
const TOOLS: &[&str] = &[
    "inspect",
    "doctor",
    "features",
    "resources",
    "commands",
    "queries",
    "grammar",
    "docs",
];

/// Closed-catalog of MCP resource URI prefixes. Length-pinned by
/// `closed_resource_prefix_catalog_is_exactly_4` test.
const RESOURCE_PREFIXES: &[&str] = &[
    "lazuli://docs/",
    "lazuli://feature/",
    "lazuli://schema/",
    "lazuli://grammar",
];

/// Entry point wired by `Commands::Mcp` in `main.rs`. Spins the
/// stdio JSON-RPC loop until stdin closes or `shutdown` arrives.
pub fn run_mcp_server() -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line.context("reading MCP request line from stdin")?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(err) => {
                let response = error_response(Value::Null, -32700, format!("parse error: {err}"));
                write_response(&mut out, &response)?;
                continue;
            }
        };

        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        let is_shutdown = method == "shutdown";
        let response = dispatch(method, id, params);

        // Notifications (no `id`) get no response per JSON-RPC 2.0.
        if !response.is_null() {
            write_response(&mut out, &response)?;
        }
        if is_shutdown {
            break;
        }
    }
    Ok(())
}

fn write_response(out: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *out, response).context("writing MCP response")?;
    writeln!(out).context("writing MCP response newline")?;
    out.flush().context("flushing MCP stdout")?;
    Ok(())
}

fn dispatch(method: &str, id: Value, params: Value) -> Value {
    // Notifications (id = null AND well-known notification methods)
    // collapse to a null response that `run_mcp_server` skips.
    let is_notification =
        id.is_null() && (method == "notifications/initialized" || method == "initialized");
    if is_notification {
        return Value::Null;
    }

    match method {
        "initialize" => ok_response(id, initialize_result()),
        "tools/list" => ok_response(id, tools_list_result()),
        "tools/call" => tools_call_dispatch(id, params),
        "resources/list" => ok_response(id, resources_list_result()),
        "resources/read" => resources_read_dispatch(id, params),
        "shutdown" => ok_response(id, Value::Null),
        _ => error_response(id, -32601, format!("method not found: {method}")),
    }
}

// --- initialize ------------------------------------------------------

fn initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false, "subscribe": false },
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION,
        },
    })
}

// --- tools/list ------------------------------------------------------

fn tools_list_result() -> Value {
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

// --- tools/call ------------------------------------------------------

fn tools_call_dispatch(id: Value, params: Value) -> Value {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match name {
        "inspect" => tool_inspect(&args),
        "doctor" => tool_doctor(&args),
        "features" => tool_features(&args),
        "resources" => tool_resources(&args),
        "commands" => tool_commands(&args),
        "queries" => tool_queries(&args),
        "grammar" => tool_grammar(&args),
        "docs" => tool_docs(&args),
        other => Err(McpError::user(format!(
            "unknown tool `{other}` (closed catalog: {})",
            TOOLS.join(", ")
        ))),
    };

    match result {
        Ok(value) => ok_response(id, wrap_tool_result(value)),
        Err(err) => err.into_response(id),
    }
}

/// Wrap a tool's structured JSON payload in the MCP `content` shape.
/// MCP tools return `content: [{type, text}]`; we serialize the JSON
/// as text so it round-trips cleanly across the wire.
fn wrap_tool_result(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "content": [
            { "type": "text", "text": text }
        ],
        "isError": false,
    })
}

fn tool_inspect(args: &Value) -> std::result::Result<Value, McpError> {
    let path = required_path(args, "path")?;
    let expand_axes: Vec<String> = args
        .get("expand")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let expansions = expansions_from(&expand_axes)?;
    inspect_value(&path, expansions).map_err(McpError::internal)
}

fn tool_doctor(args: &Value) -> std::result::Result<Value, McpError> {
    let path = required_path(args, "path")?;
    let severity_filter = args
        .get("severity")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());
    let diagnostics = crate::doctor::doctor_diagnostics_json(&path, SecurityProfile::Strict)
        .map_err(McpError::internal)?;
    let filtered = match severity_filter.as_deref() {
        None => diagnostics,
        Some("error") => filter_diagnostics(diagnostics, &["error"]),
        Some("warning") => filter_diagnostics(diagnostics, &["error", "warning"]),
        Some(other) => {
            return Err(McpError::user(format!(
                "unknown severity `{other}` (allowed: error, warning)"
            )));
        }
    };
    Ok(filtered)
}

fn filter_diagnostics(value: Value, keep: &[&str]) -> Value {
    let Value::Array(items) = value else {
        return value;
    };
    let filtered: Vec<Value> = items
        .into_iter()
        .filter(|item| {
            item.get("severity")
                .and_then(|v| v.as_str())
                .map(|s| keep.contains(&s))
                .unwrap_or(false)
        })
        .collect();
    Value::Array(filtered)
}

fn tool_features(args: &Value) -> std::result::Result<Value, McpError> {
    let path = required_path(args, "path")?;
    let mut expansions = ExpandSet::default();
    expansions.resources = true;
    expansions.commands = true;
    expansions.queries = true;
    let report = inspect_value(&path, expansions).map_err(McpError::internal)?;
    Ok(project_features(&report))
}

fn project_features(report: &Value) -> Value {
    let ir = report.get("ir").unwrap_or(report);
    let empty = Vec::new();
    let features = ir
        .get("features")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty);
    let projected: Vec<Value> = features
        .iter()
        .map(|f| {
            let name = f.get("name").cloned().unwrap_or(Value::Null);
            let path = f.get("path").cloned().unwrap_or(Value::Null);
            let lzi = f.get("lzi").cloned().unwrap_or(Value::Null);
            let lzx = f
                .get("lzx")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let handlers = f
                .get("handlers")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let cells_per_client = f
                .get("cells_per_client")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default()));
            json!({
                "name": name,
                "path": path,
                "lzi": lzi,
                "lzx": lzx,
                "handlers": handlers,
                "cells_per_client": cells_per_client,
            })
        })
        .collect();
    Value::Array(projected)
}

fn tool_resources(args: &Value) -> std::result::Result<Value, McpError> {
    let path = required_path(args, "path")?;
    let mut expansions = ExpandSet::default();
    expansions.resources = true;
    let report = inspect_value(&path, expansions).map_err(McpError::internal)?;
    Ok(project_resources(&report))
}

fn project_resources(report: &Value) -> Value {
    let ir = report.get("ir").unwrap_or(report);
    let empty = Vec::new();
    let features = ir
        .get("features")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty);
    let mut out: Vec<Value> = Vec::new();
    for feature in features {
        let feature_name = feature
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let resources = feature
            .get("resources")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        for resource in resources {
            let name = resource.get("name").cloned().unwrap_or(Value::Null);
            let fields = resource
                .get("fields")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            out.push(json!({
                "feature": feature_name,
                "name": name,
                "fields": fields,
                "raw": resource,
            }));
        }
    }
    Value::Array(out)
}

fn tool_commands(args: &Value) -> std::result::Result<Value, McpError> {
    let path = required_path(args, "path")?;
    let mut expansions = ExpandSet::default();
    expansions.commands = true;
    let report = inspect_value(&path, expansions).map_err(McpError::internal)?;
    Ok(project_per_feature(&report, "commands"))
}

fn tool_queries(args: &Value) -> std::result::Result<Value, McpError> {
    let path = required_path(args, "path")?;
    let mut expansions = ExpandSet::default();
    expansions.queries = true;
    let report = inspect_value(&path, expansions).map_err(McpError::internal)?;
    Ok(project_per_feature(&report, "queries"))
}

fn project_per_feature(report: &Value, key: &str) -> Value {
    let ir = report.get("ir").unwrap_or(report);
    let empty = Vec::new();
    let features = ir
        .get("features")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty);
    let mut out: Vec<Value> = Vec::new();
    for feature in features {
        let feature_name = feature
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let items = feature
            .get(key)
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        for item in items {
            let mut entry = json!({ "feature": feature_name });
            if let Value::Object(map) = &mut entry {
                if let Value::Object(item_map) = item.clone() {
                    for (k, v) in item_map {
                        map.insert(k, v);
                    }
                } else {
                    map.insert("raw".to_owned(), item);
                }
            }
            out.push(entry);
        }
    }
    Value::Array(out)
}

fn tool_grammar(args: &Value) -> std::result::Result<Value, McpError> {
    let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("lzi");
    let allowed = ["lzi", "lzx", "app", "registry", "contract", "workspace"];
    if !allowed.contains(&kind) {
        return Err(McpError::user(format!(
            "unknown grammar kind `{kind}` (allowed: {})",
            allowed.join(", ")
        )));
    }
    let docs_dir = lazuli_docs_dir().ok_or_else(|| {
        McpError::user(
            "could not locate Lazuli docs directory (set LAZULI_DOCS_DIR or run from a Lazuli checkout)".to_owned(),
        )
    })?;
    let path = docs_dir.join(format!("grammar.{kind}.md"));
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))
        .map_err(McpError::internal)?;
    Ok(json!({
        "kind": kind,
        "path": path.display().to_string(),
        "text": text,
    }))
}

fn tool_docs(args: &Value) -> std::result::Result<Value, McpError> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::user("`query` is required".to_owned()))?
        .to_lowercase();
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let docs_dir = lazuli_docs_dir().ok_or_else(|| {
        McpError::user(
            "could not locate Lazuli docs directory (set LAZULI_DOCS_DIR or run from a Lazuli checkout)".to_owned(),
        )
    })?;
    let mut hits: Vec<Value> = Vec::new();
    let entries =
        fs::read_dir(&docs_dir).map_err(|err| McpError::internal(anyhow::anyhow!("{err}")))?;
    let mut sorted: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    sorted.sort();
    for path in sorted {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let lower = text.to_lowercase();
        let Some(idx) = lower.find(&query) else {
            continue;
        };
        let start = idx.saturating_sub(60);
        let end = (idx + query.len() + 80).min(text.len());
        let snippet: String = text[start..end].chars().collect();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        hits.push(json!({
            "name": name,
            "path": path.display().to_string(),
            "snippet": snippet,
        }));
        if hits.len() >= limit {
            break;
        }
    }
    Ok(Value::Array(hits))
}

// --- resources/list --------------------------------------------------

fn resources_list_result() -> Value {
    // MCP `resources/list` advertises both concrete URIs (the static
    // grammar resource) and resourceTemplates (the templated prefixes).
    json!({
        "resources": [
            {
                "uri": "lazuli://grammar",
                "name": "Lazuli grammar (lzi)",
                "description": "Canonical .lzi grammar reference.",
                "mimeType": "text/markdown",
            }
        ],
        "resourceTemplates": [
            {
                "uriTemplate": "lazuli://docs/{name}",
                "name": "Lazuli doc",
                "description": "Read any Lazuli doc by its canonical [[name]].",
                "mimeType": "text/markdown",
            },
            {
                "uriTemplate": "lazuli://feature/{name}",
                "name": "Feature source bundle",
                "description": "Concatenated .lzi + sibling .lzx sources for a feature.",
                "mimeType": "text/plain",
            },
            {
                "uriTemplate": "lazuli://schema/{resource}",
                "name": "Resource schema",
                "description": "Typed schema JSON for a declared resource.",
                "mimeType": "application/json",
            }
        ]
    })
}

// --- resources/read --------------------------------------------------

fn resources_read_dispatch(id: Value, params: Value) -> Value {
    let uri = match params.get("uri").and_then(|v| v.as_str()) {
        Some(uri) => uri.to_owned(),
        None => return error_response(id, -32602, "`uri` parameter is required".to_owned()),
    };
    let result = read_resource(&uri);
    match result {
        Ok(value) => ok_response(id, value),
        Err(err) => err.into_response(id),
    }
}

fn read_resource(uri: &str) -> std::result::Result<Value, McpError> {
    if uri == "lazuli://grammar" {
        let docs_dir = lazuli_docs_dir()
            .ok_or_else(|| McpError::user("could not locate Lazuli docs directory".to_owned()))?;
        let path = docs_dir.join("grammar.lzi.md");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))
            .map_err(McpError::internal)?;
        return Ok(resource_text_content(uri, "text/markdown", &text));
    }
    if let Some(name) = uri.strip_prefix("lazuli://docs/") {
        let docs_dir = lazuli_docs_dir()
            .ok_or_else(|| McpError::user("could not locate Lazuli docs directory".to_owned()))?;
        let path = docs_dir.join(format!("{name}.md"));
        let text = fs::read_to_string(&path)
            .map_err(|err| McpError::user(format!("doc `{name}` not found ({err})")))?;
        return Ok(resource_text_content(uri, "text/markdown", &text));
    }
    if let Some(name) = uri.strip_prefix("lazuli://feature/") {
        return read_feature_bundle(uri, name);
    }
    if let Some(resource_name) = uri.strip_prefix("lazuli://schema/") {
        return read_schema(uri, resource_name);
    }
    Err(McpError::user(format!(
        "unknown resource URI `{uri}` (allowed prefixes: {})",
        RESOURCE_PREFIXES.join(", ")
    )))
}

fn read_feature_bundle(uri: &str, name: &str) -> std::result::Result<Value, McpError> {
    let cwd =
        std::env::current_dir().map_err(|err| McpError::internal(anyhow::anyhow!("{err}")))?;
    // Walk the cwd for `features/<name>/<name>.lzi` and sibling .lzx.
    let candidates = [
        cwd.join("features").join(name).join(format!("{name}.lzi")),
        cwd.join(format!("{name}.lzi")),
    ];
    let mut bundle = String::new();
    let mut found_any = false;
    for candidate in &candidates {
        if candidate.exists() {
            if let Ok(text) = fs::read_to_string(candidate) {
                bundle.push_str(&format!("// === {} ===\n", candidate.display()));
                bundle.push_str(&text);
                bundle.push('\n');
                found_any = true;
                // Also pick up sibling .lzx files in the same directory.
                if let Some(parent) = candidate.parent() {
                    if let Ok(read_dir) = fs::read_dir(parent) {
                        let mut lzx_paths: Vec<PathBuf> = read_dir
                            .filter_map(|e| e.ok())
                            .map(|e| e.path())
                            .filter(|p| {
                                p.extension().and_then(|e| e.to_str()) == Some("lzx")
                                    && p.file_stem()
                                        .and_then(|s| s.to_str())
                                        .map(|s| s.starts_with(name) || s == name)
                                        .unwrap_or(false)
                            })
                            .collect();
                        lzx_paths.sort();
                        for lzx in lzx_paths {
                            if let Ok(text) = fs::read_to_string(&lzx) {
                                bundle.push_str(&format!("// === {} ===\n", lzx.display()));
                                bundle.push_str(&text);
                                bundle.push('\n');
                            }
                        }
                    }
                }
                break;
            }
        }
    }
    if !found_any {
        return Err(McpError::user(format!(
            "feature `{name}` not found (looked under cwd/features/{name}/ and cwd/{name}.lzi)"
        )));
    }
    Ok(resource_text_content(uri, "text/plain", &bundle))
}

fn read_schema(uri: &str, resource_name: &str) -> std::result::Result<Value, McpError> {
    let cwd =
        std::env::current_dir().map_err(|err| McpError::internal(anyhow::anyhow!("{err}")))?;
    let mut expansions = ExpandSet::default();
    expansions.resources = true;
    let report = inspect_value(&cwd, expansions).map_err(McpError::internal)?;
    let ir = report.get("ir").unwrap_or(&report);
    let empty = Vec::new();
    let features = ir
        .get("features")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty);
    for feature in features {
        let resources = feature
            .get("resources")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        for resource in resources {
            let name = resource.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name == resource_name {
                let body = serde_json::to_string_pretty(&resource)
                    .unwrap_or_else(|_| resource.to_string());
                return Ok(resource_text_content(uri, "application/json", &body));
            }
        }
    }
    Err(McpError::user(format!(
        "resource `{resource_name}` not found in current project"
    )))
}

fn resource_text_content(uri: &str, mime: &str, text: &str) -> Value {
    json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": mime,
                "text": text,
            }
        ]
    })
}

// --- helpers ---------------------------------------------------------

fn ok_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn inspect_value(path: &Path, expansions: ExpandSet) -> Result<Value> {
    let source_path = if path.is_dir() {
        path.join("app.lzi")
    } else {
        path.to_path_buf()
    };
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("reading {}", source_path.display()))?;
    let include: Vec<InspectInclude> = Vec::new();
    // Pass `path` as the project-root hint so manifest lookup hits
    // the directory the MCP caller authored.
    inspect_json_value(&source, &source_path, path, expansions, &include)
}

fn expansions_from(axes: &[String]) -> std::result::Result<ExpandSet, McpError> {
    if axes.is_empty() {
        return Ok(ExpandSet::default());
    }
    let joined = axes.join(",");
    parse_expand_set(&joined).map_err(|err| McpError::user(format!("invalid expand axes: {err}")))
}

fn required_path(args: &Value, key: &str) -> std::result::Result<PathBuf, McpError> {
    let raw = args
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::user(format!("`{key}` is required")))?;
    Ok(PathBuf::from(raw))
}

fn lazuli_docs_dir() -> Option<PathBuf> {
    if let Ok(env) = std::env::var("LAZULI_DOCS_DIR") {
        let p = PathBuf::from(env);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Search upward from `CARGO_MANIFEST_DIR` (= crates/lazuli_cli) for
    // a sibling `docs/` directory (= lazuli checkout root).
    let mut cur: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = cur.join("docs");
        if candidate.join("grammar.lzi.md").exists() {
            return Some(candidate);
        }
        if !cur.pop() {
            break;
        }
    }
    // Fall back to cwd/docs (covers `lazuli mcp` launched from a
    // user's Lazuli checkout, not the dev tree).
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join("docs");
        if candidate.join("grammar.lzi.md").exists() {
            return Some(candidate);
        }
    }
    None
}

// --- error envelope --------------------------------------------------

/// MCP error envelope. `user` errors are protocol-level (bad args,
/// unknown resource); `internal` errors wrap anyhow chains from the
/// reused CLI internals.
enum McpError {
    User(String),
    Internal(anyhow::Error),
}

impl McpError {
    fn user(message: String) -> Self {
        Self::User(message)
    }
    fn internal(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
    fn into_response(self, id: Value) -> Value {
        match self {
            Self::User(msg) => error_response(id, -32602, msg),
            Self::Internal(err) => error_response(id, -32603, format!("{err:#}")),
        }
    }
}

// --- tests -----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_tool_catalog_is_exactly_8() {
        assert_eq!(TOOLS.len(), 8, "MCP tool catalog is pinned at 8");
    }

    #[test]
    fn closed_resource_prefix_catalog_is_exactly_4() {
        assert_eq!(
            RESOURCE_PREFIXES.len(),
            4,
            "MCP resource prefix catalog is pinned at 4"
        );
    }

    #[test]
    fn no_write_tool_advertised() {
        let forbidden = [
            "generate", "migrate", "new", "write", "exec", "eval", "shell", "compile", "run",
            "build", "delete", "remove",
        ];
        for tool in TOOLS {
            for word in &forbidden {
                assert!(
                    !tool.contains(word),
                    "tool `{tool}` contains write-like keyword `{word}` — MCP is read-only"
                );
            }
        }
    }

    #[test]
    fn tools_list_advertises_all_8() {
        let value = tools_list_result();
        let tools = value.get("tools").and_then(|v| v.as_array()).unwrap();
        assert_eq!(tools.len(), 8);
        let names: Vec<&str> = tools
            .iter()
            .map(|t| t.get("name").and_then(|v| v.as_str()).unwrap())
            .collect();
        for expected in TOOLS {
            assert!(names.contains(expected), "missing tool `{expected}`");
        }
    }

    #[test]
    fn resources_list_includes_grammar() {
        let value = resources_list_result();
        let resources = value.get("resources").and_then(|v| v.as_array()).unwrap();
        let has_grammar = resources.iter().any(|r| {
            r.get("uri")
                .and_then(|v| v.as_str())
                .map(|s| s == "lazuli://grammar")
                .unwrap_or(false)
        });
        assert!(has_grammar, "lazuli://grammar must be in resources/list");
    }

    #[test]
    fn initialize_advertises_pinned_protocol_version() {
        let value = initialize_result();
        assert_eq!(
            value.get("protocolVersion").and_then(|v| v.as_str()),
            Some(MCP_PROTOCOL_VERSION)
        );
    }

    #[test]
    fn dispatch_unknown_method_returns_minus_32601() {
        let response = dispatch("totally_not_a_method", json!(7), json!({}));
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn dispatch_shutdown_returns_null_result() {
        let response = dispatch("shutdown", json!(1), Value::Null);
        assert!(response.get("result").is_some());
    }
}
