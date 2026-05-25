//! Model Context Protocol (MCP) server IR — feature-scoped projections.
//!
//! `mcp_server <name>` is the language slot that lets a feature expose
//! a slice of its own surface — selected commands, queries, resources,
//! prompts — over the **Model Context Protocol** so external LLM agents
//! can call it as a tool. The IR family in this module mirrors that
//! slot byte-for-byte, with no transport mechanics: the actual JSON-RPC
//! handlers, SDK wiring, and stdio/HTTP plumbing live in
//! `runtime/go/lazuli/mcp` (Lazuli Go), invoked by emitters that consume
//! [`MCPServerSpec`] from the IR.
//!
//! ## Why a separate family
//!
//! MCP is a sibling of `notifications` / `channels` / `pollers` on
//! `Feature`: it's an outbound surface the feature publishes. Keeping
//! the types together makes the catalog visible in one place — a
//! reader looking at "what does this feature project over MCP?" sees
//! the tools / resources / prompts side-by-side rather than scrolling
//! through a mixed 7000-LOC IR file.
//!
//! ## Closed catalogs
//!
//! Two enums are intentionally **closed**:
//!
//! - [`MCPTransport`] — `stdio` | `http_sse` | `http_streamable`.
//!   Doctor `MCP-TRANSPORT-001` enforces. Widening requires a proposal
//!   because each transport implies a different runtime adapter shape.
//! - [`MCPAuth`] — v0 is bearer-via-env only. Future OAuth / mTLS
//!   shapes widen the enum additively without breaking existing
//!   bearer payloads.
//!
//! ## Boundary
//!
//! These are **pure declarations**. No protocol mechanics live here:
//! initialize/list/call dispatch, capability negotiation, schema
//! emission to JSON Schema — all run in the runtime + codegen. The
//! IR records *what* the feature exposes; *how* is owned by Lazuli Go.
//!
//! ## See also
//!
//! - `docs/proposals/bucket-mcp-cycle.md` §L1 — surface design + IR
//!   contract
//! - `runtime/go/lazuli/mcp` — byte-level adapter against the upstream
//!   Go SDK that consumes the emitted `*.mcp.gen.go`

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// MCP server endpoint declaration. Codegen emits one
/// `<feature>_mcp_<name>.mcp.gen.go` per entry, wiring the runtime helper
/// against the upstream Go SDK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPServerSpec {
    pub name: String,
    /// Closed catalog: `stdio` | `http_sse` | `http_streamable`.
    pub transport: MCPTransport,
    /// `scope feature.<name>` — captures which feature surface this
    /// server projects. None for top-level mcp_server (future).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_feature: Option<String>,
    /// `auth bearer env.<NAME>` — optional. Stored verbatim
    /// (the env var name lives in `MCPAuth::BearerEnvVar`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<MCPAuth>,
    pub metadata: MCPServerMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<MCPTool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<MCPResource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<MCPPrompt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of MCP wire transports. Doctor `MCP-TRANSPORT-001`
/// enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MCPTransport {
    Stdio,
    HttpSse,
    HttpStreamable,
}

/// MCP auth shape. v0 only supports bearer-via-env; future expansions
/// (OAuth, mTLS) widen the enum without breaking the bearer form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum MCPAuth {
    BearerEnvVar { env: String },
}

/// Server metadata projected over the MCP `initialize` response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPServerMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A single MCP tool declaration. `handler` is an `@fn.<name>` ref
/// resolved by codegen against the feature's `handlers/<name>.go`
/// (or, when scope is cross-feature, the resolution is done by the
/// analyzer ahead of codegen).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<MCPParam>,
    /// `returns <KindRef>` — optional verbatim type reference. Doctor
    /// `MCP-TOOL-RETURNS-001` validates it resolves to a known kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns_kind: Option<String>,
    /// `handler @fn.<name>` — required.
    pub handler_fn: String,
    /// `policy @policy.<name>` — optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// A single MCP resource declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPResource {
    pub name: String,
    pub uri_template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    pub handler_fn: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// A single MCP prompt declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPPrompt {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<MCPParam>,
    pub template_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One parameter row inside an `MCPTool` or `MCPPrompt`. `ty_literal`
/// is the verbatim author-side type token (`string`, `int`,
/// `enum [a, b]`, etc.); codegen renders it to JSON Schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPParam {
    pub name: String,
    pub ty_literal: String,
    pub required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_round_trips_snake_case() {
        let s = serde_json::to_string(&MCPTransport::HttpStreamable).unwrap();
        assert_eq!(s, "\"http_streamable\"");
        let back: MCPTransport = serde_json::from_str(&s).unwrap();
        assert_eq!(back, MCPTransport::HttpStreamable);
    }

    #[test]
    fn auth_uses_tag_kind_content_value_envelope() {
        let a = MCPAuth::BearerEnvVar {
            env: "OPENAI_API_KEY".into(),
        };
        let s = serde_json::to_string(&a).unwrap();
        // Tag+content envelope: {"kind":"BearerEnvVar","value":{"env":"..."}}
        assert!(s.contains("\"kind\":\"BearerEnvVar\""));
        assert!(s.contains("\"value\""));
        assert!(s.contains("\"env\":\"OPENAI_API_KEY\""));
        let back: MCPAuth = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn server_metadata_skips_none_fields() {
        let m = MCPServerMetadata {
            name: Some("billing".into()),
            description: None,
            version: None,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"name\":\"billing\""));
        assert!(!s.contains("description"));
        assert!(!s.contains("version"));
    }

    #[test]
    fn server_spec_round_trip() {
        let spec = MCPServerSpec {
            name: "billing".into(),
            transport: MCPTransport::Stdio,
            scope_feature: Some("billing".into()),
            auth: None,
            metadata: MCPServerMetadata::default(),
            tools: vec![MCPTool {
                name: "create_invoice".into(),
                description: Some("Create an invoice for a customer".into()),
                params: vec![MCPParam {
                    name: "customer_id".into(),
                    ty_literal: "string".into(),
                    required: true,
                }],
                returns_kind: Some("Invoice".into()),
                handler_fn: "@fn.create_invoice".into(),
                policy: Some("@policy.billing.write".into()),
                span_ref: None,
            }],
            resources: vec![],
            prompts: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&spec).unwrap();
        let back: MCPServerSpec = serde_json::from_str(&s).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn empty_tools_resources_prompts_omit_from_json() {
        let spec = MCPServerSpec {
            name: "n".into(),
            transport: MCPTransport::Stdio,
            scope_feature: None,
            auth: None,
            metadata: MCPServerMetadata::default(),
            tools: vec![],
            resources: vec![],
            prompts: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&spec).unwrap();
        assert!(!s.contains("tools"));
        assert!(!s.contains("resources"));
        assert!(!s.contains("prompts"));
    }

    #[test]
    fn param_required_round_trips() {
        let p = MCPParam {
            name: "x".into(),
            ty_literal: "int".into(),
            required: false,
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"required\":false"));
        let back: MCPParam = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }
}
