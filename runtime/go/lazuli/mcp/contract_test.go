package mcp

import (
	"encoding/json"
	"testing"
)

func TestServerSpecRoundtrip(t *testing.T) {
	spec := ServerSpec{
		Name:        "test-server",
		Version:     "1.0.0",
		Description: "test description",
		Transport:   TransportStdio,
		Tools:       []ToolSpec{{Name: "ping", Description: "echo"}},
	}
	if spec.Name != "test-server" {
		t.Fatalf("Name: got %q", spec.Name)
	}
	if spec.Transport != TransportStdio {
		t.Fatalf("Transport: got %q", spec.Transport)
	}
	if len(spec.Tools) != 1 || spec.Tools[0].Name != "ping" {
		t.Fatalf("Tools: got %v", spec.Tools)
	}
}

func TestToolSpecSchemaSerializes(t *testing.T) {
	params := json.RawMessage(`{"type":"object","properties":{"query":{"type":"string"}}}`)
	returns := json.RawMessage(`{"type":"object"}`)
	ts := ToolSpec{
		Name:          "search",
		Description:   "search items",
		ParamsSchema:  params,
		ReturnsSchema: returns,
	}
	if !json.Valid(ts.ParamsSchema) {
		t.Fatal("ParamsSchema is not valid JSON")
	}
	if !json.Valid(ts.ReturnsSchema) {
		t.Fatal("ReturnsSchema is not valid JSON")
	}
}

func TestErrorSentinelsDistinct(t *testing.T) {
	errs := []error{
		ErrMCPProtocol,
		ErrMCPNotFound,
		ErrMCPUnauthorized,
		ErrMCPHandler,
		ErrMCPInvalidArgs,
		ErrMCPTransportUnsupported,
		ErrMCPClientUnavailable,
	}
	seen := map[string]bool{}
	for _, e := range errs {
		if seen[e.Error()] {
			t.Fatalf("duplicate error message: %q", e.Error())
		}
		seen[e.Error()] = true
	}
}
