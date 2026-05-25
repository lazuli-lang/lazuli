//! JSON-RPC 2.0 dispatch core for `lazuli mcp`.
//!
//! Holds the stdio loop (`run_mcp_server`), the method `dispatch`
//! router, the `initialize` handshake response, and the
//! `ok_response` / `error_response` envelope constructors. The
//! tool surface (tools/list, tools/call) and resource surface
//! (resources/list, resources/read) are dispatched out to their
//! own modules so this file stays the wire-thin entry.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::resources::{resources_list_result, resources_read_dispatch};
use super::tools_call::tools_call_dispatch;
use super::tools_list::tools_list_result;
use super::{MCP_PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION};

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

pub(super) fn dispatch(method: &str, id: Value, params: Value) -> Value {
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

pub(super) fn initialize_result() -> Value {
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

pub(super) fn ok_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub(super) fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
