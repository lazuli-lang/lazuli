package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestCorsMiddleware_NoContract_Passthrough(t *testing.T) {
	currentCors.Store(nil)
	called := false
	next := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		called = true
		w.WriteHeader(http.StatusOK)
	})

	req := httptest.NewRequest("GET", "/foo", nil)
	rec := httptest.NewRecorder()
	CorsMiddleware(next).ServeHTTP(rec, req)

	if !called {
		t.Fatalf("expected passthrough to invoke next handler")
	}
	if got := rec.Header().Get("Access-Control-Allow-Origin"); got != "" {
		t.Fatalf("expected no CORS header without contract, got %q", got)
	}
}

func TestCorsMiddleware_SimpleRequest_AddsHeader(t *testing.T) {
	t.Setenv("LAZULI_ENV", "local")
	SetCorsContract(&AppCors{
		AllowOrigins: map[string][]string{
			"local": {"http://localhost:5173"},
		},
		AllowCredentials: true,
		MaxAge:           3600,
	})
	t.Cleanup(func() { currentCors.Store(nil) })

	req := httptest.NewRequest("POST", "/api/v1/c/x", nil)
	req.Header.Set("Origin", "http://localhost:5173")
	rec := httptest.NewRecorder()
	CorsMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})).ServeHTTP(rec, req)

	if got, want := rec.Header().Get("Access-Control-Allow-Origin"), "http://localhost:5173"; got != want {
		t.Fatalf("Allow-Origin = %q want %q", got, want)
	}
	if got, want := rec.Header().Get("Access-Control-Allow-Credentials"), "true"; got != want {
		t.Fatalf("Allow-Credentials = %q want %q", got, want)
	}
}

func TestCorsMiddleware_Preflight_ReturnsNoContent(t *testing.T) {
	t.Setenv("LAZULI_ENV", "local")
	SetCorsContract(&AppCors{
		AllowOrigins: map[string][]string{
			"local": {"http://localhost:5173"},
		},
		AllowCredentials: true,
		MaxAge:           3600,
	})
	t.Cleanup(func() { currentCors.Store(nil) })

	req := httptest.NewRequest("OPTIONS", "/api/v1/c/x", nil)
	req.Header.Set("Origin", "http://localhost:5173")
	req.Header.Set("Access-Control-Request-Method", "POST")
	req.Header.Set("Access-Control-Request-Headers", "content-type")
	rec := httptest.NewRecorder()
	CorsMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		t.Fatalf("preflight must NOT invoke downstream handler")
	})).ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("preflight status = %d want 204", rec.Code)
	}
	if got, want := rec.Header().Get("Access-Control-Allow-Origin"), "http://localhost:5173"; got != want {
		t.Fatalf("preflight Allow-Origin = %q want %q", got, want)
	}
	if rec.Header().Get("Access-Control-Allow-Methods") == "" {
		t.Fatalf("preflight must set Allow-Methods")
	}
	if got, want := rec.Header().Get("Access-Control-Max-Age"), "3600"; got != want {
		t.Fatalf("preflight Max-Age = %q want %q", got, want)
	}
}

func TestCorsMiddleware_DisallowedOrigin_NoHeader(t *testing.T) {
	t.Setenv("LAZULI_ENV", "local")
	SetCorsContract(&AppCors{
		AllowOrigins: map[string][]string{
			"local": {"http://localhost:5173"},
		},
	})
	t.Cleanup(func() { currentCors.Store(nil) })

	req := httptest.NewRequest("POST", "/api/v1/c/x", nil)
	req.Header.Set("Origin", "http://evil.example.com")
	rec := httptest.NewRecorder()
	CorsMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})).ServeHTTP(rec, req)

	if got := rec.Header().Get("Access-Control-Allow-Origin"); got != "" {
		t.Fatalf("disallowed origin must not get Allow-Origin, got %q", got)
	}
}
