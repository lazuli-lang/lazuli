//! Shared MCP helpers — inspect-value adapter, expansion parsing,
//! docs-dir lookup, and the `McpError` envelope.
//!
//! `inspect_value` is the bridge between the MCP tool surface and
//! the CLI's `inspect_json_value` — every tool that lifts the IR
//! routes through this single helper so directory vs. file path
//! resolution stays in one place. `expansions_from` parses an
//! `expand` JSON-array argument into a typed `ExpandSet`.
//!
//! `lazuli_docs_dir` resolves the docs directory in three steps:
//! `LAZULI_DOCS_DIR` env override, parent-walk from the crate's
//! manifest dir (= dev tree), and finally `cwd/docs/` (= user
//! checkout). Without it the `grammar` and `docs` tools cannot
//! locate their corpus.
//!
//! `McpError` is the two-variant envelope (`User` for protocol-level
//! arg mistakes, `Internal` for anyhow chains from the CLI reuse)
//! that maps to JSON-RPC `-32602` / `-32603` respectively.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::{ExpandSet, InspectInclude, inspect_json_value, parse_expand_set};

use super::protocol::error_response;

pub(super) fn inspect_value(path: &Path, expansions: ExpandSet) -> Result<Value> {
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

pub(super) fn expansions_from(axes: &[String]) -> std::result::Result<ExpandSet, McpError> {
    if axes.is_empty() {
        return Ok(ExpandSet::default());
    }
    let joined = axes.join(",");
    parse_expand_set(&joined).map_err(|err| McpError::user(format!("invalid expand axes: {err}")))
}

pub(super) fn required_path(args: &Value, key: &str) -> std::result::Result<PathBuf, McpError> {
    let raw = args
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::user(format!("`{key}` is required")))?;
    Ok(PathBuf::from(raw))
}

pub(super) fn lazuli_docs_dir() -> Option<PathBuf> {
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

pub(super) fn filter_diagnostics(value: Value, keep: &[&str]) -> Value {
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

/// MCP error envelope. `user` errors are protocol-level (bad args,
/// unknown resource); `internal` errors wrap anyhow chains from the
/// reused CLI internals.
pub(super) enum McpError {
    User(String),
    Internal(anyhow::Error),
}

impl McpError {
    pub(super) fn user(message: String) -> Self {
        Self::User(message)
    }
    pub(super) fn internal(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
    pub(super) fn into_response(self, id: Value) -> Value {
        match self {
            Self::User(msg) => error_response(id, -32602, msg),
            Self::Internal(err) => error_response(id, -32603, format!("{err:#}")),
        }
    }
}
