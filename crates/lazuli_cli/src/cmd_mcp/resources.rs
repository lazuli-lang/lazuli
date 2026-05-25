//! MCP `resources/list` + `resources/read` surface.
//!
//! Advertises the static `lazuli://grammar` resource and three
//! resource templates (`lazuli://docs/{name}`,
//! `lazuli://feature/{name}`, `lazuli://schema/{resource}`). The
//! `RESOURCE_PREFIXES` constant pins the catalog length per
//! `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §4.
//!
//! `read_resource` dispatches by URI prefix to one of three
//! readers: grammar markdown, doc markdown, feature-bundle text
//! (concatenated .lzi + sibling .lzx), or resource schema JSON.

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde_json::{Value, json};

use crate::ExpandSet;

use super::helpers::{McpError, inspect_value, lazuli_docs_dir};
use super::protocol::{error_response, ok_response};

/// Closed-catalog of MCP resource URI prefixes. Length-pinned by
/// `closed_resource_prefix_catalog_is_exactly_4` test.
pub(super) const RESOURCE_PREFIXES: &[&str] = &[
    "lazuli://docs/",
    "lazuli://feature/",
    "lazuli://schema/",
    "lazuli://grammar",
];

pub(super) fn resources_list_result() -> Value {
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

pub(super) fn resources_read_dispatch(id: Value, params: Value) -> Value {
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
