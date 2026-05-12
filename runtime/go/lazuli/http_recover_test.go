package lazuli

import (
	"bytes"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRecoverMiddlewareRecoversPanicAndReturns500(t *testing.T) {
	var logBuf bytes.Buffer
	oldLogger := slog.Default()
	slog.SetDefault(slog.New(slog.NewJSONHandler(&logBuf, nil)))
	t.Cleanup(func() {
		slog.SetDefault(oldLogger)
	})

	handler := RecoverMiddleware(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic("boom")
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/create", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusInternalServerError)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}
	body := decodeProblemResponse(t, rec)
	if body["status"] != float64(http.StatusInternalServerError) {
		t.Fatalf("body status = %v, want %d", body["status"], http.StatusInternalServerError)
	}
	if body["detail"] != "internal server error" {
		t.Fatalf("body detail = %v, want internal server error", body["detail"])
	}
	if body["code"] != CodeInternal {
		t.Fatalf("body code = %v, want %s", body["code"], CodeInternal)
	}

	var entry map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(logBuf.Bytes()), &entry); err != nil {
		t.Fatalf("log JSON decode error: %v; log = %q", err, logBuf.String())
	}
	if entry["msg"] != "lazuli: panic in handler" {
		t.Fatalf("log msg = %v, want lazuli: panic in handler", entry["msg"])
	}
	if entry["panic"] != "boom" {
		t.Fatalf("log panic = %v, want boom", entry["panic"])
	}
	if entry["method"] != http.MethodPost {
		t.Fatalf("log method = %v, want %s", entry["method"], http.MethodPost)
	}
	if entry["path"] != "/api/v1/c/create" {
		t.Fatalf("log path = %v, want /api/v1/c/create", entry["path"])
	}
	stack, ok := entry["stack"].(string)
	if !ok || !strings.Contains(stack, "TestRecoverMiddlewareRecoversPanicAndReturns500") {
		t.Fatalf("log stack = %v, want test stack trace", entry["stack"])
	}
}

func TestRecoverMiddlewareHappyPathUnaffected(t *testing.T) {
	handler := RecoverMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("X-Lazuli-Test", "ok")
		w.WriteHeader(http.StatusAccepted)
		_, _ = w.Write([]byte("accepted"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/v1/q/list", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if got := rec.Header().Get("X-Lazuli-Test"); got != "ok" {
		t.Fatalf("X-Lazuli-Test = %q, want ok", got)
	}
	if got := rec.Body.String(); got != "accepted" {
		t.Fatalf("body = %q, want accepted", got)
	}
}
