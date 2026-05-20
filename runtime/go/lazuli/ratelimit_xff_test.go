package lazuli

import (
	"net/http/httptest"
	"net/netip"
	"testing"
)

func TestXFFIgnoredWithoutTrustedProxies(t *testing.T) {
	t.Cleanup(func() { SetTrustedProxies(nil) })
	SetTrustedProxies(nil)
	req := httptest.NewRequest("GET", "/", nil)
	req.RemoteAddr = "203.0.113.1:1234"
	req.Header.Set("X-Forwarded-For", "192.0.2.99")
	got := clientIPFromRequest(req)
	if got != "203.0.113.1" {
		t.Fatalf("XFF should be ignored without trust list; got %q", got)
	}
}

func TestXFFHonoredFromTrustedProxy(t *testing.T) {
	t.Cleanup(func() { SetTrustedProxies(nil) })
	SetTrustedProxies([]netip.Prefix{netip.MustParsePrefix("10.0.0.0/8")})
	req := httptest.NewRequest("GET", "/", nil)
	req.RemoteAddr = "10.0.0.5:1234"
	req.Header.Set("X-Forwarded-For", "203.0.113.7")
	got := clientIPFromRequest(req)
	if got != "203.0.113.7" {
		t.Fatalf("XFF should be honored from trusted proxy; got %q", got)
	}
}
