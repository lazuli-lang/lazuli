//! MCP `tools/call` dispatch — eight tool handlers + per-feature
//! projectors.
//!
//! Each tool handler is a small adapter over `inspect_value` plus a
//! per-axis projector (`project_features`, `project_resources`,
//! `project_per_feature`) that flattens the IR's nested feature
//! shape into the MCP-friendly list shape the clients prefer.
//!
//! `wrap_tool_result` wraps any tool's structured JSON payload in
//! the MCP `content` shape (`[{type: "text", text: <pretty JSON>}]`)
//! so it round-trips cleanly across stdio.

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use lazuli_doctor_config::DoctorProfile as SecurityProfile;
use serde_json::{Value, json};

use crate::ExpandSet;

use super::helpers::{
    McpError, expansions_from, filter_diagnostics, inspect_value, lazuli_docs_dir, required_path,
};
use super::protocol::ok_response;
use super::tools_list::TOOLS;

pub(super) fn tools_call_dispatch(id: Value, params: Value) -> Value {
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
