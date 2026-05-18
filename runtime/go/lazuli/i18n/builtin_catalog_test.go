package i18n

import (
	"strings"
	"testing"
)

// expectedCodes mirrors the closed catalog of framework-emitted
// error codes (proposal §2.D, §5.4). The PT-BR and en-US JSON files
// MUST cover all eight — otherwise the resolver's L3/L4 falls
// through to the zero pair on a code that the framework itself
// emits, restoring the pre-RUNTIME-1 wire jargon.
var expectedCodes = []string{
	"policy_denied",
	"validation_failed",
	"tenant_mismatch",
	"not_found",
	"rate_limited",
	"bad_request",
	"method_not_allowed",
	"integration_error",
}

func TestBuiltinCatalogPTBRCoversAllCodes(t *testing.T) {
	cat := BuiltinCatalog("pt-BR")
	if cat == nil {
		t.Fatal("BuiltinCatalog(\"pt-BR\") returned nil — pt-BR must ship with the runtime")
	}
	for _, code := range expectedCodes {
		text, ok := cat[code]
		if !ok {
			t.Errorf("pt-BR catalog missing code %q", code)
			continue
		}
		if strings.TrimSpace(text) == "" {
			t.Errorf("pt-BR[%q] is empty/whitespace; must be a layperson message", code)
		}
	}
}

func TestBuiltinCatalogEnUSCoversAllCodes(t *testing.T) {
	cat := BuiltinCatalog("en-US")
	if cat == nil {
		t.Fatal("BuiltinCatalog(\"en-US\") returned nil — en-US must ship with the runtime")
	}
	for _, code := range expectedCodes {
		text, ok := cat[code]
		if !ok {
			t.Errorf("en-US catalog missing code %q", code)
			continue
		}
		if strings.TrimSpace(text) == "" {
			t.Errorf("en-US[%q] is empty/whitespace; must be a layperson message", code)
		}
	}
}

func TestBuiltinCatalogPolicyDeniedPTBRMatchesProposal(t *testing.T) {
	cat := BuiltinCatalog("pt-BR")
	got := cat["policy_denied"]
	want := "Você precisa entrar na sua conta para fazer isso."
	if got != want {
		t.Fatalf("pt-BR[policy_denied] = %q, want %q (proposal §2.D verbatim)", got, want)
	}
}

func TestBuiltinCatalogPolicyDeniedEnUSMatchesProposal(t *testing.T) {
	cat := BuiltinCatalog("en-US")
	got := cat["policy_denied"]
	want := "You need to sign in to do this."
	if got != want {
		t.Fatalf("en-US[policy_denied] = %q, want %q (proposal §2.D verbatim)", got, want)
	}
}

func TestBuiltinCatalogUnknownLocaleReturnsNil(t *testing.T) {
	if cat := BuiltinCatalog("xx-XX"); cat != nil {
		t.Fatalf("BuiltinCatalog(\"xx-XX\") = %v, want nil", cat)
	}
	if cat := BuiltinCatalog(""); cat != nil {
		t.Fatalf("BuiltinCatalog(\"\") = %v, want nil", cat)
	}
}

func TestBuiltinCatalogReturnsCopyNotSingleton(t *testing.T) {
	// Mutating one returned map must not corrupt the cached
	// singleton — the next caller gets the original strings.
	first := BuiltinCatalog("pt-BR")
	first["policy_denied"] = "MUTATED"
	second := BuiltinCatalog("pt-BR")
	if second["policy_denied"] == "MUTATED" {
		t.Fatal("BuiltinCatalog returned a shared reference; mutating one corrupted the next caller's view")
	}
}

// TestBuiltinCatalogNoFrameworkJargon is the load-bearing regression
// guard: the whole point of Cell RUNTIME-2 is to ban framework
// vocabulary ("policy atom", "axis", "RBAC", "tenant_id", …) from
// end-user-visible strings. If a future contributor edits the JSON
// and slips in any banned term, this test fails.
func TestBuiltinCatalogNoFrameworkJargon(t *testing.T) {
	bannedSubstrings := []string{
		"policy atom",
		"policy_atom",
		"atom",
		"axis",
		"RBAC",
		"rbac",
		"tenant_id",
		"actor.org",
		"@policy.",
		"@scope.",
		"PolicyAtom",
		"no policy atom matches",
		"no policy branch matches",
	}
	locales := []string{"pt-BR", "en-US"}
	for _, locale := range locales {
		cat := BuiltinCatalog(locale)
		for code, text := range cat {
			lower := strings.ToLower(text)
			for _, banned := range bannedSubstrings {
				if strings.Contains(lower, strings.ToLower(banned)) {
					t.Errorf("locale %q code %q contains banned jargon %q in text %q",
						locale, code, banned, text)
				}
			}
		}
	}
}

func TestBuiltinLocalesEnumeratesShippedLocales(t *testing.T) {
	got := BuiltinLocales()
	if len(got) != 2 {
		t.Fatalf("BuiltinLocales() len = %d, want 2", len(got))
	}
	have := map[string]bool{}
	for _, locale := range got {
		have[locale] = true
	}
	for _, want := range []string{"pt-BR", "en-US"} {
		if !have[want] {
			t.Errorf("BuiltinLocales() missing %q (got %v)", want, got)
		}
	}
}

// TestDefaultResolverWiresBuiltinAtPackageInit asserts the package
// init installed `NewDefaultResolver`, so a freshly imported `i18n`
// package already serves the built-in catalog without any boot
// glue. This is the load-bearing wiring contract.
func TestDefaultResolverWiresBuiltinAtPackageInit(t *testing.T) {
	r := Default()
	text, key := r.Resolve(ErrorRequest{
		Code:   "policy_denied",
		Locale: "pt-BR",
	})
	wantText := "Você precisa entrar na sua conta para fazer isso."
	wantKey := "builtin.policy_denied"
	if text != wantText {
		t.Fatalf("default resolver pt-BR policy_denied text = %q, want %q", text, wantText)
	}
	if key != wantKey {
		t.Fatalf("default resolver pt-BR policy_denied key = %q, want %q", key, wantKey)
	}
}

func TestDefaultResolverEnUSFallbackThroughBuiltin(t *testing.T) {
	// L3 miss on a locale we don't ship → L4 hits en-US via builtin.
	r := Default()
	text, key := r.Resolve(ErrorRequest{
		Code:   "policy_denied",
		Locale: "fr-FR", // not shipped
	})
	wantText := "You need to sign in to do this."
	wantKey := "builtin.policy_denied"
	if text != wantText {
		t.Fatalf("L4 fallback text = %q, want %q", text, wantText)
	}
	if key != wantKey {
		t.Fatalf("L4 fallback key = %q, want %q", key, wantKey)
	}
}
