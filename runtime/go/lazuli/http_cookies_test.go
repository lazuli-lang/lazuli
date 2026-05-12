package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSetCookieUsesSessionDefaults(t *testing.T) {
	rec := httptest.NewRecorder()

	SetCookie(rec, "session", "token", CookieOpts{TTL: time.Hour})

	cookies := rec.Result().Cookies()
	if len(cookies) != 1 {
		t.Fatalf("cookies len = %d, want 1", len(cookies))
	}
	cookie := cookies[0]
	if cookie.Name != "session" {
		t.Fatalf("Name = %q, want %q", cookie.Name, "session")
	}
	if cookie.Value != "token" {
		t.Fatalf("Value = %q, want %q", cookie.Value, "token")
	}
	if cookie.Path != "/" {
		t.Fatalf("Path = %q, want /", cookie.Path)
	}
	if !cookie.HttpOnly {
		t.Fatal("HttpOnly = false, want true")
	}
	if cookie.Secure {
		t.Fatal("Secure = true, want false")
	}
	if cookie.SameSite != http.SameSiteStrictMode {
		t.Fatalf("SameSite = %v, want %v", cookie.SameSite, http.SameSiteStrictMode)
	}
	if cookie.MaxAge != int(time.Hour.Seconds()) {
		t.Fatalf("MaxAge = %d, want %d", cookie.MaxAge, int(time.Hour.Seconds()))
	}
}

func TestSetCookieUsesOptions(t *testing.T) {
	rec := httptest.NewRecorder()

	SetCookie(rec, "csrf", "nonce", CookieOpts{
		TTL:      5 * time.Minute,
		Path:     "/auth",
		Domain:   "example.test",
		AllowJS:  true,
		Secure:   true,
		SameSite: http.SameSiteLaxMode,
	})

	cookie := rec.Result().Cookies()[0]
	if cookie.Path != "/auth" {
		t.Fatalf("Path = %q, want /auth", cookie.Path)
	}
	if cookie.Domain != "example.test" {
		t.Fatalf("Domain = %q, want example.test", cookie.Domain)
	}
	if cookie.HttpOnly {
		t.Fatal("HttpOnly = true, want false")
	}
	if !cookie.Secure {
		t.Fatal("Secure = false, want true")
	}
	if cookie.SameSite != http.SameSiteLaxMode {
		t.Fatalf("SameSite = %v, want %v", cookie.SameSite, http.SameSiteLaxMode)
	}
}

func TestGetCookie(t *testing.T) {
	r := httptest.NewRequest(http.MethodGet, "/", nil)
	r.AddCookie(&http.Cookie{Name: "session", Value: "token"})

	got, err := GetCookie(r, "session")
	if err != nil {
		t.Fatalf("GetCookie returned error: %v", err)
	}
	if got != "token" {
		t.Fatalf("GetCookie = %q, want token", got)
	}
}

func TestGetCookieMissing(t *testing.T) {
	r := httptest.NewRequest(http.MethodGet, "/", nil)

	got, err := GetCookie(r, "session")
	if got != "" {
		t.Fatalf("GetCookie = %q, want empty", got)
	}
	if !errors.Is(err, ErrCookieMissing) {
		t.Fatalf("error = %v, want ErrCookieMissing", err)
	}
}

func TestDeleteCookie(t *testing.T) {
	rec := httptest.NewRecorder()

	DeleteCookie(rec, "session")

	header := rec.Result().Header.Get("Set-Cookie")
	if !strings.Contains(header, "session=") {
		t.Fatalf("Set-Cookie = %q, want session cookie", header)
	}
	if !strings.Contains(header, "Max-Age=0") {
		t.Fatalf("Set-Cookie = %q, want Max-Age=0 delete marker", header)
	}
	if !strings.Contains(header, "Path=/") {
		t.Fatalf("Set-Cookie = %q, want Path=/", header)
	}
}
