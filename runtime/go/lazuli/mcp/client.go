// Wire of the upstream `github.com/modelcontextprotocol/go-sdk/mcp`
// package for the client side. Same wire-thin acceptance: single
// external import (the SDK), no stdlib gymnastics.
package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"os/exec"

	sdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

// Client is a thin wrapper around the SDK's ClientSession. Codegen
// emits one Client per `mcp_client` block; user code retrieves it
// from the runtime registry rather than constructing directly.
type Client struct {
	session *sdk.ClientSession
	cancel  context.CancelFunc
	reg     ClientRegistration
}

// Dial connects a ClientRegistration to its upstream MCP server.
// Caller must call Close() when done; recommended in a defer near
// the boot path.
//
// `on_unavailable: degrade` returns a Client with session=nil; calls
// against it return ErrMCPClientUnavailable. `on_unavailable: fail`
// returns a wrapped error from this function.
func Dial(parent context.Context, reg ClientRegistration) (*Client, error) {
	c := &Client{reg: reg}

	transport, err := buildClientTransport(reg)
	if err != nil {
		return c.handleDialErr(err)
	}

	impl := &sdk.Implementation{Name: reg.Name, Version: "0.0.0"}
	sdkClient := sdk.NewClient(impl, nil)

	ctx, cancel := context.WithCancel(parent)
	c.cancel = cancel
	session, err := sdkClient.Connect(ctx, transport, nil)
	if err != nil {
		cancel()
		return c.handleDialErr(fmt.Errorf("%w: %v", ErrMCPClientUnavailable, err))
	}
	c.session = session
	return c, nil
}

func (c *Client) handleDialErr(err error) (*Client, error) {
	switch c.reg.OnUnavailable {
	case FailModeDegrade:
		return c, nil
	case FailModeFail, "":
		return nil, err
	default:
		return nil, fmt.Errorf("%w: unknown on_unavailable %q", ErrMCPClientUnavailable, c.reg.OnUnavailable)
	}
}

// Close terminates the upstream connection. Safe to call on a degraded
// (nil-session) client.
func (c *Client) Close() error {
	if c.cancel != nil {
		c.cancel()
	}
	if c.session == nil {
		return nil
	}
	return c.session.Close()
}

// CallTool invokes a tool on the upstream server. Returns the raw
// text result (the first TextContent block in CallToolResult.Content,
// or "" if the result has no text content). Errors from the upstream
// transport surface as ErrMCPClientUnavailable; tool-level errors
// (IsError=true on the result) surface as ErrMCPHandlerFailed.
func (c *Client) CallTool(ctx context.Context, name string, args map[string]any) (string, error) {
	if c.session == nil {
		return "", fmt.Errorf("%w: %s", ErrMCPClientUnavailable, c.reg.Name)
	}
	if !c.hasImport(ImportTool, name) {
		return "", fmt.Errorf("%w: client %q has not imported tool %q", ErrMCPUnknownTool, c.reg.Name, name)
	}
	result, err := c.session.CallTool(ctx, &sdk.CallToolParams{
		Name:      name,
		Arguments: args,
	})
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrMCPClientUnavailable, err)
	}
	text := firstText(result.Content)
	if result.IsError {
		return text, fmt.Errorf("%w: %s", ErrMCPHandlerFailed, text)
	}
	return text, nil
}

// CallToolJSON is the typed variant: marshals args, unmarshals the
// returned text into `out`. Use when the tool returns JSON-shaped
// data and the caller has a typed struct ready.
func (c *Client) CallToolJSON(ctx context.Context, name string, args map[string]any, out any) error {
	text, err := c.CallTool(ctx, name, args)
	if err != nil {
		return err
	}
	if text == "" {
		return nil
	}
	if err := json.Unmarshal([]byte(text), out); err != nil {
		return fmt.Errorf("%w: %v", ErrMCPSchemaMismatch, err)
	}
	return nil
}

// ReadResource reads a single resource by URI. Returns the resource's
// text (or base64-encoded bytes if the server sent Blob).
func (c *Client) ReadResource(ctx context.Context, uri string) ([]byte, string, error) {
	if c.session == nil {
		return nil, "", fmt.Errorf("%w: %s", ErrMCPClientUnavailable, c.reg.Name)
	}
	result, err := c.session.ReadResource(ctx, &sdk.ReadResourceParams{URI: uri})
	if err != nil {
		return nil, "", fmt.Errorf("%w: %v", ErrMCPClientUnavailable, err)
	}
	if len(result.Contents) == 0 {
		return nil, "", fmt.Errorf("%w: %s", ErrMCPUnknownResource, uri)
	}
	c0 := result.Contents[0]
	if c0.Blob != nil {
		return c0.Blob, c0.MIMEType, nil
	}
	return []byte(c0.Text), c0.MIMEType, nil
}

func (c *Client) hasImport(kind ClientImportKind, name string) bool {
	if len(c.reg.Imports) == 0 {
		return true // no allow-list declared; pass everything through
	}
	for _, imp := range c.reg.Imports {
		if imp.Kind == kind && imp.Name == name {
			return true
		}
	}
	return false
}

func buildClientTransport(reg ClientRegistration) (sdk.Transport, error) {
	switch reg.Transport {
	case TransportStdio:
		if reg.Endpoint.Command == "" {
			return nil, fmt.Errorf("%w: stdio client %q needs Endpoint.Command", ErrMCPInvalidArgs, reg.Name)
		}
		return &sdk.CommandTransport{Command: exec.Command("sh", "-c", reg.Endpoint.Command)}, nil
	case TransportHTTPSSE:
		if reg.Endpoint.URL == "" {
			return nil, fmt.Errorf("%w: http_sse client %q needs Endpoint.URL", ErrMCPInvalidArgs, reg.Name)
		}
		return &sdk.SSEClientTransport{Endpoint: reg.Endpoint.URL}, nil
	case TransportHTTPStreamable:
		if reg.Endpoint.URL == "" {
			return nil, fmt.Errorf("%w: http_streamable client %q needs Endpoint.URL", ErrMCPInvalidArgs, reg.Name)
		}
		return &sdk.StreamableClientTransport{Endpoint: reg.Endpoint.URL}, nil
	default:
		return nil, fmt.Errorf("%w: %q", ErrMCPTransportUnsupported, reg.Transport)
	}
}

func firstText(content []sdk.Content) string {
	for _, c := range content {
		if t, ok := c.(*sdk.TextContent); ok {
			return t.Text
		}
	}
	return ""
}
