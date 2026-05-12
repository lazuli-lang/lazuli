package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSafeRedirectURLAllowsRelativeByDefault(t *testing.T) {
	for _, target := range []string{
		"/dashboard",
		"/dashboard?tab=activity#latest",
		"settings/profile",
		"?next=/dashboard",
	} {
		got, err := SafeRedirectURL(target, RedirectGuard{})
		if err != nil {
			t.Fatalf("SafeRedirectURL(%q) error = %v", target, err)
		}
		if got != target {
			t.Fatalf("SafeRedirectURL(%q) = %q, want unchanged", target, got)
		}
	}
}

func TestSafeRedirectURLRejectsUnsafeTargets(t *testing.T) {
	tests := []string{
		"",
		" //evil.example/path",
		"//evil.example/path",
		"https://evil.example/path",
		"javascript:alert(1)",
		"data:text/html,hello",
		"/dashboard\r\nSet-Cookie:%20x=y",
		"/dashboard%0d%0aSet-Cookie:x=y",
		"https://user:pass@example.com/path",
		"/admin/../settings",
		"/admin/%2e%2e/settings",
		`/admin\settings`,
		"/admin/%5csettings",
		"/%2f%2fevil.example/path",
	}

	for _, target := range tests {
		if got, err := SafeRedirectURL(target, RedirectGuard{}); !errors.Is(err, ErrRedirectRejected) {
			t.Fatalf("SafeRedirectURL(%q) = %q, %v; want ErrRedirectRejected", target, got, err)
		}
	}
}

func TestSafeRedirectURLAllowsConfiguredAbsoluteURL(t *testing.T) {
	config := RedirectGuard{
		AllowedSchemes: []string{"https"},
		AllowedHosts:   []string{"example.com", "auth.example.com:8443"},
	}

	tests := []string{
		"https://example.com/callback",
		"https://EXAMPLE.com/callback?state=ok",
		"https://auth.example.com:8443/callback",
	}

	for _, target := range tests {
		got, err := SafeRedirectURL(target, config)
		if err != nil {
			t.Fatalf("SafeRedirectURL(%q) error = %v", target, err)
		}
		if got == "" {
			t.Fatalf("SafeRedirectURL(%q) returned empty URL", target)
		}
	}
}

func TestSafeRedirectURLRequiresAllowedAbsoluteSchemeAndHost(t *testing.T) {
	config := RedirectGuard{
		AllowedSchemes: []string{"https"},
		AllowedHosts:   []string{"example.com"},
	}

	tests := []string{
		"http://example.com/callback",
		"https://evil.example/callback",
		"https://example.com:8443/callback",
		"https://user@example.com/callback",
	}

	for _, target := range tests {
		if got, err := SafeRedirectURL(target, config); !errors.Is(err, ErrRedirectRejected) {
			t.Fatalf("SafeRedirectURL(%q) = %q, %v; want ErrRedirectRejected", target, got, err)
		}
	}
}

func TestSafeRedirectURLCanDenyRelativeURLs(t *testing.T) {
	if got, err := SafeRedirectURL("/dashboard", RedirectGuard{DenyRelative: true}); !errors.Is(err, ErrRedirectRejected) {
		t.Fatalf("SafeRedirectURL relative with DenyRelative = %q, %v; want ErrRedirectRejected", got, err)
	}
}

func TestRedirectWritesSafeLocation(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	err := Redirect(rec, req, "/dashboard", http.StatusSeeOther, RedirectGuard{})
	if err != nil {
		t.Fatalf("Redirect error = %v", err)
	}
	if rec.Code != http.StatusSeeOther {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusSeeOther)
	}
	if got := rec.Result().Header.Get("Location"); got != "/dashboard" {
		t.Fatalf("Location = %q, want /dashboard", got)
	}
}

func TestRedirectReturnsErrorWithoutWritingUnsafeLocation(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	err := Redirect(rec, req, "//evil.example/path", http.StatusFound, RedirectGuard{})
	if !errors.Is(err, ErrRedirectRejected) {
		t.Fatalf("Redirect error = %v, want ErrRedirectRejected", err)
	}
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want unwritten recorder status %d", rec.Code, http.StatusOK)
	}
	if got := rec.Result().Header.Get("Location"); got != "" {
		t.Fatalf("Location = %q, want empty", got)
	}
}
