package lazuli

import (
	"bytes"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// captureSlog redirects the default slog logger to a buffer for the duration
// of the test and returns the buffer.
func captureSlog(t *testing.T) *bytes.Buffer {
	t.Helper()
	var buf bytes.Buffer
	old := slog.Default()
	slog.SetDefault(slog.New(slog.NewJSONHandler(&buf, nil)))
	t.Cleanup(func() { slog.SetDefault(old) })
	return &buf
}

// fiveHundredError is a typed *Error carrying a wrapped cause + source tag,
// the shape a real command/query 500 produces.
func fiveHundredError() *Error {
	return &Error{
		Status:     http.StatusInternalServerError,
		Code:       CodeInternal,
		Message:    "internal server error",
		MessageKey: CodeInternal,
		Base: ErrorBase{
			Status:  http.StatusInternalServerError,
			Code:    CodeInternal,
			Message: "void_invoice: pq: relation \"invoices\" does not exist",
			Surface: SurfaceAdapterRuntime,
			Feature: "billing",
			Kind:    "command",
			Op:      "void_invoice",
			Source:  "features/billing.lzi:42:1",
			Cause:   errors.New("pq: relation \"invoices\" does not exist"),
		},
	}
}

// TestWriteError5xxAlwaysLogsAndMasksInProd asserts that a 500-producing
// handler logs the structured cause via slog EVEN in prod, while the client
// body stays generic (no message detail leak). Closes W4-5.
func TestWriteError5xxAlwaysLogsAndMasksInProd(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	buf := captureSlog(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/void_invoice", nil)
	req.Header.Set("X-Request-ID", "req-123")

	writeError(rec, req, fiveHundredError())

	// (a) server log carries the structured cause.
	var entry map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(buf.Bytes()), &entry); err != nil {
		t.Fatalf("slog not valid JSON: %v; log=%q", err, buf.String())
	}
	if entry["msg"] != "lazuli: request failed (5xx)" {
		t.Fatalf("slog msg = %v, want lazuli: request failed (5xx)", entry["msg"])
	}
	if entry["cause"] != "pq: relation \"invoices\" does not exist" {
		t.Fatalf("slog cause = %v, want pq error", entry["cause"])
	}
	if entry["op"] != "void_invoice" {
		t.Fatalf("slog op = %v, want void_invoice", entry["op"])
	}
	if entry["feature"] != "billing" {
		t.Fatalf("slog feature = %v, want billing", entry["feature"])
	}
	if entry["request_id"] != "req-123" {
		t.Fatalf("slog request_id = %v, want req-123", entry["request_id"])
	}

	// (b) client body is generic in prod — no detail, no cause leak.
	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("body not JSON: %v; body=%q", err, rec.Body.String())
	}
	if body["code"] != CodeInternal {
		t.Fatalf("body code = %v, want internal", body["code"])
	}
	if _, ok := body["detail"]; ok {
		t.Fatalf("prod body leaked detail block: %q", rec.Body.String())
	}
	if msg, _ := body["message"].(string); strings.Contains(msg, "invoices") {
		t.Fatalf("prod body leaked SQL in message: %q", msg)
	}
}

// TestWriteError5xxDevExposesDetail asserts that in the dev allow-list env the
// 500 envelope carries the cause/source/surface detail block so an agent
// driving the API sees the cause. Closes 05-DX F7.
func TestWriteError5xxDevExposesDetail(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	_ = captureSlog(t) // silence + keep deterministic

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/void_invoice", nil)

	writeError(rec, req, fiveHundredError())

	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("body not JSON: %v; body=%q", err, rec.Body.String())
	}
	detail, ok := body["detail"].(map[string]any)
	if !ok {
		t.Fatalf("dev body missing detail block: %q", rec.Body.String())
	}
	if detail["cause"] != "pq: relation \"invoices\" does not exist" {
		t.Fatalf("dev detail cause = %v, want pq error", detail["cause"])
	}
	if detail["op"] != "void_invoice" {
		t.Fatalf("dev detail op = %v, want void_invoice", detail["op"])
	}
	if detail["surface"] != SurfaceAdapterRuntime.String() {
		t.Fatalf("dev detail surface = %v, want %s", detail["surface"], SurfaceAdapterRuntime.String())
	}
	if detail["source"] != "features/billing.lzi:42:1" {
		t.Fatalf("dev detail source = %v, want .lzi loc", detail["source"])
	}
}

// TestWriteErrorUnmappedLogsAndDevDetail asserts the unmapped (non-*Error)
// path also logs the cause and exposes it in dev — the raw error becomes
// Base.Cause.
func TestWriteErrorUnmappedLogsAndDevDetail(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	buf := captureSlog(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/q/request_billing_summary", nil)

	writeError(rec, req, errors.New("boom: connection refused"))

	var entry map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(buf.Bytes()), &entry); err != nil {
		t.Fatalf("slog not JSON: %v; log=%q", err, buf.String())
	}
	if entry["cause"] != "boom: connection refused" {
		t.Fatalf("slog cause = %v, want boom...", entry["cause"])
	}

	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("body not JSON: %v", err)
	}
	detail, ok := body["detail"].(map[string]any)
	if !ok {
		t.Fatalf("dev unmapped body missing detail: %q", rec.Body.String())
	}
	if detail["cause"] != "boom: connection refused" {
		t.Fatalf("dev unmapped detail cause = %v", detail["cause"])
	}
}

// TestNon5xxNotLoggedAsServerError asserts a 4xx typed error does NOT emit the
// 5xx server-error slog line (we only log server faults, not client faults).
func TestNon5xxNotLoggedAsServerError(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	buf := captureSlog(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/create", nil)

	writeError(rec, req, &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
		Message: "bad request", MessageKey: CodeBadRequest})

	if strings.Contains(buf.String(), "request failed (5xx)") {
		t.Fatalf("4xx wrongly logged as 5xx server error: %q", buf.String())
	}
}

// TestRuntimeOrigin500DefaultsToNonUserDSLSurface is the ERR-SURFACE-DEFAULT-USERDSL
// regression. A runtime-origin 500 — e.g. "validator not registered",
// "transition requires transaction", a nil DB — is constructed at dozens of
// sites as `{Status:500, Code:CodeInternal}` with NO Base.Surface, so the
// field defaults to the zero value `SurfaceUserDSL`. That mis-routes a Lazuli
// runtime/adapter fault to user-DSL (.lzi) debugging. After the fix the HTTP
// boundary promotes the zero value to a runtime-origin surface for any 5xx.
func TestRuntimeOrigin500DefaultsToNonUserDSLSurface(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	_ = captureSlog(t)

	// The exact shape produced by the many internal 500 sites (handle.go,
	// run.go, binding_fn.go, …): no Base, so Base.Surface == SurfaceUserDSL.
	runtimeFault := &Error{
		Status:  http.StatusInternalServerError,
		Code:    CodeInternal,
		Message: "validator not registered: billing.assert_balance",
	}
	if runtimeFault.Base.Surface != SurfaceUserDSL {
		t.Fatalf("precondition: zero-value Base.Surface should be SurfaceUserDSL, got %s",
			runtimeFault.Base.Surface)
	}

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/charge", nil)
	writeError(rec, req, runtimeFault)

	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("body not JSON: %v; body=%q", err, rec.Body.String())
	}
	detail, ok := body["detail"].(map[string]any)
	if !ok {
		t.Fatalf("dev body missing detail block: %q", rec.Body.String())
	}
	surface, _ := detail["surface"].(string)
	if surface == SurfaceUserDSL.String() {
		t.Fatalf("runtime-origin 500 mis-attributed to %q surface; a non-DB runtime fault "+
			"must NOT route to user-DSL debugging", surface)
	}
	if surface != SurfaceLibInternal.String() {
		t.Fatalf("runtime-origin 500 surface = %q, want %q", surface, SurfaceLibInternal.String())
	}
}

// TestExplicitSurfacePreservedOn500 asserts the boundary's surface-promotion
// only fills the ZERO value — an explicitly-attributed surface (e.g. the
// DB-origin adapter_runtime set by classifyDBError) is never overwritten.
func TestExplicitSurfacePreservedOn500(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	_ = captureSlog(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/void_invoice", nil)
	writeError(rec, req, fiveHundredError()) // carries SurfaceAdapterRuntime

	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("body not JSON: %v", err)
	}
	detail, _ := body["detail"].(map[string]any)
	if detail["surface"] != SurfaceAdapterRuntime.String() {
		t.Fatalf("explicit surface overwritten: got %v, want %s",
			detail["surface"], SurfaceAdapterRuntime.String())
	}
}

// TestPanicMiddlewareDevDetail asserts the recover path surfaces the panic +
// stack in the dev detail block, and returns the JSON envelope.
func TestPanicMiddlewareDevDetail(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	_ = captureSlog(t)

	handler := RecoverMiddleware(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic("kaboom-dev")
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/create", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500", rec.Code)
	}
	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("panic body not JSON: %v; body=%q", err, rec.Body.String())
	}
	if body["code"] != CodeInternal {
		t.Fatalf("panic body code = %v, want internal", body["code"])
	}
	detail, ok := body["detail"].(map[string]any)
	if !ok {
		t.Fatalf("dev panic body missing detail: %q", rec.Body.String())
	}
	if detail["panic"] != "kaboom-dev" {
		t.Fatalf("dev panic detail = %v, want kaboom-dev", detail["panic"])
	}
	if s, _ := detail["stack"].(string); !strings.Contains(s, "TestPanicMiddlewareDevDetail") {
		t.Fatalf("dev panic stack missing test frame: %v", detail["stack"])
	}
}
