package mcp

import (
	"context"
	"errors"
	"testing"
)

func TestTransportConstants(t *testing.T) {
	cases := []struct {
		got, want Transport
	}{
		{TransportStdio, "stdio"},
		{TransportHTTPSSE, "http_sse"},
		{TransportHTTPStreamable, "http_streamable"},
	}
	for _, c := range cases {
		if c.got != c.want {
			t.Errorf("transport literal drift: got %q want %q", c.got, c.want)
		}
	}
}

func TestClientFailModeConstants(t *testing.T) {
	if FailModeDegrade != "degrade" {
		t.Errorf("FailModeDegrade = %q, want degrade", FailModeDegrade)
	}
	if FailModeFail != "fail" {
		t.Errorf("FailModeFail = %q, want fail", FailModeFail)
	}
}

func TestClientImportKindConstants(t *testing.T) {
	cases := []struct {
		got, want ClientImportKind
	}{
		{ImportTool, "tool"},
		{ImportResource, "resource"},
		{ImportPrompt, "prompt"},
	}
	for _, c := range cases {
		if c.got != c.want {
			t.Errorf("import kind drift: got %q want %q", c.got, c.want)
		}
	}
}

func TestTypedErrorsAreDistinct(t *testing.T) {
	errs := []error{
		ErrMCPInvalidArgs,
		ErrMCPHandlerFailed,
		ErrMCPTransportUnsupported,
		ErrMCPClientUnavailable,
		ErrMCPSchemaMismatch,
		ErrMCPUnknownTool,
		ErrMCPUnknownResource,
		ErrMCPUnknownPrompt,
	}
	for i, a := range errs {
		for j, b := range errs {
			if i == j {
				continue
			}
			if errors.Is(a, b) {
				t.Errorf("errors[%d] %q reports Is errors[%d] %q — must be distinct", i, a, j, b)
			}
		}
	}
}

func TestToolHandlerSignature(t *testing.T) {
	// Verify the type alias is callable with the documented shape.
	var h ToolHandler = func(ctx context.Context, args map[string]any) (any, error) {
		if v, ok := args["echo"]; ok {
			return v, nil
		}
		return nil, ErrMCPInvalidArgs
	}
	out, err := h(context.Background(), map[string]any{"echo": "ok"})
	if err != nil {
		t.Fatalf("handler returned err: %v", err)
	}
	if out != "ok" {
		t.Fatalf("handler echo: got %v want ok", out)
	}
	_, err = h(context.Background(), map[string]any{})
	if !errors.Is(err, ErrMCPInvalidArgs) {
		t.Fatalf("missing echo arg: got %v want ErrMCPInvalidArgs", err)
	}
}

func TestServerRegistrationZeroValue(t *testing.T) {
	var s ServerRegistration
	if s.Name != "" || s.Transport != "" || s.Auth != nil {
		t.Errorf("zero ServerRegistration drifted: %#v", s)
	}
	if len(s.Tools) != 0 || len(s.Resources) != 0 || len(s.Prompts) != 0 {
		t.Errorf("zero ServerRegistration slices not nil-empty: %#v", s)
	}
}

func TestClientRegistrationZeroValue(t *testing.T) {
	var c ClientRegistration
	if c.Name != "" || c.Transport != "" || c.OnUnavailable != "" {
		t.Errorf("zero ClientRegistration drifted: %#v", c)
	}
	if c.Endpoint.Command != "" || c.Endpoint.URL != "" {
		t.Errorf("zero ClientEndpoint drifted: %#v", c.Endpoint)
	}
	if len(c.Imports) != 0 {
		t.Errorf("zero ClientRegistration imports not empty: %#v", c)
	}
}

func TestPromptMessageRoundtrip(t *testing.T) {
	m := PromptMessage{Role: "user", Content: "hi"}
	if m.Role != "user" || m.Content != "hi" {
		t.Errorf("PromptMessage literal drift: %#v", m)
	}
}
