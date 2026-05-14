// Package mcp implements the runtime contract for the Lazuli `mcp_server`
// and `mcp_client` DSL kinds. This file is types-only; no I/O lives here.
// The wire (M6/M7) imports the upstream Go SDK; codegen (M8/M9) emits
// against this contract surface.
package mcp

import (
	"context"
	"encoding/json"
	"errors"
)

// Transport is the closed-catalog MCP transport enum.
type Transport string

const (
	TransportStdio          Transport = "stdio"
	TransportHttpSSE        Transport = "http_sse"
	TransportHttpStreamable Transport = "http_streamable"
)

// Scope is an opaque policy-scope string. Codegen emits the resolved
// literal from the `policy @scope.*` atom; M6 passes it to the auth
// middleware before invoking the ToolSpec.Handler.
type Scope string

// ServerSpec is the per-mcp_server registration shape codegen emits.
// Serve (M6) accepts this and binds it to the upstream SDK.
type ServerSpec struct {
	Name        string
	Version     string
	Description string
	Transport   Transport
	Tools       []ToolSpec
	Resources   []ResourceSpec
	Prompts     []PromptSpec
}

// ToolSpec mirrors one `tool <name>` sub-block.
type ToolSpec struct {
	Name          string
	Description   string
	ParamsSchema  json.RawMessage
	ReturnsSchema json.RawMessage
	Handler       func(ctx context.Context, params map[string]any) (any, error)
	Policy        Scope
}

// ResourceSpec mirrors one `resource <name>` sub-block.
type ResourceSpec struct {
	URITemplate string
	MIMEType    string
	Handler     func(ctx context.Context, uri string) ([]byte, error)
}

// PromptSpec mirrors one `prompt <name>` sub-block.
type PromptSpec struct {
	Name         string
	Description  string
	TemplatePath string
	ParamsSchema json.RawMessage
}

// Typed errors — all sentinels are distinct values.
var (
	// ErrMCPProtocol signals a JSON-RPC protocol violation (bad envelope,
	// unknown method). Maps to JSON-RPC -32600 (Invalid Request) or
	// -32700 (Parse error) in M6.
	ErrMCPProtocol = errors.New("mcp: protocol error")

	// ErrMCPNotFound signals that a requested tool, resource, or prompt
	// name is not registered. Maps to JSON-RPC -32601 (Method Not Found).
	ErrMCPNotFound = errors.New("mcp: name not registered")

	// ErrMCPUnauthorized signals that the Bearer token check (or
	// unauthenticated request) failed. Maps to JSON-RPC -32000.
	ErrMCPUnauthorized = errors.New("mcp: unauthorized")

	// ErrMCPHandler signals that a handler itself returned an error.
	// M6 wraps the underlying lazuli.Error.Surface to derive the
	// JSON-RPC error code.
	ErrMCPHandler = errors.New("mcp: handler error")

	// ErrMCPInvalidArgs signals tool argument deserialization failure
	// or a missing required param. Maps to JSON-RPC -32602 (Invalid Params).
	ErrMCPInvalidArgs = errors.New("mcp: invalid tool arguments")

	// ErrMCPTransportUnsupported signals a Transport value outside the
	// closed catalog. Returned by Serve (M6) at startup.
	ErrMCPTransportUnsupported = errors.New("mcp: transport not supported")

	// ErrMCPClientUnavailable signals that the upstream MCP server is
	// unreachable. Returned by Connect (M7).
	ErrMCPClientUnavailable = errors.New("mcp: client unavailable")
)
