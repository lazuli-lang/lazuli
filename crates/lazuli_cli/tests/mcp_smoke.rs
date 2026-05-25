//! Integration tests for `lazuli mcp` — the Model Context Protocol
//! server over stdio.
//!
//! Per `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §7.
//!
//! Each test spawns the `lazuli` binary with the `mcp` subcommand,
//! writes a sequence of newline-delimited JSON-RPC requests to stdin,
//! and parses the newline-delimited JSON-RPC responses from stdout.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn full_capsule_dir() -> PathBuf {
    repo_root().join("examples").join("full-capsule")
}

/// Helper: spawn `lazuli mcp` with cwd at `full-capsule/`, send the
/// requests, read N responses, then close stdin (which terminates the
/// server). Returns the parsed responses in order.
fn run_mcp_session(requests: &[Value], expect_responses: usize) -> Vec<Value> {
    let mut child: Child = Command::new(cli_bin())
        .arg("mcp")
        .current_dir(full_capsule_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lazuli mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for req in requests {
            let line = serde_json::to_string(req).unwrap();
            writeln!(stdin, "{line}").expect("write request");
        }
        stdin.flush().ok();
    }
    // Drop stdin so the server's BufRead loop ends.
    drop(child.stdin.take());

    let stdout = child.stdout.take().expect("stdout");
    let reader = BufReader::new(stdout);
    let mut responses: Vec<Value> = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => panic!("invalid JSON from server: {err}\nline: {line}"),
        };
        responses.push(value);
        if responses.len() >= expect_responses {
            break;
        }
    }

    let _ = child.wait();
    responses
}

#[test]
fn mcp_lists_8_tools() {
    let requests = vec![
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }),
    ];
    let responses = run_mcp_session(&requests, 2);
    assert_eq!(
        responses.len(),
        2,
        "expected initialize + tools/list responses"
    );

    let tools_list = &responses[1];
    let tools = tools_list["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list result.tools is not an array: {tools_list}"));
    assert_eq!(tools.len(), 8, "closed catalog: exactly 8 tools");

    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap_or(""))
        .collect();
    for expected in &[
        "inspect",
        "doctor",
        "features",
        "resources",
        "commands",
        "queries",
        "grammar",
        "docs",
    ] {
        assert!(
            names.contains(expected),
            "tools/list missing `{expected}` (got {names:?})"
        );
    }
}

#[test]
fn mcp_inspect_call_returns_ir_projection() {
    let path = full_capsule_dir().display().to_string();
    let requests = vec![
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "inspect", "arguments": { "path": path } }
        }),
    ];
    let responses = run_mcp_session(&requests, 2);
    assert_eq!(responses.len(), 2);

    let call = &responses[1];
    assert!(
        call.get("error").is_none(),
        "tools/call inspect returned an error: {call}"
    );
    let content = call["result"]["content"]
        .as_array()
        .expect("result.content is an array");
    assert_eq!(content.len(), 1, "single text content block");
    let text = content[0]["text"].as_str().expect("text content");
    // The inspect projection is JSON serialised as a string; parse it.
    let inner: Value = serde_json::from_str(text).expect("inspect text payload parses as JSON");
    // Either `{ir: {...}, manifest: {...}}` (when a manifest is
    // present) or the raw IR projection (no manifest). full-capsule
    // ships a manifest, but we accept either shape so the test does
    // not regress if the fixture drops it.
    let ir = inner.get("ir").unwrap_or(&inner);
    assert!(
        ir.get("features").is_some(),
        "inspect projection must include `features`: {inner}"
    );
}

#[test]
fn mcp_doctor_call_returns_diagnostics() {
    let path = full_capsule_dir().display().to_string();
    let requests = vec![
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "doctor", "arguments": { "path": path } }
        }),
    ];
    let responses = run_mcp_session(&requests, 2);
    assert_eq!(responses.len(), 2);

    let call = &responses[1];
    assert!(
        call.get("error").is_none(),
        "tools/call doctor returned an error: {call}"
    );
    let content = call["result"]["content"]
        .as_array()
        .expect("result.content is an array");
    let text = content[0]["text"].as_str().expect("text content");
    let inner: Value = serde_json::from_str(text).expect("doctor text payload parses as JSON");
    assert!(
        inner.is_array(),
        "doctor result must be a JSON array of diagnostics: {inner}"
    );
    // Each diagnostic carries the expected shape.
    if let Some(arr) = inner.as_array() {
        for diag in arr {
            assert!(diag.get("severity").is_some(), "diagnostic.severity");
            assert!(diag.get("code").is_some(), "diagnostic.code");
            assert!(diag.get("message").is_some(), "diagnostic.message");
        }
    }
}

#[test]
fn mcp_resources_list_includes_grammar() {
    let requests = vec![
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list", "params": {} }),
    ];
    let responses = run_mcp_session(&requests, 2);
    assert_eq!(responses.len(), 2);

    let list = &responses[1];
    let resources = list["result"]["resources"]
        .as_array()
        .unwrap_or_else(|| panic!("resources/list result.resources missing: {list}"));
    let has_grammar = resources.iter().any(|r| {
        r.get("uri")
            .and_then(|v| v.as_str())
            .map(|s| s == "lazuli://grammar")
            .unwrap_or(false)
    });
    assert!(
        has_grammar,
        "resources/list must include `lazuli://grammar`: {list}"
    );

    // Resource templates also exposed.
    let templates = list["result"]["resourceTemplates"]
        .as_array()
        .expect("resourceTemplates array");
    let template_uris: Vec<&str> = templates
        .iter()
        .filter_map(|t| t["uriTemplate"].as_str())
        .collect();
    for expected in &[
        "lazuli://docs/{name}",
        "lazuli://feature/{name}",
        "lazuli://schema/{resource}",
    ] {
        assert!(
            template_uris.contains(expected),
            "resourceTemplates missing `{expected}` (got {template_uris:?})"
        );
    }
}
