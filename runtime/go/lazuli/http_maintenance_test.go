package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestMaintenanceModeMiddlewareRejectsEnabledRequests(t *testing.T) {
	called := false
	handler := MaintenanceModeMiddleware(MaintenanceModeConfig{
		Enabled:    true,
		RetryAfter: 90 * time.Second,
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/orders", nil))

	if called {
		t.Fatal("next handler was called")
	}
	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusServiceUnavailable)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}
	if got := rec.Header().Get("Retry-After"); got != "90" {
		t.Fatalf("Retry-After = %q, want 90", got)
	}

	body := decodeProblemResponse(t, rec)
	if body["status"] != float64(http.StatusServiceUnavailable) {
		t.Fatalf("status body = %v, want %d", body["status"], http.StatusServiceUnavailable)
	}
	if body["title"] != http.StatusText(http.StatusServiceUnavailable) {
		t.Fatalf("title = %v, want %s", body["title"], http.StatusText(http.StatusServiceUnavailable))
	}
	if body["detail"] != defaultMaintenanceDetail {
		t.Fatalf("detail = %v, want %s", body["detail"], defaultMaintenanceDetail)
	}
	if body["code"] != CodeMaintenance {
		t.Fatalf("code = %v, want %s", body["code"], CodeMaintenance)
	}
}

func TestMaintenanceModeMiddlewareDisabledPassesThrough(t *testing.T) {
	called := false
	handler := MaintenanceModeMiddleware(MaintenanceModeConfig{
		Enabled:    false,
		RetryAfter: time.Minute,
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		called = true
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/orders", nil))

	if !called {
		t.Fatal("next handler was not called")
	}
	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if got := rec.Header().Get("Retry-After"); got != "" {
		t.Fatalf("Retry-After = %q, want empty", got)
	}
}

func TestMaintenanceModeMiddlewareBypassPathPassesThrough(t *testing.T) {
	called := false
	handler := MaintenanceModeMiddleware(MaintenanceModeConfig{
		Enabled:     true,
		BypassPaths: []string{"/healthz"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		called = true
		_, _ = w.Write([]byte("ok"))
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	if !called {
		t.Fatal("next handler was not called")
	}
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Body.String(); got != "ok" {
		t.Fatalf("body = %q, want ok", got)
	}
}

func TestMaintenanceModeMiddlewareDynamicProvider(t *testing.T) {
	enabled := false
	providerCalls := 0
	nextCalls := 0
	handler := MaintenanceModeMiddleware(MaintenanceModeConfig{
		Enabled: true,
		EnabledProvider: func(*http.Request) bool {
			providerCalls++
			return enabled
		},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		nextCalls++
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/orders", nil))
	if rec.Code != http.StatusNoContent {
		t.Fatalf("disabled provider status = %d, want %d", rec.Code, http.StatusNoContent)
	}

	enabled = true
	rec = httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/orders", nil))
	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("enabled provider status = %d, want %d", rec.Code, http.StatusServiceUnavailable)
	}

	if providerCalls != 2 {
		t.Fatalf("provider calls = %d, want 2", providerCalls)
	}
	if nextCalls != 1 {
		t.Fatalf("next calls = %d, want 1", nextCalls)
	}
}
