package i18n

import (
	"embed"
	"strings"
	"testing"
)

// The four embeds below stand in for codegen-emitted per-feature
// `translationFS embed.FS` values. The package-level `//go:embed`
// is the only way to materialize an `embed.FS` in Go, so each
// fixture lives under its own directory.
//
//go:embed testdata/account/*
var accountFS embed.FS

//go:embed testdata/account_v2/*
var accountV2FS embed.FS

//go:embed testdata/billing/*
var billingFS embed.FS

//go:embed testdata/malformed/*
var malformedFS embed.FS

// withFreshResolver swaps in a clean DefaultResolver for the duration
// of one subtest. Each loader test starts from an empty Catalogs map
// so assertions reason about the loader's behaviour in isolation.
func withFreshResolver(t *testing.T) {
	t.Helper()
	prev := defaultResolver
	SetDefaultResolver(&DefaultResolver{
		Catalogs:       map[string]map[string]string{},
		FallbackLocale: "en-US",
	})
	t.Cleanup(func() { SetDefaultResolver(prev) })
}

func TestRegisterFeatureTranslationCatalogLoadsAuthoredText(t *testing.T) {
	withFreshResolver(t)

	if err := RegisterFeatureTranslationCatalog("account", accountFS, "testdata/account"); err != nil {
		t.Fatalf("register: %v", err)
	}

	// PT-BR catalog: bare key from JSON gets qualified as
	// `account.<bare_key>`; resolver returns the authored text when
	// asked via `MessageRef.Qualified()`.
	text, key := Default().Resolve(ErrorRequest{
		MessageKey: MessageRef{Feature: "account", Key: "account_me_signin_required"}.Qualified(),
		Locale:     "pt-BR",
	})
	if want := "AUTHORED PT-BR account.me"; text != want {
		t.Fatalf("pt-BR resolve text = %q, want %q", text, want)
	}
	if want := "account.account_me_signin_required"; key != want {
		t.Fatalf("pt-BR resolve key = %q, want %q", key, want)
	}

	// en-US catalog: same shape, different text.
	text, _ = Default().Resolve(ErrorRequest{
		MessageKey: MessageRef{Feature: "account", Key: "account_me_signin_required"}.Qualified(),
		Locale:     "en-US",
	})
	if want := "AUTHORED EN-US account.me"; text != want {
		t.Fatalf("en-US resolve text = %q, want %q", text, want)
	}
}

func TestRegisterFeatureTranslationCatalogIgnoresAdapterFiles(t *testing.T) {
	withFreshResolver(t)

	if err := RegisterFeatureTranslationCatalog("account", accountFS, "testdata/account"); err != nil {
		t.Fatalf("register: %v", err)
	}

	r := defaultResolver.(*DefaultResolver)
	for locale, catalog := range r.Catalogs {
		for key := range catalog {
			if strings.Contains(key, "ignored_by_loader") {
				t.Fatalf(
					"loader must skip files that don't match `<feature>.<locale>.json`: %s in %s",
					key, locale,
				)
			}
		}
	}
}

func TestRegisterFeatureTranslationCatalogIsIdempotent(t *testing.T) {
	withFreshResolver(t)

	if err := RegisterFeatureTranslationCatalog("account", accountFS, "testdata/account"); err != nil {
		t.Fatalf("first register: %v", err)
	}
	if err := RegisterFeatureTranslationCatalog("account", accountV2FS, "testdata/account_v2"); err != nil {
		t.Fatalf("second register: %v", err)
	}

	// The second registration replaces the first's entries for the
	// same locale + key.
	text, _ := Default().Resolve(ErrorRequest{
		MessageKey: "account.account_me_signin_required",
		Locale:     "pt-BR",
	})
	if want := "REPLACED PT-BR account.me"; text != want {
		t.Fatalf("re-registration must replace prior entries, got %q want %q", text, want)
	}
	text, _ = Default().Resolve(ErrorRequest{
		MessageKey: "account.account_policy_denied",
		Locale:     "pt-BR",
	})
	if want := "REPLACED PT-BR policy_denied"; text != want {
		t.Fatalf("re-registration must replace prior entries, got %q want %q", text, want)
	}
}

func TestRegisterFeatureTranslationCatalogMergesAcrossFeatures(t *testing.T) {
	withFreshResolver(t)

	if err := RegisterFeatureTranslationCatalog("account", accountFS, "testdata/account"); err != nil {
		t.Fatalf("register account: %v", err)
	}
	if err := RegisterFeatureTranslationCatalog("billing", billingFS, "testdata/billing"); err != nil {
		t.Fatalf("register billing: %v", err)
	}

	// Both features now have entries under the same locale; neither
	// stomps the other.
	text, _ := Default().Resolve(ErrorRequest{
		MessageKey: "account.account_me_signin_required",
		Locale:     "pt-BR",
	})
	if want := "AUTHORED PT-BR account.me"; text != want {
		t.Fatalf("account survived billing register? got %q want %q", text, want)
	}
	text, _ = Default().Resolve(ErrorRequest{
		MessageKey: "billing.billing_policy_denied",
		Locale:     "pt-BR",
	})
	if want := "AUTHORED PT-BR billing policy_denied"; text != want {
		t.Fatalf("billing missing? got %q want %q", text, want)
	}
}

func TestRegisterFeatureTranslationCatalogReturnsErrorOnMalformedJSON(t *testing.T) {
	withFreshResolver(t)

	err := RegisterFeatureTranslationCatalog("malformed", malformedFS, "testdata/malformed")
	if err == nil {
		t.Fatal("expected error on malformed JSON, got nil")
	}
	if !strings.Contains(err.Error(), "parse") {
		t.Fatalf("expected parse error, got %v", err)
	}
}

func TestRegisterFeatureTranslationCatalogReturnsErrorOnEmptyFeatureName(t *testing.T) {
	withFreshResolver(t)

	if err := RegisterFeatureTranslationCatalog("", accountFS, "testdata/account"); err == nil {
		t.Fatal("expected error on empty feature name, got nil")
	}
}

func TestRegisterFeatureTranslationCatalogReturnsErrorOnMissingBasePath(t *testing.T) {
	withFreshResolver(t)

	err := RegisterFeatureTranslationCatalog("account", accountFS, "testdata/does_not_exist")
	if err == nil {
		t.Fatal("expected error on missing basePath, got nil")
	}
}
