package probe

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestReadinessTogglesReadyAndDraining(t *testing.T) {
	readiness := NewReadiness()

	rec := httptest.NewRecorder()
	readiness.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))
	assertReadinessResponse(t, rec, http.StatusServiceUnavailable, "unready")

	readiness.MarkReady()

	rec = httptest.NewRecorder()
	readiness.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))
	assertReadinessResponse(t, rec, http.StatusOK, "ready")

	readiness.MarkDraining()

	rec = httptest.NewRecorder()
	readiness.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))
	assertReadinessResponse(t, rec, http.StatusServiceUnavailable, "unready")
}

func TestReadinessHandlerJSONShape(t *testing.T) {
	readiness := NewReadiness()
	readiness.MarkReady()

	rec := httptest.NewRecorder()
	readiness.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var body map[string]string
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if len(body) != 1 {
		t.Fatalf("response fields = %v, want only status", body)
	}
	if body["status"] != "ready" {
		t.Fatalf("response status = %q, want ready", body["status"])
	}
}

func assertReadinessResponse(t *testing.T, rec *httptest.ResponseRecorder, wantStatus int, wantBodyStatus string) {
	t.Helper()

	if rec.Code != wantStatus {
		t.Fatalf("status = %d, want %d", rec.Code, wantStatus)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var body struct {
		Status string `json:"status"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Status != wantBodyStatus {
		t.Fatalf("response status = %q, want %q", body.Status, wantBodyStatus)
	}
}
