package lazuli

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func strptr(s string) *string { return &s }
func boolptr(b bool) *bool    { return &b }

// A zero-valued SessionCookieConfig (the `cookie`-less case) must stamp a
// cookie byte-identical to the pre-`cookie` runtime: name lazuli_session,
// Path=/, HttpOnly, Secure, SameSite=Lax.
func TestSessionCookieDefaultsUnchangedWhenNoConfig(t *testing.T) {
	t.Cleanup(func() { ConfigureSessionCookie(SessionCookieConfig{}) })
	ConfigureSessionCookie(SessionCookieConfig{})

	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.SetSessionCookie("tok", time.Minute)

	c := rec.Result().Cookies()[0]
	if c.Name != ProductionSessionCookieName {
		t.Fatalf("name = %q, want %q", c.Name, ProductionSessionCookieName)
	}
	if c.Path != "/" {
		t.Fatalf("path = %q, want /", c.Path)
	}
	if !c.HttpOnly {
		t.Fatalf("HttpOnly should default true")
	}
	if !c.Secure {
		t.Fatalf("Secure should default true")
	}
	if c.SameSite != http.SameSiteLaxMode {
		t.Fatalf("SameSite = %v, want Lax", c.SameSite)
	}
	if c.Domain != "" {
		t.Fatalf("Domain = %q, want host-only", c.Domain)
	}
}

// Declared axes flow through the CookieOpts sink; http_only=true maps to
// the inverse AllowJS (HttpOnly set). same_site=strict / secure=false /
// domain / path / name all override.
func TestSessionCookieDeclaredAttributesFlowThrough(t *testing.T) {
	t.Cleanup(func() {
		ConfigureSessionCookie(SessionCookieConfig{})
		SetSessionCookieSecure(true)
	})
	ConfigureSessionCookie(SessionCookieConfig{
		Name:     strptr("app_sess"),
		SameSite: func() *http.SameSite { s := http.SameSiteStrictMode; return &s }(),
		Secure:   boolptr(false),
		HTTPOnly: boolptr(true),
		Domain:   strptr(".example.com"),
		Path:     strptr("/app"),
	})

	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.SetSessionCookie("tok", time.Minute)

	// Assert on the raw Set-Cookie header. Note: Go's stdlib
	// `http.SetCookie` strips a leading-dot Domain to host form when it
	// serializes the header (`sanitizeCookieDomain`), per RFC 6265
	// §4.1.2.3 (a leading-dot domain and the bare domain are equivalent
	// and both match subdomains). Our lowering passes `.example.com`
	// faithfully to the CookieOpts sink; Go normalizes it on emit. So the
	// declared domain surfaces on the wire as `Domain=example.com`.
	header := rec.Header().Get("Set-Cookie")
	if !strings.HasPrefix(header, "app_sess=") {
		t.Fatalf("name not flowed; header = %q", header)
	}
	if !strings.Contains(header, "Domain=example.com") {
		t.Fatalf("declared domain not stamped; header = %q", header)
	}
	if !strings.Contains(header, "Path=/app") {
		t.Fatalf("declared path not stamped; header = %q", header)
	}
	if !strings.Contains(header, "SameSite=Strict") {
		t.Fatalf("declared same_site not stamped; header = %q", header)
	}
	// secure=false declared → no Secure attribute.
	if strings.Contains(header, "Secure") {
		t.Fatalf("secure=false declared but Secure present; header = %q", header)
	}
	// http_only=true declared → HttpOnly attribute present.
	if !strings.Contains(header, "HttpOnly") {
		t.Fatalf("http_only=true declared but HttpOnly absent; header = %q", header)
	}
}

// http_only=false must clear HttpOnly (AllowJS=true). A partial config
// (only http_only) keeps every other axis at the runtime default.
func TestSessionCookieHTTPOnlyFalseAllowsJS(t *testing.T) {
	t.Cleanup(func() { ConfigureSessionCookie(SessionCookieConfig{}) })
	ConfigureSessionCookie(SessionCookieConfig{HTTPOnly: boolptr(false)})

	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.SetSessionCookie("tok", time.Minute)

	c := rec.Result().Cookies()[0]
	if c.HttpOnly {
		t.Fatalf("http_only=false should clear HttpOnly")
	}
	// Untouched axes keep defaults.
	if c.Path != "/" || c.SameSite != http.SameSiteLaxMode {
		t.Fatalf("partial config leaked into other axes: path=%q samesite=%v", c.Path, c.SameSite)
	}
}

// The refresh cookie inherits the declared transport attributes but keeps
// the canonical lazuli_refresh name (the name axis is session-scoped).
func TestRefreshCookieKeepsCanonicalNameButInheritsAttrs(t *testing.T) {
	t.Cleanup(func() { ConfigureSessionCookie(SessionCookieConfig{}) })
	ConfigureSessionCookie(SessionCookieConfig{
		Name:     strptr("app_sess"),
		SameSite: func() *http.SameSite { s := http.SameSiteStrictMode; return &s }(),
		Path:     strptr("/app"),
	})

	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.SetRefreshCookie("rtok", time.Minute)

	c := rec.Result().Cookies()[0]
	if c.Name != ProductionRefreshCookieName {
		t.Fatalf("refresh name = %q, want %q (name is session-scoped)", c.Name, ProductionRefreshCookieName)
	}
	if c.SameSite != http.SameSiteStrictMode || c.Path != "/app" {
		t.Fatalf("refresh cookie should inherit transport attrs: samesite=%v path=%q", c.SameSite, c.Path)
	}
}

// SET and READ stay symmetric: a declared name is both stamped by
// SetSessionCookie and resolved by extractSessionToken.
func TestSessionCookieNameRoundTripsSetThenRead(t *testing.T) {
	t.Cleanup(func() { ConfigureSessionCookie(SessionCookieConfig{}) })
	ConfigureSessionCookie(SessionCookieConfig{Name: strptr("app_sess")})

	if got := SessionCookieName(); got != "app_sess" {
		t.Fatalf("SessionCookieName() = %q, want app_sess", got)
	}
	req := httptest.NewRequest(http.MethodGet, "/x", nil)
	req.AddCookie(&http.Cookie{Name: "app_sess", Value: "tok-123"})
	if got := extractSessionToken(req); got != "tok-123" {
		t.Fatalf("extractSessionToken = %q, want tok-123 (SET/READ asymmetry)", got)
	}
}

func TestClearSessionCookieHonorsDeclaredName(t *testing.T) {
	t.Cleanup(func() { ConfigureSessionCookie(SessionCookieConfig{}) })
	ConfigureSessionCookie(SessionCookieConfig{Name: strptr("app_sess")})

	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.ClearSessionCookie()

	header := rec.Header().Get("Set-Cookie")
	if !strings.HasPrefix(header, "app_sess=") {
		t.Fatalf("cleared cookie = %q, want app_sess prefix", header)
	}
}
