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
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::mcp::mcp_command;
///
/// // MCP-aware agents spawn this as a child process talking JSON-RPC
/// // over stdio:
/// // mcp_command()?;
/// ```
pub fn mcp_command() -> Result<()> {
    crate::cmd_mcp::run_mcp_server()
}

#[cfg(test)]
mod tests {
    use super::mcp_command;

    // Smoke: blocks on stdin so we cannot actually run the loop; the
    // pairing just guards the public symbol from regressing.
    #[test]
    fn mcp_command_is_callable() {
        let _ = mcp_command;
    }
}
