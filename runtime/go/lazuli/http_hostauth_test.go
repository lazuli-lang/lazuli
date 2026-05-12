package lazuli

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestHostAuthorizationMiddlewareAllowsExactHost(t *testing.T) {
	called := false
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"example.com"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		called = true
		w.WriteHeader(http.StatusAccepted)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://EXAMPLE.com/", nil)

	handler.ServeHTTP(rec, req)

	if !called {
		t.Fatal("next handler was not called")
	}
	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
}

func TestHostAuthorizationMiddlewareRejectsUnknownHost(t *testing.T) {
	called := false
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"example.com"},
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://evil.example/", nil)

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("next handler was called")
	}
	if rec.Code != http.StatusMisdirectedRequest {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMisdirectedRequest)
	}
	if got := rec.Result().Header.Get("Content-Type"); !strings.HasPrefix(got, "text/plain") {
		t.Fatalf("Content-Type = %q, want text/plain", got)
	}
}

func TestHostAuthorizationMiddlewareAllowsWildcardSuffix(t *testing.T) {
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"*.example.com"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://api.example.com/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
}

func TestHostAuthorizationMiddlewareWildcardDoesNotAllowApex(t *testing.T) {
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"*.example.com"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://example.com/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusMisdirectedRequest {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMisdirectedRequest)
	}
}

func TestHostAuthorizationMiddlewareRejectsMalformedHost(t *testing.T) {
	tests := []string{
		"*.example.com",
		"evil.example,api.example.com",
	}

	for _, host := range tests {
		t.Run(host, func(t *testing.T) {
			handler := HostAuthorizationMiddleware(HostAuthorization{
				AllowedHosts: []string{"*.example.com"},
			})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				w.WriteHeader(http.StatusNoContent)
			}))

			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, "http://example.com/", nil)
			req.Host = host

			handler.ServeHTTP(rec, req)

			if rec.Code != http.StatusMisdirectedRequest {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusMisdirectedRequest)
			}
		})
	}
}

func TestHostAuthorizationMiddlewareNormalizePortOptIn(t *testing.T) {
	tests := []struct {
		name          string
		normalizePort bool
		wantStatus    int
	}{
		{name: "disabled", normalizePort: false, wantStatus: http.StatusMisdirectedRequest},
		{name: "enabled", normalizePort: true, wantStatus: http.StatusNoContent},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := HostAuthorizationMiddleware(HostAuthorization{
				AllowedHosts:  []string{"example.com"},
				NormalizePort: tt.normalizePort,
			})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				w.WriteHeader(http.StatusNoContent)
			}))

			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, "http://example.com/", nil)
			req.Host = "example.com:8443"

			handler.ServeHTTP(rec, req)

			if rec.Code != tt.wantStatus {
				t.Fatalf("status = %d, want %d", rec.Code, tt.wantStatus)
			}
		})
	}
}

func TestHostAuthorizationMiddlewareRejectsUnknownForwardedHost(t *testing.T) {
	called := false
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"example.com"},
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://example.com/", nil)
	req.Header.Set(headerXForwardedHost, "example.com, evil.example")

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("next handler was called")
	}
	if rec.Code != http.StatusMisdirectedRequest {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMisdirectedRequest)
	}
}

func TestHostAuthorizationMiddlewareAllowsKnownForwardedHosts(t *testing.T) {
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"example.com", "*.example.net"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://example.com/", nil)
	req.Header.Add(headerXForwardedHost, "example.com")
	req.Header.Add(headerXForwardedHost, "edge.example.net, app.example.net")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
}

func TestHostAuthorizationMiddlewareAllowsLocalhostWhenOptedIn(t *testing.T) {
	tests := []string{
		"localhost:3000",
		"app.localhost",
		"127.0.0.1:5000",
		"[::1]:8080",
		"0.0.0.0:8080",
	}

	for _, host := range tests {
		t.Run(host, func(t *testing.T) {
			handler := HostAuthorizationMiddleware(HostAuthorization{
				AllowLocalhost: true,
			})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				w.WriteHeader(http.StatusNoContent)
			}))

			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, "http://example.com/", nil)
			req.Host = host

			handler.ServeHTTP(rec, req)

			if rec.Code != http.StatusNoContent {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
			}
		})
	}
}

func TestHostAuthorizationMiddlewareRejectsLocalhostByDefault(t *testing.T) {
	handler := HostAuthorizationMiddleware(HostAuthorization{})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://localhost/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusMisdirectedRequest {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMisdirectedRequest)
	}
}

func TestHostAuthorizationMiddlewareSupportsForbiddenJSONRejection(t *testing.T) {
	handler := HostAuthorizationMiddleware(HostAuthorization{
		AllowedHosts: []string{"example.com"},
		StatusCode:   http.StatusForbidden,
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://evil.example/", nil)
	req.Header.Set("Accept", "application/json")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusForbidden {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusForbidden)
	}
	if got := rec.Result().Header.Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}
	if got := rec.Body.String(); !strings.Contains(got, `"error":"forbidden"`) {
		t.Fatalf("body = %q, want JSON error", got)
	}
}
