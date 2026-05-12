package lazuli

import (
	"crypto/tls"
	"net/http"
	"net/http/httptest"
	"slices"
	"testing"
)

func TestDefaultServerTLSConfigAppliesDefaultsWithoutMutatingBase(t *testing.T) {
	base := &tls.Config{
		ServerName:               "api.example.test",
		MinVersion:               tls.VersionTLS10,
		NextProtos:               []string{"acme-tls/1", "http/1.1"},
		PreferServerCipherSuites: false,
	}

	config := DefaultServerTLSConfig(base)

	if config == base {
		t.Fatal("DefaultServerTLSConfig returned the input config")
	}
	if config.ServerName != base.ServerName {
		t.Fatalf("ServerName = %q, want %q", config.ServerName, base.ServerName)
	}
	if config.MinVersion != DefaultServerTLSMinVersion {
		t.Fatalf("MinVersion = %x, want %x", config.MinVersion, DefaultServerTLSMinVersion)
	}
	if !config.PreferServerCipherSuites {
		t.Fatal("PreferServerCipherSuites = false, want true")
	}
	wantNextProtos := []string{"h2", "http/1.1", "acme-tls/1"}
	if !slices.Equal(config.NextProtos, wantNextProtos) {
		t.Fatalf("NextProtos = %v, want %v", config.NextProtos, wantNextProtos)
	}

	if base.MinVersion != tls.VersionTLS10 {
		t.Fatalf("base MinVersion = %x, want %x", base.MinVersion, tls.VersionTLS10)
	}
	if !slices.Equal(base.NextProtos, []string{"acme-tls/1", "http/1.1"}) {
		t.Fatalf("base NextProtos = %v, want unchanged", base.NextProtos)
	}
	if base.PreferServerCipherSuites {
		t.Fatal("base PreferServerCipherSuites = true, want unchanged false")
	}
}

func TestDefaultServerTLSConfigKeepsStricterMinimumVersion(t *testing.T) {
	base := &tls.Config{MinVersion: tls.VersionTLS13}

	config := DefaultServerTLSConfig(base)

	if config.MinVersion != tls.VersionTLS13 {
		t.Fatalf("MinVersion = %x, want %x", config.MinVersion, tls.VersionTLS13)
	}
}

func TestDefaultServerTLSNextProtosReturnsClone(t *testing.T) {
	protos := DefaultServerTLSNextProtos()
	protos[0] = "mutated"

	next := DefaultServerTLSNextProtos()

	if !slices.Equal(next, []string{"h2", "http/1.1"}) {
		t.Fatalf("DefaultServerTLSNextProtos = %v, want default clone", next)
	}
}

func TestCloneTLSConfigHandlesNilAndClonesSlices(t *testing.T) {
	if config := CloneTLSConfig(nil); config == nil {
		t.Fatal("CloneTLSConfig(nil) returned nil")
	}

	base := &tls.Config{NextProtos: []string{"acme-tls/1"}}
	clone := CloneTLSConfig(base)
	clone.NextProtos[0] = "changed"

	if got := base.NextProtos[0]; got != "acme-tls/1" {
		t.Fatalf("base NextProtos[0] = %q, want acme-tls/1", got)
	}
}

func TestServerWithTLSDefaultsClonesServerAndTLSConfig(t *testing.T) {
	handler := http.NewServeMux()
	tlsConfig := &tls.Config{NextProtos: []string{"acme-tls/1"}}
	server := &http.Server{
		Addr:      ":8443",
		Handler:   handler,
		TLSConfig: tlsConfig,
	}

	configured := ServerWithTLSDefaults(server)

	if configured == server {
		t.Fatal("ServerWithTLSDefaults returned the input server")
	}
	if configured.Addr != server.Addr {
		t.Fatalf("Addr = %q, want %q", configured.Addr, server.Addr)
	}
	if configured.Handler != handler {
		t.Fatalf("Handler = %v, want configured handler", configured.Handler)
	}
	if configured.TLSConfig == tlsConfig {
		t.Fatal("ServerWithTLSDefaults reused the input TLSConfig")
	}
	wantNextProtos := []string{"h2", "http/1.1", "acme-tls/1"}
	if !slices.Equal(configured.TLSConfig.NextProtos, wantNextProtos) {
		t.Fatalf("NextProtos = %v, want %v", configured.TLSConfig.NextProtos, wantNextProtos)
	}
	if !slices.Equal(tlsConfig.NextProtos, []string{"acme-tls/1"}) {
		t.Fatalf("input TLSConfig NextProtos = %v, want unchanged", tlsConfig.NextProtos)
	}
}

func TestWithHSTSIntegratesWithSecurityHeadersMiddleware(t *testing.T) {
	headers := WithHSTS(SecurityHeaders{ContentTypeOptions: "nosniff"}, "")
	handler := SecurityHeadersMiddleware(headers)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	result := rec.Result()
	if got := result.Header.Get("Strict-Transport-Security"); got != DefaultHSTSPolicy {
		t.Fatalf("Strict-Transport-Security = %q, want %q", got, DefaultHSTSPolicy)
	}
	if got := result.Header.Get("X-Content-Type-Options"); got != "nosniff" {
		t.Fatalf("X-Content-Type-Options = %q, want nosniff", got)
	}
}

func TestWithHSTSPreservesCustomPolicy(t *testing.T) {
	headers := WithHSTS(SecurityHeaders{}, " max-age=60 ")

	if got := headers.StrictTransportSecurity; got != "max-age=60" {
		t.Fatalf("StrictTransportSecurity = %q, want max-age=60", got)
	}
}
