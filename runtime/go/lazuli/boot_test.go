package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

// resetEscape clears the process-global escape renderer.
// Use with t.Cleanup so a test cannot leak state into the next.
func resetEscape() { i18n.SetEscapeRenderer(nil) }

func TestRegisterErrorRendererEscape_WinsOverChain(t *testing.T) {
	installFixture(t)
	t.Cleanup(resetEscape)

	// The escape returns a non-empty string for every call → wire MUST
	// carry the escape's text, not the resolver chain's text.
	const want = "[escape] custom wire text"
	RegisterErrorRendererEscape(func(le *Error, locale string) string {
		if le == nil {
			t.Fatal("escape received nil *Error; Source not threaded through")
		}
		if le.Code != CodePolicyDenied {
			t.Fatalf("escape saw Code=%q, want %q", le.Code, CodePolicyDenied)
		}
		if locale == "" {
			t.Fatal("escape received empty locale; boundary did not negotiate")
		}
		return want
	})

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	req.Header.Set("Accept-Language", "pt-BR")
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))
	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "raw lib bug string — must not reach the wire",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got := payload["message"]; got != want {
		t.Fatalf("message = %q, want %q (escape did not win over chain)", got, want)
	}
}

func TestRegisterErrorRendererEscape_NotRegistered_ChainRuns(t *testing.T) {
	installFixture(t)
	// Sanity guard: do NOT register an escape. Confirm RUNTIME-1 +
	// RUNTIME-2 behaviour is preserved (resolver chain wins).
	t.Cleanup(resetEscape)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	req.Header.Set("Accept-Language", "pt-BR")
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))
	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "raw lib bug string — must not reach the wire",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	// installFixture seeds account.policy_denied → account_signin_required
	// in pt-BR = "Entre na sua conta para usar Account."
	if want := "Entre na sua conta para usar Account."; payload["message"] != want {
		t.Fatalf("message = %q, want %q (resolver chain did not run)", payload["message"], want)
	}
	if want := "account.account_signin_required"; payload["message_key"] != want {
		t.Fatalf("message_key = %q, want %q", payload["message_key"], want)
	}
}

func TestRegisterErrorRendererEscape_EmptyFallsThroughSelectively(t *testing.T) {
	installFixture(t)
	t.Cleanup(resetEscape)

	// The escape returns a custom string ONLY for the `tenant_mismatch`
	// code; every other code returns "" → chain proceeds normally.
	const override = "[escape] white-label tenant_mismatch text"
	RegisterErrorRendererEscape(func(le *Error, locale string) string {
		if le != nil && le.Code == "tenant_mismatch" {
			return override
		}
		return ""
	})

	// 1. tenant_mismatch → escape wins.
	{
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.foo", nil)
		req = req.WithContext(WithSource(req.Context(), SourceTag{
			Capsule: "demo", Feature: "account", Kind: "command", Op: "foo",
		}))
		writeError(rec, req, &Error{
			Status:  http.StatusForbidden,
			Code:    "tenant_mismatch",
			Message: "raw lib bug string",
		})
		payload := decodePayload(t, rec.Body.Bytes())
		if got := payload["message"]; got != override {
			t.Fatalf("tenant_mismatch message = %q, want %q (escape did not win)", got, override)
		}
	}

	// 2. policy_denied → escape returns "" → chain wins.
	{
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
		req.Header.Set("Accept-Language", "pt-BR")
		req = req.WithContext(WithSource(req.Context(), SourceTag{
			Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
		}))
		writeError(rec, req, &Error{
			Status:  http.StatusForbidden,
			Code:    CodePolicyDenied,
			Message: "raw lib bug string",
		})
		payload := decodePayload(t, rec.Body.Bytes())
		if want := "Entre na sua conta para usar Account."; payload["message"] != want {
			t.Fatalf("policy_denied message = %q, want %q (chain should have won)", payload["message"], want)
		}
	}
}

func TestRegisterErrorRendererEscape_NilClears(t *testing.T) {
	installFixture(t)
	t.Cleanup(resetEscape)

	// Install an escape, then clear it. Wire must revert to chain output.
	RegisterErrorRendererEscape(func(*Error, string) string {
		return "[escape] should not appear after nil reset"
	})
	RegisterErrorRendererEscape(nil)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	req.Header.Set("Accept-Language", "pt-BR")
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))
	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "raw lib bug string",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if want := "Entre na sua conta para usar Account."; payload["message"] != want {
		t.Fatalf("after nil reset, message = %q, want %q", payload["message"], want)
	}
}
