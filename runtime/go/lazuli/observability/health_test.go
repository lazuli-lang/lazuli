package observability

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestHealthHandlerHappyPath(t *testing.T) {
	resetHealthState(t)
	RegisterCheck("custom", func(context.Context) error { return nil })

	rec := httptest.NewRecorder()
	HealthHandler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}

	var body struct {
		Status  string `json:"status"`
		Version string `json:"version"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Status != "ok" {
		t.Fatalf("status = %q, want ok", body.Status)
	}
	if body.Version != lazuliVersion {
		t.Fatalf("version = %q, want %q", body.Version, lazuliVersion)
	}
}

func TestHealthHandlerReportsFailingCheck(t *testing.T) {
	resetHealthState(t)
	RegisterCheck("cache", func(context.Context) error {
		return errors.New("cache unavailable")
	})

	rec := httptest.NewRecorder()
	HealthHandler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusServiceUnavailable)
	}

	var body struct {
		Status string            `json:"status"`
		Checks map[string]string `json:"checks"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Status != "unhealthy" {
		t.Fatalf("status = %q, want unhealthy", body.Status)
	}
	if body.Checks["cache"] != "cache unavailable" {
		t.Fatalf("checks[cache] = %q, want cache unavailable", body.Checks["cache"])
	}
}

func TestHealthHandlerTimesOutCheck(t *testing.T) {
	resetHealthState(t)
	healthCheckTimeout = 10 * time.Millisecond
	RegisterCheck("slow", func(ctx context.Context) error {
		<-ctx.Done()
		return ctx.Err()
	})

	rec := httptest.NewRecorder()
	HealthHandler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusServiceUnavailable)
	}

	var body struct {
		Status string            `json:"status"`
		Checks map[string]string `json:"checks"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Status != "unhealthy" {
		t.Fatalf("status = %q, want unhealthy", body.Status)
	}
	if body.Checks["slow"] != context.DeadlineExceeded.Error() {
		t.Fatalf("checks[slow] = %q, want %q", body.Checks["slow"], context.DeadlineExceeded.Error())
	}
}

func resetHealthState(t *testing.T) {
	t.Helper()

	healthRegistry.Lock()
	previousDB := healthRegistry.db
	previousChecks := healthRegistry.checks
	previousTimeout := healthCheckTimeout
	healthRegistry.db = nil
	healthRegistry.checks = make(map[string]healthCheck)
	healthCheckTimeout = 2 * time.Second
	healthRegistry.Unlock()

	t.Cleanup(func() {
		healthRegistry.Lock()
		healthRegistry.db = previousDB
		healthRegistry.checks = previousChecks
		healthCheckTimeout = previousTimeout
		healthRegistry.Unlock()
	})
}
