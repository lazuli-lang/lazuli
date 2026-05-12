package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSecurityHeadersMiddlewareAppliesDefaultHeaders(t *testing.T) {
	handler := SecurityHeadersMiddleware(DefaultSecurityHeaders())(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusAccepted)
		_, _ = w.Write([]byte("accepted"))
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	header := rec.Result().Header
	want := map[string]string{
		"Strict-Transport-Security": "max-age=31536000; includeSubDomains",
		"Content-Security-Policy":   "default-src 'self'; base-uri 'self'; frame-ancestors 'none'; object-src 'none'",
		"X-Frame-Options":           "DENY",
		"X-Content-Type-Options":    "nosniff",
		"Referrer-Policy":           "no-referrer",
		"Permissions-Policy":        "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()",
	}
	for name, value := range want {
		if got := header.Get(name); got != value {
			t.Fatalf("%s = %q, want %q", name, got, value)
		}
	}
	if got := rec.Body.String(); got != "accepted" {
		t.Fatalf("body = %q, want accepted", got)
	}
}

func TestSecurityHeadersMiddlewarePreservesDownstreamHeaders(t *testing.T) {
	handler := SecurityHeadersMiddleware(DefaultSecurityHeaders())(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Security-Policy", "default-src 'none'")
		w.Header().Set("X-Frame-Options", "SAMEORIGIN")
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	header := rec.Result().Header
	if got := header.Get("Content-Security-Policy"); got != "default-src 'none'" {
		t.Fatalf("Content-Security-Policy = %q, want downstream value", got)
	}
	if got := header.Get("X-Frame-Options"); got != "SAMEORIGIN" {
		t.Fatalf("X-Frame-Options = %q, want downstream value", got)
	}
	if got := header.Get("X-Content-Type-Options"); got != "nosniff" {
		t.Fatalf("X-Content-Type-Options = %q, want nosniff", got)
	}
}

func TestSecurityHeadersMiddlewareOverridesDownstreamHeadersWhenConfigured(t *testing.T) {
	config := DefaultSecurityHeaders()
	config.OverrideExisting = true

	handler := SecurityHeadersMiddleware(config)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Referrer-Policy", "same-origin")
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get("Referrer-Policy"); got != "no-referrer" {
		t.Fatalf("Referrer-Policy = %q, want no-referrer", got)
	}
}

func TestSecurityHeadersMiddlewareAppliesWhenHandlerDoesNotWrite(t *testing.T) {
	handler := SecurityHeadersMiddleware(SecurityHeaders{
		ContentTypeOptions: "nosniff",
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get("X-Content-Type-Options"); got != "nosniff" {
		t.Fatalf("X-Content-Type-Options = %q, want nosniff", got)
	}
}

func TestSecurityHeadersMiddlewarePreservesImplicitContentType(t *testing.T) {
	handler := SecurityHeadersMiddleware(SecurityHeaders{
		ContentTypeOptions: "nosniff",
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte("plain text"))
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get("Content-Type"); got != "text/plain; charset=utf-8" {
		t.Fatalf("Content-Type = %q, want text/plain; charset=utf-8", got)
	}
	if got := rec.Result().Header.Get("X-Content-Type-Options"); got != "nosniff" {
		t.Fatalf("X-Content-Type-Options = %q, want nosniff", got)
	}
}

func TestSecurityHeadersMiddlewareEmptyConfigOmitsHeaders(t *testing.T) {
	handler := SecurityHeadersMiddleware(SecurityHeaders{})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	for _, name := range []string{
		"Strict-Transport-Security",
		"Content-Security-Policy",
		"X-Frame-Options",
		"X-Content-Type-Options",
		"Referrer-Policy",
		"Permissions-Policy",
	} {
		if got := rec.Result().Header.Get(name); got != "" {
			t.Fatalf("%s = %q, want empty", name, got)
		}
	}
}
