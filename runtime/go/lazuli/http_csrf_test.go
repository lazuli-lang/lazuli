package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

// stateChange issues a cross-site, cookie-bearing POST (the CSRF attack shape):
// a browser cross-site request carries `Sec-Fetch-Site: cross-site`, which
// Go's CrossOriginProtection rejects unless the origin is trusted or bypassed.
func stateChangeCrossSite(guard *http.CrossOriginProtection) *httptest.ResponseRecorder {
	next := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	req := httptest.NewRequest("POST", "http://app.example.com/api/v1/c/x", nil)
	req.Header.Set("Sec-Fetch-Site", "cross-site")
	req.Header.Set("Origin", "http://evil.example.com")
	req.AddCookie(&http.Cookie{Name: "lazuli_session", Value: "abc"})
	rec := httptest.NewRecorder()
	guard.Handler(next).ServeHTTP(rec, req)
	return rec
}

// (a) CORS=* no longer disables CSRF: a cross-site cookie POST without a valid
// token is still rejected (403) in a dev env.
func TestNewCSRFGuard_WildcardDoesNotDisableCSRF_Dev(t *testing.T) {
	t.Setenv("LAZULI_ENV", "local")
	guard, err := NewCSRFGuard([]string{"*"})
	if err != nil {
		t.Fatalf("dev wildcard must not error, got %v", err)
	}
	rec := stateChangeCrossSite(guard)
	if rec.Code != http.StatusForbidden {
		t.Fatalf("cross-site cookie POST with CORS=* must be rejected (CSRF enforced); got status %d", rec.Code)
	}
}

// (b) prod + CORS=* errors / refuses to construct the guard.
func TestNewCSRFGuard_WildcardProd_Errors(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	guard, err := NewCSRFGuard([]string{"*"})
	if err == nil {
		t.Fatalf("prod wildcard must error, got nil (guard=%v)", guard)
	}
	if !errors.Is(err, ErrCSRFWildcardProd) {
		t.Fatalf("expected ErrCSRFWildcardProd (CORS-WILDCARD-PROD-001), got %v", err)
	}
	if guard != nil {
		t.Fatalf("guard must be nil on prod-wildcard error")
	}
}

// (b') boot wiring refuses to serve: Mux() panics on prod-wildcard.
func TestMux_WildcardProd_RefusesToServe(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	SetCorsContract(&AppCors{
		AllowOrigins: map[string][]string{"production": {"*"}},
	})
	t.Cleanup(func() { currentCors.Store(nil) })

	defer func() {
		if r := recover(); r == nil {
			t.Fatalf("Mux() must panic (refuse to serve) on prod + CORS=*")
		}
	}()
	_ = Mux()
}

// (c) dev + CORS=* warns but serves (no error, guard usable).
func TestNewCSRFGuard_WildcardDev_Serves(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	guard, err := NewCSRFGuard([]string{"*"})
	if err != nil {
		t.Fatalf("dev wildcard must not error, got %v", err)
	}
	if guard == nil {
		t.Fatalf("dev wildcard must yield a usable guard")
	}
	// A same-origin / non-browser (no Sec-Fetch-Site) request still flows.
	next := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	req := httptest.NewRequest("POST", "/api/v1/c/x", nil)
	rec := httptest.NewRecorder()
	guard.Handler(next).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("same-origin/non-browser POST should pass; got %d", rec.Code)
	}
}

// (d) a valid (trusted-origin) cross-site request still passes.
func TestNewCSRFGuard_TrustedOrigin_Passes(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	guard, err := NewCSRFGuard([]string{"https://app.example.com"})
	if err != nil {
		t.Fatalf("explicit origin must not error, got %v", err)
	}
	next := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	req := httptest.NewRequest("POST", "https://api.example.com/api/v1/c/x", nil)
	req.Header.Set("Sec-Fetch-Site", "cross-site")
	req.Header.Set("Origin", "https://app.example.com")
	req.AddCookie(&http.Cookie{Name: "lazuli_session", Value: "abc"})
	rec := httptest.NewRecorder()
	guard.Handler(next).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("trusted cross-site origin must pass CSRF; got %d", rec.Code)
	}
}

// (e) Bearer-only / non-browser requests behave as before: no Sec-Fetch-Site,
// no cookie — CrossOriginProtection treats them as not-cross-site and lets them
// through, regardless of the wildcard handling.
func TestNewCSRFGuard_BearerOnly_Exempt(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	guard, err := NewCSRFGuard([]string{"https://app.example.com"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	next := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	req := httptest.NewRequest("POST", "https://api.example.com/api/v1/c/x", nil)
	req.Header.Set("Authorization", "Bearer token-123")
	// No Sec-Fetch-Site, no Origin, no cookie — the API-client shape.
	rec := httptest.NewRecorder()
	guard.Handler(next).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("Bearer/non-browser request must remain CSRF-exempt; got %d", rec.Code)
	}
}
