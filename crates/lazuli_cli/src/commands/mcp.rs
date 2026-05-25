//! `lazuli mcp` — run an MCP (Model Context Protocol) server over stdio.
//!
//! Closed catalog of 8 read-only tools and 4 resource prefixes, exposing
//! Lazuli's introspection surface (inspect, doctor diagnostics, vocab,
//! examples) to MCP-aware AI agents (Claude Code, Zed AI, etc.). The
//! actual MCP server implementation — tool registration, transport
//! framing, request handling — lives in the sibling `crate::cmd_mcp`
//! module; this handler is purely the dispatch shim called from the
//! `Commands::Mcp` clap arm.
//!
//! Cross-refs:
//! - `crate::cmd_mcp::run_mcp_server` — the server entry.
//! - `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` — spec.
//! - Sibling: `commands/lsp.rs` (the language-server transport for
//!   editors, a separate audience from the agent-facing MCP server).

use anyhow::Result;

/// Handler for the `Commands::Mcp` clap arm. Blocks on the MCP server
/// loop until stdin is closed.
pub fn mcp_command() -> Result<()> {
    crate::cmd_mcp::run_mcp_server()
}
