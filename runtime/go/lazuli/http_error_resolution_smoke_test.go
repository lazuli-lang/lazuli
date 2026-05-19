package lazuli

import (
	"context"
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

// TestWave35AuthoredCatalogSurfacesPerCommandKey is the Wave 3.5
// regression guard: it asserts that the authored per-feature
// `translation` block — when registered via
// `RegisterFeatureTranslationCatalog` and selected by a per-command
// `ErrorKeys.PolicyDenied` — wins over the built-in L3 floor.
//
// Before Wave 3.5 the resolver fell through to the built-in PT-BR
// because the per-feature catalog was never loaded into
// `i18n.DefaultResolver.Catalogs`. This test exercises the full
// pipeline: stamp MessageKey (`handleCommandRequest` →
// `stampPerCommandMessageKey`) → resolver L1 lookup → response body
// carries the AUTHORED text (NOT the builtin).
func TestWave35AuthoredCatalogSurfacesPerCommandKey(t *testing.T) {
	installZeroAuthoringFixture(t)

	// Load the per-feature catalog via the wire-thin loader. Real
	// codegen does this from a generated `init()`; the test does it
	// directly so it does not depend on the codegen pipeline.
	prevCatalogs := map[string]map[string]string{}
	if r, ok := i18n.Default().(*i18n.DefaultResolver); ok {
		for locale, cat := range r.Catalogs {
			copied := map[string]string{}
			for k, v := range cat {
				copied[k] = v
			}
			prevCatalogs[locale] = copied
		}
	}
	t.Cleanup(func() {
		if r, ok := i18n.Default().(*i18n.DefaultResolver); ok {
			r.Catalogs = prevCatalogs
		}
	})

	// Mimic the merge `RegisterFeatureTranslationCatalog` performs:
	// qualified `<feature>.<bare>` key under each locale.
	resolver := i18n.Default().(*i18n.DefaultResolver)
	if resolver.Catalogs == nil {
		resolver.Catalogs = map[string]map[string]string{}
	}
	for _, locale := range []string{"pt-BR", "en-US"} {
		if resolver.Catalogs[locale] == nil {
			resolver.Catalogs[locale] = map[string]string{}
		}
	}
	resolver.Catalogs["pt-BR"]["account.account_me_signin_required"] = "AUTHORED PT-BR account.me"
	resolver.Catalogs["en-US"]["account.account_me_signin_required"] = "AUTHORED EN-US account.me"

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.me", nil)
	req.Header.Set("Accept-Language", "pt-BR")

	// `handleCommandRequest` would normally do this; we mimic the
	// per-command stamp here so the test exercises the resolver
	// chain end-to-end without spinning up the dispatcher.
	keys := &i18n.ErrorKeys{
		PolicyDenied: i18n.MessageRef{Feature: "account", Key: "account_me_signin_required"},
	}
	le := &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "no policy atom matches the active actor for @policy.authenticated",
	}
	stampPerCommandMessageKey(le, keys)

	if le.MessageKey != "account.account_me_signin_required" {
		t.Fatalf(
			"stampPerCommandMessageKey did not write qualified key: got %q want %q",
			le.MessageKey, "account.account_me_signin_required",
		)
	}

	writeError(rec, req, le)
	payload := decodePayload(t, rec.Body.Bytes())

	if got, want := payload["message"], "AUTHORED PT-BR account.me"; got != want {
		t.Fatalf(
			"Wave 3.5 regression: authored per-feature key did not surface — got %q, want %q",
			got, want,
		)
	}
	if got, want := payload["message_key"], "account.account_me_signin_required"; got != want {
		t.Fatalf("message_key = %q, want %q", got, want)
	}

	// Same flow under en-US — the AUTHORED en-US text wins, not the
	// builtin en-US floor.
	rec = httptest.NewRecorder()
	req = httptest.NewRequest(http.MethodPost, "/api/v1/c/account.me", nil)
	req.Header.Set("Accept-Language", "en-US")
	le = &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "no policy atom matches the active actor for @policy.authenticated",
	}
	stampPerCommandMessageKey(le, keys)
	writeError(rec, req, le)
	payload = decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "AUTHORED EN-US account.me"; got != want {
		t.Fatalf("en-US authored: got %q want %q", got, want)
	}
}

func TestQueryPolicyDeniedSurfacesPolicyWhenDeniedOverride(t *testing.T) {
	installZeroAuthoringFixture(t)
	t.Cleanup(clearRegistriesForTest())

	resolver := i18n.Default().(*i18n.DefaultResolver)
	if resolver.Catalogs == nil {
		resolver.Catalogs = map[string]map[string]string{}
	}
	if resolver.Catalogs["en-US"] == nil {
		resolver.Catalogs["en-US"] = map[string]string{}
	}
	resolver.Catalogs["en-US"]["account.account_signin"] = "AUTHORED EN-US account signin"

	queryKeys := &i18n.ErrorKeys{
		PolicyDenied: i18n.MessageRef{Feature: "account", Key: "account_signin"},
	}
	query := &Query[struct{}, struct{}]{
		Name: "account.query.me",
		Kind: QueryList,
		Policy: Policy{
			Name: "@policy.authenticated",
			Atoms: []PolicyAtom{
				{Namespace: "scope", Name: "authenticated"},
			},
		},
		ErrorKeys: queryKeys,
		WithSource: func(ctx context.Context) context.Context {
			return WithSource(ctx, SourceTag{
				Capsule: "demo",
				Feature: "account",
				Kind:    "query",
				Op:      "me",
			})
		},
	}
	Register(query)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/q/account.query.me", nil)
	req.Header.Set("Accept-Language", "en-US")

	Mux().ServeHTTP(rec, req)

	if got, want := rec.Code, http.StatusForbidden; got != want {
		t.Fatalf("status = %d, want %d; body: %s", got, want, rec.Body.String())
	}
	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "AUTHORED EN-US account signin"; got != want {
		t.Fatalf("query policy denial message = %q, want %q", got, want)
	}
	if got, want := payload["message_key"], "account.account_signin"; got != want {
		t.Fatalf("query message_key = %q, want %q", got, want)
	}
	if got := payload["message"]; got == "You need to sign in to do this." {
		t.Fatalf("query fell through to builtin floor: %v", payload)
	}
}

// TestZeroAuthoringAllEightCodesResolveThroughBuiltin asserts the
// built-in catalog covers every closed-catalog error code (§2.D).
// The framework promises: any code the runtime itself emits gets a
// layperson native message even on zero-authoring apps.
//
// DB-INTEGRITY-CATALOG-EXT (2026-05-19): extended to cover the 4
// db-integrity codes emitted by `classifyDBError`.
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
		{"unique_violation", http.StatusConflict},
		{"foreign_key_violation", http.StatusBadRequest},
		{"not_null_violation", http.StatusBadRequest},
		{"check_violation", http.StatusBadRequest},
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
