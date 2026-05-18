package lazuli

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

// installZeroAuthoringFixture resets per-feature error contracts to
// empty (the zero-authoring state) and installs the production
// locale contract that codegen emits for every Lazuli app
// (`default_locale "pt-BR"` per `examples/full-capsule/app.lzi:7`).
// This is the canonical "no .lzi error-vocab authoring" state — the
// app declared its supported locales but no `errors` blocks, no
// `when_denied` overrides, no per-feature `RegisterFeatureErrors`
// calls.
func installZeroAuthoringFixture(t *testing.T) {
	t.Helper()
	prevResolver := i18n.Default()
	prevRegistry := appErrorRegistry
	prevLocale := appLocaleContract
	appErrorRegistry = i18n.AppErrorResolverRegistry{}
	appLocaleContract = i18n.LocaleContract{
		Default:   "pt-BR",
		Supported: []string{"pt-BR", "en-US"},
	}
	i18n.SetDefaultResolver(i18n.NewDefaultResolver())
	t.Cleanup(func() {
		i18n.SetDefaultResolver(prevResolver)
		appErrorRegistry = prevRegistry
		appLocaleContract = prevLocale
	})
}

// TestZeroAuthoringPolicyDeniedEmitsBuiltinPTBR is the regression
// guard for the hostpoint field incident (proposal §1.1). A
// freshly-scaffolded app with ZERO `.lzi` error-vocab authoring —
// no `errors` block, no `when_denied`, no feature contract
// registered — must still emit the layperson PT-BR message
// on `policy_denied`, NEVER the legacy
// `"no policy atom matches the active actor for @policy.X"`
// jargon string.
//
// This test exercises the full HTTP boundary: `writeError` →
// resolver `Default()` → embedded built-in catalog. The default
// resolver is wired by the package-level `init` in
// `runtime/go/lazuli/boot.go`, so the test does NOT install any
// per-feature error contract — only the locale contract that
// codegen always emits.
func TestZeroAuthoringPolicyDeniedEmitsBuiltinPTBR(t *testing.T) {
	installZeroAuthoringFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/foo.bar", nil)
	req.Header.Set("Accept-Language", "pt-BR")

	// The legacy jargon string is exactly what `policy.go:146` would
	// emit if no override ran. The wire MUST replace it with the
	// built-in PT-BR text.
	writeError(rec, req, &Error{
		Status:     http.StatusForbidden,
		Code:       CodePolicyDenied,
		Message:    "no policy atom matches the active actor for @policy.authenticated",
		MessageKey: "policy_denied",
	})

	body := rec.Body.String()
	if strings.Contains(body, "no policy atom matches") {
		t.Fatalf("regression: wire payload contains the banned jargon string: %s", body)
	}
	if strings.Contains(body, "@policy.") {
		t.Fatalf("regression: wire payload leaks `@policy.` namespace token: %s", body)
	}

	payload := decodePayload(t, rec.Body.Bytes())
	wantMessage := "Você precisa entrar na sua conta para fazer isso."
	if got, ok := payload["message"].(string); !ok || got != wantMessage {
		t.Fatalf("message = %v, want %q (Cell RUNTIME-2 §2.D verbatim)", payload["message"], wantMessage)
	}
	if got, want := payload["code"], "policy_denied"; got != want {
		t.Fatalf("code = %q, want %q", got, want)
	}
}

// TestZeroAuthoringPolicyDeniedEmitsBuiltinEnUSWhenLocaleAbsent
// asserts the en-US fallback path: when an English Accept-Language
// is sent (or `pt-BR` is unavailable), the boundary negotiates
// en-US and the built-in catalog serves the en-US text.
func TestZeroAuthoringPolicyDeniedEmitsBuiltinEnUSWhenLocaleAbsent(t *testing.T) {
	installZeroAuthoringFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/foo.bar", nil)
	req.Header.Set("Accept-Language", "en-US")

	writeError(rec, req, &Error{
		Status:     http.StatusForbidden,
		Code:       CodePolicyDenied,
		Message:    "no policy atom matches the active actor for @policy.authenticated",
		MessageKey: "policy_denied",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	wantMessage := "You need to sign in to do this."
	if got, ok := payload["message"].(string); !ok || got != wantMessage {
		t.Fatalf("message = %v, want %q", payload["message"], wantMessage)
	}
}

// TestZeroAuthoringAllEightCodesResolveThroughBuiltin asserts the
// built-in catalog covers every closed-catalog error code (§2.D).
// The framework promises: any code the runtime itself emits gets a
// layperson native message even on zero-authoring apps.
func TestZeroAuthoringAllEightCodesResolveThroughBuiltin(t *testing.T) {
	installZeroAuthoringFixture(t)

	codes := []struct {
		code   string
		status int
	}{
		{"policy_denied", http.StatusForbidden},
		{"validation_failed", http.StatusBadRequest},
		{"tenant_mismatch", http.StatusBadRequest},
		{"not_found", http.StatusNotFound},
		{"rate_limited", http.StatusTooManyRequests},
		{"bad_request", http.StatusBadRequest},
		{"method_not_allowed", http.StatusMethodNotAllowed},
		{"integration_error", http.StatusBadGateway},
	}
	for _, c := range codes {
		c := c
		t.Run(c.code, func(t *testing.T) {
			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodPost, "/api/v1/c/foo.bar", nil)
			req.Header.Set("Accept-Language", "pt-BR")
			writeError(rec, req, &Error{
				Status:  c.status,
				Code:    c.code,
				Message: "legacy raw text that must be replaced",
			})
			payload := decodePayload(t, rec.Body.Bytes())
			msg, ok := payload["message"].(string)
			if !ok {
				t.Fatalf("payload has no message for code %q: %v", c.code, payload)
			}
			if msg == "" || msg == "legacy raw text that must be replaced" {
				t.Fatalf("code %q: built-in PT-BR did not fire, message = %q", c.code, msg)
			}
			if strings.Contains(msg, "policy atom") ||
				strings.Contains(msg, "@policy.") ||
				strings.Contains(msg, "tenant_id") {
				t.Fatalf("code %q: jargon leaked into message %q", c.code, msg)
			}
		})
	}
}
