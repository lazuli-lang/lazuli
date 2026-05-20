package lazuli

import (
	"net/http/httptest"
	"strings"
	"testing"
)

func TestDefaultCSPBuilderHTMLProducesSafeHeader(t *testing.T) {
	b := DefaultCSPBuilder{}
	req := httptest.NewRequest("GET", "/", nil)
	csp := b.Build(httptest.NewRecorder(), req, Context{RouteKind: RouteHTML})
	if !strings.Contains(csp, "default-src 'self'") {
		t.Fatalf("missing default-src: %q", csp)
	}
	if strings.Contains(csp, "unsafe-eval") {
		t.Fatal("default builder leaks unsafe-eval")
	}
}

func TestDefaultCSPBuilderAPIRouteSkips(t *testing.T) {
	b := DefaultCSPBuilder{}
	req := httptest.NewRequest("GET", "/api/x", nil)
	csp := b.Build(httptest.NewRecorder(), req, Context{RouteKind: RouteAPI})
	if csp != "" {
		t.Fatalf("API route should not emit CSP; got %q", csp)
	}
}
