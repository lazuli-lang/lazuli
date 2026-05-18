package mcp

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	sdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

// TestServerToolRoundtrip exercises a full propose-call-result roundtrip
// over an in-memory transport pair. Acts as the wire-thin smoke test
// the bucket-mcp-cycle §M10 specifies (in-memory pair, one tool call,
// envelope mapping verified).
func TestServerToolRoundtrip(t *testing.T) {
	reg := ServerRegistration{
		Name:      "smoke",
		Transport: TransportStdio,
		Metadata: ServerMetadata{
			Name:    "lazuli-mcp-smoke",
			Version: "0.0.0",
		},
		Tools: []ToolRegistration{
			{
				Name:        "echo",
				Description: "echoes args back",
				InputSchema: map[string]any{
					"type": "object",
					"properties": map[string]any{
						"text": map[string]any{"type": "string"},
					},
					"required": []any{"text"},
				},
				Handler: func(_ context.Context, args map[string]any) (any, error) {
					txt, _ := args["text"].(string)
					return map[string]any{"reply": "echo: " + txt}, nil
				},
			},
		},
	}

	server, err := buildServer(reg)
	if err != nil {
		t.Fatalf("buildServer: %v", err)
	}

	clientTransport, serverTransport := sdk.NewInMemoryTransports()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	serverDone := make(chan error, 1)
	go func() {
		serverDone <- server.Run(ctx, serverTransport)
	}()

	impl := &sdk.Implementation{Name: "smoke-client", Version: "0.0.0"}
	client := sdk.NewClient(impl, nil)
	cs, err := client.Connect(ctx, clientTransport, nil)
	if err != nil {
		t.Fatalf("client connect: %v", err)
	}
	defer cs.Close()

	result, err := cs.CallTool(ctx, &sdk.CallToolParams{
		Name:      "echo",
		Arguments: map[string]any{"text": "hello"},
	})
	if err != nil {
		t.Fatalf("CallTool: %v", err)
	}
	if result.IsError {
		t.Fatalf("CallTool reported IsError; content: %#v", result.Content)
	}
	text := ""
	for _, c := range result.Content {
		if tc, ok := c.(*sdk.TextContent); ok {
			text = tc.Text
			break
		}
	}
	if text == "" {
		t.Fatalf("no text content returned; got %#v", result.Content)
	}
	var decoded map[string]any
	if err := json.Unmarshal([]byte(text), &decoded); err != nil {
		t.Fatalf("decode result text: %v (text=%q)", err, text)
	}
	if reply, _ := decoded["reply"].(string); reply != "echo: hello" {
		t.Fatalf("reply mismatch: got %q want %q", reply, "echo: hello")
	}
}

// TestServerHandlerError ensures user-returned errors land as
// IsError=true on the CallToolResult, with the error message
// surfaced as text content.
func TestServerHandlerError(t *testing.T) {
	reg := ServerRegistration{
		Name:      "errsmoke",
		Transport: TransportStdio,
		Metadata: ServerMetadata{
			Name:    "lazuli-mcp-err",
			Version: "0.0.0",
		},
		Tools: []ToolRegistration{
			{
				Name:        "boom",
				Description: "always errors",
				InputSchema: map[string]any{"type": "object"},
				Handler: func(_ context.Context, _ map[string]any) (any, error) {
					return nil, errors.New("boom: simulated handler failure")
				},
			},
		},
	}

	server, err := buildServer(reg)
	if err != nil {
		t.Fatalf("buildServer: %v", err)
	}

	clientTransport, serverTransport := sdk.NewInMemoryTransports()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go server.Run(ctx, serverTransport)

	impl := &sdk.Implementation{Name: "err-client", Version: "0.0.0"}
	client := sdk.NewClient(impl, nil)
	cs, err := client.Connect(ctx, clientTransport, nil)
	if err != nil {
		t.Fatalf("client connect: %v", err)
	}
	defer cs.Close()

	result, err := cs.CallTool(ctx, &sdk.CallToolParams{Name: "boom"})
	if err != nil {
		t.Fatalf("CallTool transport error: %v", err)
	}
	if !result.IsError {
		t.Fatalf("expected IsError=true, got false")
	}
	got := ""
	for _, c := range result.Content {
		if tc, ok := c.(*sdk.TextContent); ok {
			got = tc.Text
			break
		}
	}
	if !strings.Contains(got, "boom: simulated handler failure") {
		t.Fatalf("error text missing: got %q", got)
	}
}

// TestServerEmptyNameRejected verifies the wire-thin validation in
// buildServer rejects an unnamed registration up front.
func TestServerEmptyNameRejected(t *testing.T) {
	_, err := buildServer(ServerRegistration{})
	if !errors.Is(err, ErrMCPInvalidArgs) {
		t.Fatalf("expected ErrMCPInvalidArgs, got %v", err)
	}
}

// TestServeUnsupportedTransport verifies the Serve() entry point
// rejects HTTP transports with the documented error sentinel.
func TestServeUnsupportedTransport(t *testing.T) {
	reg := ServerRegistration{
		Name:      "x",
		Transport: TransportHTTPSSE,
		Metadata:  ServerMetadata{Name: "x", Version: "0.0.0"},
	}
	err := Serve(context.Background(), reg)
	if !errors.Is(err, ErrMCPTransportUnsupported) {
		t.Fatalf("expected ErrMCPTransportUnsupported, got %v", err)
	}
}

// TestHTTPHandlerSupported verifies HTTPHandler() returns a non-nil
// handler for both HTTP transport variants.
func TestHTTPHandlerSupported(t *testing.T) {
	for _, transport := range []Transport{TransportHTTPSSE, TransportHTTPStreamable} {
		reg := ServerRegistration{
			Name:      "x",
			Transport: transport,
			Metadata:  ServerMetadata{Name: "x", Version: "0.0.0"},
		}
		h, err := HTTPHandler(reg)
		if err != nil {
			t.Fatalf("HTTPHandler(%s) err: %v", transport, err)
		}
		if h == nil {
			t.Fatalf("HTTPHandler(%s) returned nil handler", transport)
		}
	}
}

// TestHTTPHandlerRejectsStdio verifies HTTPHandler() refuses stdio
// transports (caller should use Serve() for stdio).
func TestHTTPHandlerRejectsStdio(t *testing.T) {
	reg := ServerRegistration{
		Name:      "x",
		Transport: TransportStdio,
		Metadata:  ServerMetadata{Name: "x", Version: "0.0.0"},
	}
	_, err := HTTPHandler(reg)
	if !errors.Is(err, ErrMCPTransportUnsupported) {
		t.Fatalf("expected ErrMCPTransportUnsupported, got %v", err)
	}
}
