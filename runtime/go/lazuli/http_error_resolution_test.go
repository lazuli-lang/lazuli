package lazuli

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

// installFixture wires a process-global resolver + locale contract +
// app error registry for the duration of a single test. Returns a
// cleanup the test must invoke.
func installFixture(t *testing.T) {
	t.Helper()

	prevResolver := i18n.Default()
	prevRegistry := appErrorRegistry
	prevLocale := appLocaleContract

	resolver := &i18n.DefaultResolver{
		Registry: i18n.AppErrorResolverRegistry{
			Features: map[string]i18n.FeatureErrorContract{
				"account": {
					Default:         i18n.ExposureHide,
					ExposeClient4xx: []string{"message", "code", "message_key"},
					ExposeClient5xx: []string{"code"},
					Messages: map[string]i18n.MessageRef{
						"policy_denied": {Feature: "account", Key: "account_signin_required"},
					},
				},
				"billing": {
					Default:         i18n.ExposureHide,
					ExposeClient4xx: []string{"message", "code"}, // no message_key exposure
				},
			},
		},
		Catalogs: map[string]map[string]string{
			"pt-BR": {
				"account.account_signin_required": "Entre na sua conta para usar Account.",
				"builtin.policy_denied":           "Você precisa entrar na sua conta para fazer isso.",
			},
			"en-US": {
				"account.account_signin_required": "Sign in to use Account.",
				"builtin.policy_denied":           "You need to sign in to do this.",
			},
		},
		FallbackLocale: "en-US",
	}
	i18n.SetDefaultResolver(resolver)
	RegisterAppErrorResolver(resolver.Registry)
	RegisterAppLocaleContract(i18n.LocaleContract{
		Default:   "en-US",
		Supported: []string{"en-US", "pt-BR"},
	})

	t.Cleanup(func() {
		i18n.SetDefaultResolver(prevResolver)
		appErrorRegistry = prevRegistry
		appLocaleContract = prevLocale
	})
}

func decodePayload(t *testing.T, body []byte) map[string]any {
	t.Helper()
	var payload map[string]any
	if err := json.Unmarshal(body, &payload); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	return payload
}

func TestWriteErrorResolvesLocaleViaAcceptLanguage(t *testing.T) {
	installFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	req.Header.Set("Accept-Language", "pt-BR")
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))

	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "no policy atom matches the active actor for @policy.authenticated",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "Entre na sua conta para usar Account."; got != want {
		t.Fatalf("message = %q, want %q", got, want)
	}
	if got, want := payload["code"], "policy_denied"; got != want {
		t.Fatalf("code = %q, want %q", got, want)
	}
	if got, want := payload["message_key"], "account.account_signin_required"; got != want {
		t.Fatalf("message_key = %q, want %q", got, want)
	}
}

func TestWriteErrorEnglishWhenAcceptLanguageMissing(t *testing.T) {
	installFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	// No Accept-Language → default to contract default (en-US).
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))

	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "legacy fallback text",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "Sign in to use Account."; got != want {
		t.Fatalf("message = %q, want %q", got, want)
	}
}

func TestWriteErrorMessageKeyHiddenWhenContractExcludes(t *testing.T) {
	installFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/billing.invoice", nil)
	req.Header.Set("Accept-Language", "pt-BR")
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "billing", Kind: "command", Op: "invoice",
	}))

	// billing has no L2 message override → resolver hits L3 builtin.policy_denied
	// in pt-BR. message_key should be "builtin.policy_denied". billing
	// contract excludes message_key from Expose4xx → field omitted.
	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "legacy fallback",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "Você precisa entrar na sua conta para fazer isso."; got != want {
		t.Fatalf("message = %q, want %q", got, want)
	}
	if _, present := payload["message_key"]; present {
		t.Fatalf("message_key must be omitted under billing contract, got %v", payload["message_key"])
	}
}

func TestWriteError5xxHidesMessageEvenWithContract(t *testing.T) {
	installFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))

	writeError(rec, req, &Error{
		Status:  http.StatusInternalServerError,
		Code:    CodeInternal,
		Message: "an internal panic happened, do not leak this to clients",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["code"], "internal"; got != want {
		t.Fatalf("code = %q, want %q", got, want)
	}
	if _, present := payload["message"]; present {
		t.Fatalf("5xx message must be force-hidden, got %v", payload["message"])
	}
}

func TestWriteErrorLegacyFallbackWhenNoContract(t *testing.T) {
	// No fixture installed — registry is empty, resolver is the
	// default-empty resolver. Legacy behaviour must hold: the wire
	// payload carries Message/Code/Data as before.
	prevResolver := i18n.Default()
	prevRegistry := appErrorRegistry
	prevLocale := appLocaleContract
	i18n.SetDefaultResolver(nil)
	appErrorRegistry = i18n.AppErrorResolverRegistry{}
	appLocaleContract = i18n.LocaleContract{}
	t.Cleanup(func() {
		i18n.SetDefaultResolver(prevResolver)
		appErrorRegistry = prevRegistry
		appLocaleContract = prevLocale
	})

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/foo", nil)

	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "no policy atom matches the active actor for @policy.foo",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "no policy atom matches the active actor for @policy.foo"; got != want {
		t.Fatalf("message = %q, want %q", got, want)
	}
	if got, want := payload["code"], "policy_denied"; got != want {
		t.Fatalf("code = %q, want %q", got, want)
	}
}

func TestPolicyDeniedSmokingGunSetsMessageKey(t *testing.T) {
	// Verify that policy.go:146 stamps MessageKey on the typed envelope.
	// This guards the smoking-gun fix against regressions.
	ctx := &Ctx{Actor: ActorAnonymous}
	err := EvalPolicy(ctx, Policy{
		Name: "@policy.authenticated",
		Atoms: []PolicyAtom{
			{Namespace: "scope", Name: "authenticated"},
		},
	})
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("EvalPolicy returned %T, want *Error", err)
	}
	if got, want := le.MessageKey, "policy_denied"; got != want {
		t.Fatalf("MessageKey = %q, want %q (proposal §11 RUNTIME-1 smoking-gun)", got, want)
	}
}

func TestWriteErrorAcceptLanguageBeatsContractDefault(t *testing.T) {
	installFixture(t)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.choose_role", nil)
	req.Header.Set("Accept-Language", "pt-BR,en;q=0.5")
	req = req.WithContext(WithSource(req.Context(), SourceTag{
		Capsule: "demo", Feature: "account", Kind: "command", Op: "choose_role",
	}))

	writeError(rec, req, &Error{
		Status:  http.StatusForbidden,
		Code:    CodePolicyDenied,
		Message: "legacy",
	})

	payload := decodePayload(t, rec.Body.Bytes())
	if got, want := payload["message"], "Entre na sua conta para usar Account."; got != want {
		t.Fatalf("message = %q, want %q", got, want)
	}
}
