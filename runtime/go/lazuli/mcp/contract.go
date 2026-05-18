// Package mcp is the runtime contract codegen emits against for
// `mcp_server` and `mcp_client` blocks in `.lzi`. Types only — no I/O
// lives here. Wire of the upstream MCP SDK lives in `server.go` /
// `client.go` (M6 / M7 of bucket-mcp-cycle).
//
// Wire-thin invariant: this file MUST keep zero external imports.
// Doctor `MCP-WIRE-THIN-001` enforces this — see
// docs/proposals/bucket-mcp-cycle.md §"Wire-thin acceptance".
package mcp

import (
	"context"
	"errors"
)

// Transport names the MCP wire format the server speaks or the client
// dials. Closed catalog mirrored by doctor `MCP-TRANSPORT-001`.
type Transport string

const (
	TransportStdio          Transport = "stdio"
	TransportHTTPSSE        Transport = "http_sse"
	TransportHTTPStreamable Transport = "http_streamable"
)

// ToolHandler is the user-authored Go function codegen wires to a
// `tool <name>` block. Args are decoded by the runtime against the
// tool's input schema before the handler sees them.
type ToolHandler func(ctx context.Context, args map[string]any) (any, error)

// ResourceHandler reads a single resource by uri. The handler returns
// the resource bytes + MIME content-type. URI templating
// (`slug://{workspace}/{key}`) is parsed by the SDK; the handler
// receives the resolved URI string.
type ResourceHandler func(ctx context.Context, uri string) (data []byte, mime string, err error)

// PromptHandler produces the message list for a `prompt <name>` block.
// Args follow the prompt's input schema; the runtime renders any
// `template "./path"` reference and exposes the rendered text via the
// returned messages.
type PromptHandler func(ctx context.Context, args map[string]any) (messages []PromptMessage, err error)

// PromptMessage is one rendered message in an MCP prompt response.
// `Role` is the upstream MCP role (`user`, `assistant`, `system`,
// `tool`); `Content` is the rendered text (multipart content is a
// future expansion).
type PromptMessage struct {
	Role    string
	Content string
}

// AuthSpec configures the bearer-auth surface declared by
// `auth bearer env.<NAME>`. Token is the resolved env-var value at
// boot; the codegen never inlines the literal.
type AuthSpec struct {
	Scheme string
	Token  string
}

// ServerMetadata mirrors the MCP server `initialize` response. Name +
// Version are required; Description is optional and surfaces in tool
// catalogs (Claude Desktop, Cursor, etc).
type ServerMetadata struct {
	Name        string
	Version     string
	Description string
}

// ToolRegistration is the per-tool record codegen emits inside a
// `ServerRegistration`.
type ToolRegistration struct {
	Name        string
	Description string
	InputSchema map[string]any
	Handler     ToolHandler
}

// ResourceRegistration is the per-resource record. URITemplate is the
// `uri_template` declared in `.lzi` (e.g. `slug://{workspace}/{key}`).
type ResourceRegistration struct {
	Name        string
	URITemplate string
	MIME        string
	Handler     ResourceHandler
}

// PromptRegistration is the per-prompt record. InputSchema is derived
// from `params` declarations; TemplatePath is the `.tmpl` referenced
// by `template "./prompts/..."`.
type PromptRegistration struct {
	Name         string
	Description  string
	InputSchema  map[string]any
	TemplatePath string
	Handler      PromptHandler
}

// ServerRegistration is the shape codegen emits per `mcp_server` block.
// One ServerRegistration corresponds to one Lazuli `mcp_server <name>`
// declaration; Serve(ctx, reg) wires it to the chosen transport.
type ServerRegistration struct {
	Name      string
	Transport Transport
	Auth      *AuthSpec
	Metadata  ServerMetadata
	Tools     []ToolRegistration
	Resources []ResourceRegistration
	Prompts   []PromptRegistration
}

// ClientImportKind enumerates which MCP surface a client imports
// (closed catalog; doctor `MCP-CLIENT-IMPORT-001` validates).
type ClientImportKind string

const (
	ImportTool     ClientImportKind = "tool"
	ImportResource ClientImportKind = "resource"
	ImportPrompt   ClientImportKind = "prompt"
)

// ClientEndpoint configures how the client dials the upstream MCP
// server. Exactly one of Command (stdio) or URL (http_*) is set; the
// codegen guarantees this.
type ClientEndpoint struct {
	Command string // for transport=stdio: spawn this command
	URL     string // for transport=http_*: dial this URL (already env-resolved)
}

// ClientFailMode controls disposition when the upstream server is
// unreachable. Closed catalog mirrored by doctor
// `MCP-CLIENT-FAIL-001`.
type ClientFailMode string

const (
	FailModeDegrade ClientFailMode = "degrade"
	FailModeFail    ClientFailMode = "fail"
)

// ClientImport is one entry in the client's allow-list. Codegen emits
// one per `imports` row in `.lzi`.
type ClientImport struct {
	Kind        ClientImportKind
	Name        string
	URITemplate string         // ImportResource only
	InputSchema map[string]any // ImportTool / ImportPrompt only (derived from params)
}

// ClientRegistration is the shape codegen emits per `mcp_client` block.
type ClientRegistration struct {
	Name          string
	Transport     Transport
	Endpoint      ClientEndpoint
	Auth          *AuthSpec
	Imports       []ClientImport
	OnUnavailable ClientFailMode
}

// Typed errors. The server.go / client.go wire layers map these to the
// upstream SDK's error envelopes; user code can `errors.Is` against
// them. The set is closed; widening requires a new ErrMCP* constant
// and a doctor update.
var (
	ErrMCPInvalidArgs          = errors.New("mcp: invalid tool arguments")
	ErrMCPHandlerFailed        = errors.New("mcp: handler returned error")
	ErrMCPTransportUnsupported = errors.New("mcp: transport not supported")
	ErrMCPClientUnavailable    = errors.New("mcp: client transport unavailable")
	ErrMCPSchemaMismatch       = errors.New("mcp: tool args fail input schema validation")
	ErrMCPUnknownTool          = errors.New("mcp: tool not registered on server")
	ErrMCPUnknownResource      = errors.New("mcp: resource not registered on server")
	ErrMCPUnknownPrompt        = errors.New("mcp: prompt not registered on server")
)
