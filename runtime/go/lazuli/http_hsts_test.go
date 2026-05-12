package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestDefaultHSTSConfigBuildsDefaultPolicy(t *testing.T) {
	if got := HSTSHeaderValue(DefaultHSTSConfig()); got != DefaultHSTSPolicy {
		t.Fatalf("HSTSHeaderValue(DefaultHSTSConfig()) = %q, want %q", got, DefaultHSTSPolicy)
	}
}

func TestHSTSHeaderValueSupportsPreloadPolicy(t *testing.T) {
	config := HSTSConfig{
		MaxAgeSeconds:     63072000,
		IncludeSubDomains: true,
		Preload:           true,
	}

	if got := HSTSHeaderValue(config); got != "max-age=63072000; includeSubDomains; preload" {
		t.Fatalf("HSTSHeaderValue() = %q, want preload policy", got)
	}
}

func TestHSTSMiddlewareWritesHeaderForTLSRequest(t *testing.T) {
	handler := DefaultHSTSMiddleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "https://example.test/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if got := rec.Result().Header.Get(headerStrictTransportSecurity); got != DefaultHSTSPolicy {
		t.Fatalf("%s = %q, want %q", headerStrictTransportSecurity, got, DefaultHSTSPolicy)
	}
}

func TestHSTSMiddlewareSkipsPlainHTTPRequest(t *testing.T) {
	handler := DefaultHSTSMiddleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://example.test/", nil)

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get(headerStrictTransportSecurity); got != "" {
		t.Fatalf("%s = %q, want empty for plain HTTP", headerStrictTransportSecurity, got)
	}
}

func TestHSTSMiddlewareTrustsForwardedProtoWhenConfigured(t *testing.T) {
	handler := DefaultHSTSMiddleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://example.test/", nil)
	req.Header.Set(headerXForwardedProto, "https, http")

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get(headerStrictTransportSecurity); got != DefaultHSTSPolicy {
		t.Fatalf("%s = %q, want %q", headerStrictTransportSecurity, got, DefaultHSTSPolicy)
	}
}

func TestHSTSMiddlewareIgnoresForwardedProtoUnlessConfigured(t *testing.T) {
	config := DefaultHSTSConfig()
	config.TrustXForwardedProto = false
	handler := HSTSMiddleware(config)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "http://example.test/", nil)
	req.Header.Set(headerXForwardedProto, "https")

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get(headerStrictTransportSecurity); got != "" {
		t.Fatalf("%s = %q, want empty without forwarded proto trust", headerStrictTransportSecurity, got)
	}
}

func TestHSTSMiddlewarePreservesDownstreamHeaderByDefault(t *testing.T) {
	handler := DefaultHSTSMiddleware()(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set(headerStrictTransportSecurity, "max-age=60")
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "https://example.test/", nil)

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get(headerStrictTransportSecurity); got != "max-age=60" {
		t.Fatalf("%s = %q, want downstream value", headerStrictTransportSecurity, got)
	}
}

func TestHSTSMiddlewareOverridesDownstreamHeaderWhenConfigured(t *testing.T) {
	config := DefaultHSTSConfig()
	config.Preload = true
	config.OverrideExisting = true
	handler := HSTSMiddleware(config)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set(headerStrictTransportSecurity, "max-age=60")
		w.WriteHeader(http.StatusOK)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "https://example.test/", nil)

	handler.ServeHTTP(rec, req)

	want := "max-age=31536000; includeSubDomains; preload"
	if got := rec.Result().Header.Get(headerStrictTransportSecurity); got != want {
		t.Fatalf("%s = %q, want %q", headerStrictTransportSecurity, got, want)
	}
}
