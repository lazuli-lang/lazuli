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
//! - **Closed catalog** — the `TOOLS` and `RESOURCE_PREFIXES`
//!   constants are pinned. Growing them requires a new proposal +
//!   architect re-grade per
//!   `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §4.
//! - **Read-only** — no tool writes a file, runs codegen, mutates
//!   state, or shells out. Write actions stay in the human-driven CLI.
//! - **Wire-thin** — hand-rolled JSON-RPC dispatch, no
//!   `jsonrpc-core` dependency; the MCP method surface is five verbs
//!   (`initialize`, `tools/list`, `tools/call`, `resources/list`,
//!   `resources/read`) plus `shutdown`.
//!
//! ## Module layout
//!
//! - `protocol` — JSON-RPC 2.0 dispatch core + `initialize` handshake.
//! - `tools_list` — the 8-tool closed catalog + descriptors.
//! - `tools_call` — per-tool handlers + per-feature projectors.
//! - `resources` — the 4-prefix closed catalog + readers.
//! - `helpers` — shared `inspect_value`, expansions, docs-dir, and
//!   the `McpError` envelope.
//!
//! ## Wired internals (grep-verified)
//!
//! - `crate::inspect_json_value` — Returns the typed IR projection JSON
//!   used by tools 1, 3, 4, 5, 6 and resource 3.
//! - `crate::ExpandSet` / `crate::parse_expand_set` — Drive the
//!   `expand` axis surface.
//! - `crate::doctor::doctor_diagnostics_json` — Returns
//!   `Vec<diagnostic>` JSON without printing or bailing.

mod helpers;
mod protocol;
mod resources;
mod tools_call;
mod tools_list;

pub use protocol::run_mcp_server;

/// MCP protocol revision the server speaks. Pinned per
/// `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §9.
pub(crate) const MCP_PROTOCOL_VERSION: &str = "2026-03-26";

/// Server identity returned by `initialize`.
pub(crate) const SERVER_NAME: &str = "lazuli-mcp";
pub(crate) const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::protocol::{dispatch, initialize_result};
    use super::resources::{RESOURCE_PREFIXES, resources_list_result};
    use super::tools_list::{TOOLS, tools_list_result};
    use super::MCP_PROTOCOL_VERSION;

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
