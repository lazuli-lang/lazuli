// Wire of the upstream `github.com/modelcontextprotocol/go-sdk/mcp`
// package for the server side. One file, one external import beyond
// stdlib (the SDK), exactly as the wire-thin acceptance specifies in
// docs/proposals/bucket-mcp-cycle.md §"Wire-thin acceptance".
package mcp

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"

	sdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

// Serve binds a ServerRegistration to the upstream SDK and starts the
// chosen transport. Returns when the transport exits or ctx is
// cancelled. For HTTP transports, callers must attach the returned
// http.Handler to their own server — Serve only wires stdio inline.
//
// The codegen emits `mcp.Serve(ctx, reg)` in the generated mcp_server
// init path; user code never imports this directly.
func Serve(ctx context.Context, reg ServerRegistration) error {
	server, err := buildServer(reg)
	if err != nil {
		return err
	}
	switch reg.Transport {
	case TransportStdio:
		return server.Run(ctx, &sdk.StdioTransport{})
	case TransportHTTPSSE, TransportHTTPStreamable:
		return fmt.Errorf("%w: transport %q requires HTTPHandler() not Serve()", ErrMCPTransportUnsupported, reg.Transport)
	default:
		return fmt.Errorf("%w: %q", ErrMCPTransportUnsupported, reg.Transport)
	}
}

// HTTPHandler returns an http.Handler that hosts the server over the
// chosen HTTP transport (sse or streamable). Returns
// ErrMCPTransportUnsupported when the registration's transport is not
// an HTTP variant.
func HTTPHandler(reg ServerRegistration) (http.Handler, error) {
	server, err := buildServer(reg)
	if err != nil {
		return nil, err
	}
	getServer := func(*http.Request) *sdk.Server { return server }
	switch reg.Transport {
	case TransportHTTPSSE:
		return sdk.NewSSEHandler(getServer, nil), nil
	case TransportHTTPStreamable:
		return sdk.NewStreamableHTTPHandler(getServer, nil), nil
	case TransportStdio:
		return nil, fmt.Errorf("%w: stdio is not an http transport", ErrMCPTransportUnsupported)
	default:
		return nil, fmt.Errorf("%w: %q", ErrMCPTransportUnsupported, reg.Transport)
	}
}

// buildServer materializes the SDK server from the registration. The
// auth field is informational only at this level — the SDK does not
// gate stdio transports; HTTP variants must apply bearer middleware
// at the http.Handler layer.
func buildServer(reg ServerRegistration) (*sdk.Server, error) {
	if reg.Name == "" {
		return nil, fmt.Errorf("%w: ServerRegistration.Name is empty", ErrMCPInvalidArgs)
	}
	impl := &sdk.Implementation{
		Name:    reg.Metadata.Name,
		Version: reg.Metadata.Version,
		Title:   reg.Metadata.Description,
	}
	server := sdk.NewServer(impl, nil)

	for _, t := range reg.Tools {
		tool := &sdk.Tool{
			Name:        t.Name,
			Description: t.Description,
			InputSchema: t.InputSchema,
		}
		server.AddTool(tool, wireToolHandler(t.Handler))
	}
	for _, r := range reg.Resources {
		if r.URITemplate == "" {
			return nil, fmt.Errorf("%w: resource %q has empty URITemplate", ErrMCPInvalidArgs, r.Name)
		}
		template := &sdk.ResourceTemplate{
			Name:        r.Name,
			URITemplate: r.URITemplate,
			MIMEType:    r.MIME,
		}
		server.AddResourceTemplate(template, wireResourceHandler(r.Handler))
	}
	for _, p := range reg.Prompts {
		prompt := &sdk.Prompt{
			Name:        p.Name,
			Description: p.Description,
		}
		server.AddPrompt(prompt, wirePromptHandler(p.Handler))
	}
	return server, nil
}

func wireToolHandler(h ToolHandler) sdk.ToolHandler {
	return func(ctx context.Context, req *sdk.CallToolRequest) (*sdk.CallToolResult, error) {
		args, err := decodeArgs(req.Params.Arguments)
		if err != nil {
			return mcpErrorResult(err), nil
		}
		out, err := h(ctx, args)
		if err != nil {
			return mcpErrorResult(err), nil
		}
		return mcpTextResult(out), nil
	}
}

func wireResourceHandler(h ResourceHandler) sdk.ResourceHandler {
	return func(ctx context.Context, req *sdk.ReadResourceRequest) (*sdk.ReadResourceResult, error) {
		uri := req.Params.URI
		data, mime, err := h(ctx, uri)
		if err != nil {
			if errors.Is(err, ErrMCPUnknownResource) {
				return nil, sdk.ResourceNotFoundError(uri)
			}
			return nil, fmt.Errorf("%w: %v", ErrMCPHandlerFailed, err)
		}
		if mime == "" {
			mime = "application/octet-stream"
		}
		return &sdk.ReadResourceResult{
			Contents: []*sdk.ResourceContents{
				{URI: uri, MIMEType: mime, Blob: data},
			},
		}, nil
	}
}

func wirePromptHandler(h PromptHandler) sdk.PromptHandler {
	return func(ctx context.Context, req *sdk.GetPromptRequest) (*sdk.GetPromptResult, error) {
		args := decodePromptArgs(req.Params.Arguments)
		msgs, err := h(ctx, args)
		if err != nil {
			return nil, fmt.Errorf("%w: %v", ErrMCPHandlerFailed, err)
		}
		out := make([]*sdk.PromptMessage, 0, len(msgs))
		for _, m := range msgs {
			out = append(out, &sdk.PromptMessage{
				Role:    sdk.Role(m.Role),
				Content: &sdk.TextContent{Text: m.Content},
			})
		}
		return &sdk.GetPromptResult{Messages: out}, nil
	}
}

func decodeArgs(raw json.RawMessage) (map[string]any, error) {
	if len(raw) == 0 {
		return map[string]any{}, nil
	}
	out := map[string]any{}
	if err := json.Unmarshal(raw, &out); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrMCPInvalidArgs, err)
	}
	return out, nil
}

func decodePromptArgs(raw map[string]string) map[string]any {
	out := make(map[string]any, len(raw))
	for k, v := range raw {
		out[k] = v
	}
	return out
}

func mcpTextResult(out any) *sdk.CallToolResult {
	var text string
	switch v := out.(type) {
	case nil:
		text = ""
	case string:
		text = v
	case []byte:
		text = string(v)
	default:
		if b, err := json.Marshal(out); err == nil {
			text = string(b)
		} else {
			text = fmt.Sprintf("%v", out)
		}
	}
	return &sdk.CallToolResult{
		Content: []sdk.Content{&sdk.TextContent{Text: text}},
	}
}

func mcpErrorResult(err error) *sdk.CallToolResult {
	return &sdk.CallToolResult{
		IsError: true,
		Content: []sdk.Content{&sdk.TextContent{Text: err.Error()}},
	}
}
