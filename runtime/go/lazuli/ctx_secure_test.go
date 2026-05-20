package lazuli

import (
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSessionCookieSecureByDefault(t *testing.T) {
	// Snapshot + restore the flag (test isolation).
	t.Cleanup(func() { SetSessionCookieSecure(true) })

	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.SetSessionCookie("tok", time.Minute)
	header := rec.Header().Get("Set-Cookie")
	if !strings.Contains(header, "Secure") {
		t.Fatalf("session cookie missing Secure flag: %q", header)
	}
}

func TestSessionCookieSecureOptOut(t *testing.T) {
	t.Cleanup(func() { SetSessionCookieSecure(true) })

	SetSessionCookieSecure(false)
	rec := httptest.NewRecorder()
	ctx := &Ctx{responseWriter: rec}
	ctx.SetSessionCookie("tok", time.Minute)
	header := rec.Header().Get("Set-Cookie")
	if strings.Contains(header, "Secure") {
		t.Fatalf("opt-out failed; cookie has Secure: %q", header)
	}
}
