//! MCP server bucket — feature-scoped surface for declaring an MCP
//! (Model Context Protocol) endpoint inside a feature.
//!
//! Reference: `docs/proposals/bucket-mcp-cycle.md`.
//!
//! Sibling of `notification` / `channel` / `poller` at feature scope.
//! The L0 surface design is a closed-children block: `transport`,
//! `scope`, `auth`, `metadata <block>`, `tool <name> <block>`,
//! `resource <name> <block>`, `prompt <name> <block>`. The AST captures
//! the **structured** shape (not freeform text) so doctor + codegen +
//! analyzer can lint and emit deterministically against it.
//!
//! Lowering target: `lazuli_ir::MCPServerSpec`.

use serde::{Deserialize, Serialize};

use super::Span;

/// MCP server endpoint declared inside a feature. Lowered to
/// `ir::MCPServerSpec`. Per the proposal, closed-catalog children:
/// transport / scope / auth / metadata / tool / resource / prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    /// `transport stdio | http_sse | http_streamable` — required, closed catalog.
    pub transport: String,
    /// `scope feature.<name>` — required. Captured as the qualified path.
    pub scope_feature: Option<String>,
    /// `auth bearer env.<NAME>` — optional. Stored verbatim ("bearer env.X").
    pub auth: Option<String>,
    /// `metadata` sub-block contents.
    pub metadata: McpServerMetadata,
    /// `tool <name>` declarations.
    pub tools: Vec<McpTool>,
    /// `resource <name>` declarations.
    pub resources: Vec<McpResource>,
    /// `prompt <name>` declarations.
    pub prompts: Vec<McpPrompt>,
    pub span: Span,
}

/// `metadata` sub-block of an `mcp_server` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct McpServerMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

/// `tool <name>` sub-block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    /// `params` sub-block — list of `<name>: <type> [required|optional]`.
    pub params: Vec<McpParam>,
    /// `returns <KindRef>` — optional reference to a `kind` declared
    /// elsewhere in the app. Stored verbatim; analyzer resolves.
    pub returns: Option<String>,
    /// `handler @fn.<name>` — required.
    pub handler: String,
    /// `policy @policy.<name>` — optional.
    pub policy: Option<String>,
    pub span: Span,
}

/// `resource <name>` sub-block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResource {
    pub name: String,
    /// `uri_template "..."` — required.
    pub uri_template: String,
    /// `mime "..."` — optional; defaults to "application/octet-stream"
    /// at codegen time when absent.
    pub mime: Option<String>,
    /// `handler @fn.<name>` — required.
    pub handler: String,
    /// `policy @policy.<name>` — optional.
    pub policy: Option<String>,
    pub span: Span,
}

/// `prompt <name>` sub-block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: Option<String>,
    /// `params` sub-block (same shape as `McpTool.params`).
    pub params: Vec<McpParam>,
    /// `template "./path/to.tmpl"` — required.
    pub template: String,
    pub span: Span,
}

/// One row inside a `params` sub-block on a tool or prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpParam {
    pub name: String,
    /// Type literal verbatim (`string`, `int`, `enum [...]`, etc.).
    /// The analyzer maps to `ir::ParamType` via `parse_param_type`.
    pub ty: String,
    pub required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_metadata_defaults_are_all_none() {
        let m = McpServerMetadata::default();
        assert!(m.name.is_none());
        assert!(m.description.is_none());
        assert!(m.version.is_none());
    }

    #[test]
    fn mcp_param_required_flag_serde() {
        let p = McpParam {
            name: "limit".into(),
            ty: "int".into(),
            required: true,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: McpParam = serde_json::from_str(&s).unwrap();
        assert!(back.required);
    }
}
