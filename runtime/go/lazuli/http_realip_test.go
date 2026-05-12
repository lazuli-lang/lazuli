package lazuli

import (
	"net"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestRealIPMiddlewareTrustsForwardedForFromTrustedProxy(t *testing.T) {
	var got net.IP
	var gotString string

	handler := RealIPMiddleware(RealIPConfig{
		TrustedProxies: []string{"10.0.0.0/8"},
	})(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		got = ClientIP(r)
		gotString = ClientIPString(r)
	}))

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.RemoteAddr = "10.1.2.3:443"
	req.Header.Set("X-Forwarded-For", "203.0.113.10, 10.1.2.3")
	req.Header.Set("X-Real-IP", "198.51.100.20")

	handler.ServeHTTP(httptest.NewRecorder(), req)

	want := net.ParseIP("203.0.113.10")
	if !got.Equal(want) {
		t.Fatalf("ClientIP = %v, want %v", got, want)
	}
	if gotString != "203.0.113.10" {
		t.Fatalf("ClientIPString = %q, want %q", gotString, "203.0.113.10")
	}
}

func TestRealIPMiddlewareIgnoresForwardedHeadersFromUntrustedRemote(t *testing.T) {
	var got net.IP

	handler := RealIPMiddleware(RealIPConfig{
		TrustedProxies: []string{"10.0.0.0/8"},
	})(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		got = ClientIP(r)
	}))

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.RemoteAddr = "198.51.100.44:1234"
	req.Header.Set("X-Forwarded-For", "203.0.113.10")
	req.Header.Set("X-Real-IP", "203.0.113.11")

	handler.ServeHTTP(httptest.NewRecorder(), req)

	want := net.ParseIP("198.51.100.44")
	if !got.Equal(want) {
		t.Fatalf("ClientIP = %v, want %v", got, want)
	}
}

func TestRealIPMiddlewareRequiresTrustedProxyConfig(t *testing.T) {
	var got net.IP

	handler := RealIPMiddleware(RealIPConfig{})(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		got = ClientIP(r)
	}))

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.RemoteAddr = "10.1.2.3:443"
	req.Header.Set("X-Forwarded-For", "203.0.113.10")

	handler.ServeHTTP(httptest.NewRecorder(), req)

	want := net.ParseIP("10.1.2.3")
	if !got.Equal(want) {
		t.Fatalf("ClientIP = %v, want %v", got, want)
	}
}

func TestRealIPMiddlewareUsesRealIPHeaderFromTrustedProxy(t *testing.T) {
	var got net.IP

	handler := RealIPMiddleware(RealIPConfig{
		TrustedProxies: []string{"2001:db8::10"},
	})(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		got = ClientIP(r)
	}))

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.RemoteAddr = "[2001:db8::10]:443"
	req.Header.Set("X-Forwarded-For", "unknown")
	req.Header.Set("X-Real-IP", "2001:db8::99")

	handler.ServeHTTP(httptest.NewRecorder(), req)

	want := net.ParseIP("2001:db8::99")
	if !got.Equal(want) {
		t.Fatalf("ClientIP = %v, want %v", got, want)
	}
}

func TestClientIPFallsBackToRemoteAddrWithoutMiddleware(t *testing.T) {
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.RemoteAddr = "192.0.2.5:8080"

	if got := ClientIPString(req); got != "192.0.2.5" {
		t.Fatalf("ClientIPString = %q, want %q", got, "192.0.2.5")
	}

	req.RemoteAddr = "not-an-address"
	if got := ClientIP(req); got != nil {
		t.Fatalf("ClientIP = %v, want nil", got)
	}
}

func TestRealIPMiddlewarePanicsOnInvalidTrustedProxy(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("RealIPMiddleware did not panic")
		}
	}()

	_ = RealIPMiddleware(RealIPConfig{
		TrustedProxies: []string{"not-a-cidr"},
	})
}
