package observability

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRegisterPprofDefaultPrefix(t *testing.T) {
	mux := http.NewServeMux()
	RegisterPprof(mux, "")

	assertPprofPattern(t, mux, "/debug/pprof/", "GET /debug/pprof/")
	assertPprofPattern(t, mux, "/debug/pprof/cmdline", "GET /debug/pprof/cmdline")
	assertPprofPattern(t, mux, "/debug/pprof/profile", "GET /debug/pprof/profile")
	assertPprofPattern(t, mux, "/debug/pprof/symbol", "GET /debug/pprof/symbol")
	assertPprofPattern(t, mux, "/debug/pprof/trace", "GET /debug/pprof/trace")
	assertPprofPattern(t, mux, "/debug/pprof/goroutine", "GET /debug/pprof/")

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/debug/pprof/", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("index status = %d, want %d", rec.Code, http.StatusOK)
	}
	if !strings.Contains(rec.Body.String(), "Types of profiles available") {
		t.Fatalf("index body does not list profiles")
	}

	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/debug/pprof/goroutine?debug=1", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("named profile status = %d, want %d", rec.Code, http.StatusOK)
	}
	if !strings.Contains(rec.Body.String(), "goroutine profile:") {
		t.Fatalf("named profile body does not contain goroutine profile output")
	}
}

func TestRegisterPprofCustomPrefix(t *testing.T) {
	mux := http.NewServeMux()
	RegisterPprof(mux, "/internal/pprof/")

	assertPprofPattern(t, mux, "/internal/pprof/", "GET /internal/pprof/")
	assertPprofPattern(t, mux, "/internal/pprof/cmdline", "GET /internal/pprof/cmdline")
	assertPprofPattern(t, mux, "/internal/pprof/profile", "GET /internal/pprof/profile")
	assertPprofPattern(t, mux, "/internal/pprof/symbol", "GET /internal/pprof/symbol")
	assertPprofPattern(t, mux, "/internal/pprof/trace", "GET /internal/pprof/trace")
	assertPprofPattern(t, mux, "/internal/pprof/heap", "GET /internal/pprof/")

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/internal/pprof/cmdline", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("cmdline status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.Len() == 0 {
		t.Fatalf("cmdline body is empty")
	}

	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/internal/pprof/heap?debug=1", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("custom named profile status = %d, want %d", rec.Code, http.StatusOK)
	}
	if !strings.Contains(rec.Body.String(), "heap profile:") {
		t.Fatalf("custom named profile body does not contain heap profile output")
	}
}

func TestRegisterPprofNormalizesPrefix(t *testing.T) {
	mux := http.NewServeMux()
	RegisterPprof(mux, "ops/pprof/")

	assertPprofPattern(t, mux, "/ops/pprof/cmdline", "GET /ops/pprof/cmdline")
}

func TestRegisterPprofUnknownProfile(t *testing.T) {
	mux := http.NewServeMux()
	RegisterPprof(mux, "/internal/pprof")

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/internal/pprof/not-a-profile", nil))
	if rec.Code != http.StatusNotFound {
		t.Fatalf("unknown profile status = %d, want %d", rec.Code, http.StatusNotFound)
	}
	if !strings.Contains(rec.Body.String(), "Unknown profile") {
		t.Fatalf("unknown profile body = %q, want Unknown profile", rec.Body.String())
	}
}

func assertPprofPattern(t *testing.T, mux *http.ServeMux, target, want string) {
	t.Helper()

	req := httptest.NewRequest(http.MethodGet, target, nil)
	_, pattern := mux.Handler(req)
	if pattern != want {
		t.Fatalf("pattern for %s = %q, want %q", target, pattern, want)
	}
}
