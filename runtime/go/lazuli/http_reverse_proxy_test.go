package lazuli

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestParseReverseProxyTargetValidatesTargetURL(t *testing.T) {
	target, err := ParseReverseProxyTarget("HTTP://example.com/base")
	if err != nil {
		t.Fatalf("ParseReverseProxyTarget error = %v", err)
	}
	if target.Scheme != "http" {
		t.Fatalf("Scheme = %q, want http", target.Scheme)
	}
	if target.Host != "example.com" {
		t.Fatalf("Host = %q, want example.com", target.Host)
	}
	if target.Path != "/base" {
		t.Fatalf("Path = %q, want /base", target.Path)
	}
}

func TestParseReverseProxyTargetRejectsInvalidTargets(t *testing.T) {
	tests := []string{
		"",
		" http://example.com",
		"http://example.com ",
		"/relative",
		"//example.com/path",
		"ftp://example.com",
		"http:///path",
		"http://user:pass@example.com",
		"http://example.com?fixed=1",
		"http://example.com#section",
		"http://example.com/a b",
		`http://example.com\a`,
	}

	for _, raw := range tests {
		if target, err := ParseReverseProxyTarget(raw); !errors.Is(err, ErrReverseProxyTargetRejected) {
			t.Fatalf("ParseReverseProxyTarget(%q) = %v, %v; want ErrReverseProxyTargetRejected", raw, target, err)
		}
	}
}

func TestNewReverseProxyRewritesPathAndPreservesForwardedHeaders(t *testing.T) {
	captured := make(chan *http.Request, 1)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		captured <- r.Clone(r.Context())
		w.WriteHeader(http.StatusNoContent)
	}))
	defer upstream.Close()

	proxy, err := NewReverseProxy(upstream.URL+"/base", ReverseProxyOptions{
		StripPrefix:  "/app",
		TargetPrefix: "/internal",
	})
	if err != nil {
		t.Fatalf("NewReverseProxy error = %v", err)
	}

	req := httptest.NewRequest(http.MethodGet, "http://public.example/app/users/list?filter=a%2Fb", nil)
	req.Host = "public.example"
	req.RemoteAddr = "203.0.113.7:4567"
	req.Header.Set(reverseProxyForwardedHeader, `for=198.51.100.10;proto=https`)
	req.Header.Set(reverseProxyForwardedForHeader, "198.51.100.10")
	req.Header.Set(reverseProxyForwardedHostHeader, "edge.example")
	req.Header.Set(reverseProxyForwardedProtoHeader, "https")

	rec := httptest.NewRecorder()
	proxy.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d; body = %q", rec.Code, http.StatusNoContent, rec.Body.String())
	}

	upstreamReq := <-captured
	if got := upstreamReq.URL.Path; got != "/base/internal/users/list" {
		t.Fatalf("upstream path = %q, want /base/internal/users/list", got)
	}
	if got := upstreamReq.URL.RawQuery; got != "filter=a%2Fb" {
		t.Fatalf("upstream query = %q, want filter=a%%2Fb", got)
	}
	if got := upstreamReq.Host; got != "public.example" {
		t.Fatalf("upstream host = %q, want public.example", got)
	}
	if got := upstreamReq.Header.Get(reverseProxyForwardedHeader); got != `for=198.51.100.10;proto=https` {
		t.Fatalf("Forwarded = %q, want preserved header", got)
	}
	if got := upstreamReq.Header.Get(reverseProxyForwardedForHeader); got != "198.51.100.10, 203.0.113.7" {
		t.Fatalf("X-Forwarded-For = %q, want preserved chain plus client IP", got)
	}
	if got := upstreamReq.Header.Get(reverseProxyForwardedHostHeader); got != "edge.example, public.example" {
		t.Fatalf("X-Forwarded-Host = %q, want preserved chain plus request host", got)
	}
	if got := upstreamReq.Header.Get(reverseProxyForwardedProtoHeader); got != "https, http" {
		t.Fatalf("X-Forwarded-Proto = %q, want preserved chain plus request proto", got)
	}
}

func TestNewReverseProxyCanRewriteHost(t *testing.T) {
	captured := make(chan *http.Request, 1)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		captured <- r.Clone(r.Context())
		w.WriteHeader(http.StatusNoContent)
	}))
	defer upstream.Close()

	proxy, err := NewReverseProxy(upstream.URL, ReverseProxyOptions{RewriteHost: true})
	if err != nil {
		t.Fatalf("NewReverseProxy error = %v", err)
	}

	req := httptest.NewRequest(http.MethodGet, "http://public.example/widgets", nil)
	req.Host = "public.example"
	req.RemoteAddr = "203.0.113.7:4567"

	rec := httptest.NewRecorder()
	proxy.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d; body = %q", rec.Code, http.StatusNoContent, rec.Body.String())
	}

	upstreamReq := <-captured
	wantHost := strings.TrimPrefix(upstream.URL, "http://")
	if got := upstreamReq.Host; got != wantHost {
		t.Fatalf("upstream host = %q, want %q", got, wantHost)
	}
}

func TestReverseProxyProblemErrorHandlerWritesProblemJSON(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/proxied", nil)

	ReverseProxyProblemErrorHandler(rec, req, errors.New("dial failed"))

	if rec.Code != http.StatusBadGateway {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusBadGateway)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}

	var problem Problem
	if err := json.NewDecoder(rec.Body).Decode(&problem); err != nil {
		t.Fatalf("decode problem JSON: %v; body = %q", err, rec.Body.String())
	}
	if problem.Status != http.StatusBadGateway {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadGateway)
	}
	if problem.Detail != reverseProxyDefaultUpstreamDetail {
		t.Fatalf("problem detail = %q, want %q", problem.Detail, reverseProxyDefaultUpstreamDetail)
	}
	if problem.Extensions["code"] != CodeIntegrationError {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeIntegrationError)
	}
}
