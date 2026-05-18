package i18n

import "testing"

// fixture builds a resolver populated with:
//   - L1 per-command key   "account.choose_role_signin_required" in pt-BR + en-US
//   - L2 feature-level key "account.account_signin_required"      in pt-BR + en-US
//   - L3 builtin key       "builtin.policy_denied"                in pt-BR
//   - L4 builtin key       "builtin.tenant_mismatch"              in en-US only
func fixture() *DefaultResolver {
	return &DefaultResolver{
		Registry: AppErrorResolverRegistry{
			Features: map[string]FeatureErrorContract{
				"account": {
					Default:         ExposureHide,
					ExposeClient4xx: []string{"message", "code", "message_key"},
					Messages: map[string]MessageRef{
						"policy_denied": {Feature: "account", Key: "account_signin_required"},
					},
				},
			},
		},
		Catalogs: map[string]map[string]string{
			"pt-BR": {
				"account.choose_role_signin_required": "Para escolher seu papel, entre na sua conta primeiro.",
				"account.account_signin_required":     "Entre na sua conta para usar Account.",
				"builtin.policy_denied":               "Você precisa entrar na sua conta para fazer isso.",
			},
			"en-US": {
				"account.choose_role_signin_required": "To choose your role, please sign in first.",
				"account.account_signin_required":     "Sign in to use Account.",
				"builtin.tenant_mismatch":             "Wrong workspace selected.",
			},
		},
		FallbackLocale: "en-US",
	}
}

func TestResolverL1CommandOverride(t *testing.T) {
	r := fixture()
	text, key := r.Resolve(ErrorRequest{
		Code:       "policy_denied",
		MessageKey: "account.choose_role_signin_required",
		Feature:    "account",
		Locale:     "pt-BR",
	})
	if want := "Para escolher seu papel, entre na sua conta primeiro."; text != want {
		t.Fatalf("L1 text = %q, want %q", text, want)
	}
	if want := "account.choose_role_signin_required"; key != want {
		t.Fatalf("L1 key = %q, want %q", key, want)
	}
}

func TestResolverL2FeatureCatchAll(t *testing.T) {
	r := fixture()
	// No MessageKey set on the error → L1 miss → L2 hit on the
	// feature's policy_denied entry.
	text, key := r.Resolve(ErrorRequest{
		Code:    "policy_denied",
		Feature: "account",
		Locale:  "pt-BR",
	})
	if want := "Entre na sua conta para usar Account."; text != want {
		t.Fatalf("L2 text = %q, want %q", text, want)
	}
	if want := "account.account_signin_required"; key != want {
		t.Fatalf("L2 key = %q, want %q", key, want)
	}
}

func TestResolverL3BuiltinCurrentLocale(t *testing.T) {
	r := fixture()
	// Feature without an L2 override for this code → L2 miss → L3 hit
	// on builtin.policy_denied in pt-BR.
	text, key := r.Resolve(ErrorRequest{
		Code:    "policy_denied",
		Feature: "billing", // not in fixture registry
		Locale:  "pt-BR",
	})
	if want := "Você precisa entrar na sua conta para fazer isso."; text != want {
		t.Fatalf("L3 text = %q, want %q", text, want)
	}
	if want := "builtin.policy_denied"; key != want {
		t.Fatalf("L3 key = %q, want %q", key, want)
	}
}

func TestResolverL4BuiltinFallbackLocale(t *testing.T) {
	r := fixture()
	// pt-BR has no builtin.tenant_mismatch entry → L3 miss → L4 hits
	// en-US fallback.
	text, key := r.Resolve(ErrorRequest{
		Code:    "tenant_mismatch",
		Feature: "billing",
		Locale:  "pt-BR",
	})
	if want := "Wrong workspace selected."; text != want {
		t.Fatalf("L4 text = %q, want %q", text, want)
	}
	if want := "builtin.tenant_mismatch"; key != want {
		t.Fatalf("L4 key = %q, want %q", key, want)
	}
}

func TestResolverNoCatalogEntryReturnsZero(t *testing.T) {
	r := fixture()
	text, key := r.Resolve(ErrorRequest{
		Code:    "unknown_code",
		Feature: "billing",
		Locale:  "pt-BR",
	})
	if text != "" || key != "" {
		t.Fatalf("expected zero pair for unknown code, got (%q, %q)", text, key)
	}
}

func TestResolverEmptyMessageKeyDoesNotMatchL1(t *testing.T) {
	r := fixture()
	// Empty MessageKey must not accidentally resolve to anything via L1.
	// L1 should be skipped; only L2 (feature catch-all) is consulted.
	text, key := r.Resolve(ErrorRequest{
		Code:       "policy_denied",
		MessageKey: "",
		Feature:    "account",
		Locale:     "en-US",
	})
	if want := "Sign in to use Account."; text != want {
		t.Fatalf("text = %q, want %q", text, want)
	}
	if want := "account.account_signin_required"; key != want {
		t.Fatalf("key = %q, want %q", key, want)
	}
}

func TestResolverL1MissFallsThroughToL2(t *testing.T) {
	r := fixture()
	// MessageKey set but missing from catalog → must fall through to L2.
	text, key := r.Resolve(ErrorRequest{
		Code:       "policy_denied",
		MessageKey: "account.nonexistent_key",
		Feature:    "account",
		Locale:     "en-US",
	})
	if want := "Sign in to use Account."; text != want {
		t.Fatalf("text = %q, want %q", text, want)
	}
	if want := "account.account_signin_required"; key != want {
		t.Fatalf("key = %q, want %q", key, want)
	}
}

func TestResolverFallbackLocaleSameAsRequestedSkipsL4(t *testing.T) {
	r := fixture()
	// Requested locale = fallback locale → L3 and L4 are the same lookup.
	// Just verify no double-fire / weird behavior when locale == fallback.
	text, _ := r.Resolve(ErrorRequest{
		Code:    "tenant_mismatch",
		Feature: "billing",
		Locale:  "en-US",
	})
	if want := "Wrong workspace selected."; text != want {
		t.Fatalf("text = %q, want %q", text, want)
	}
}

func TestDefaultResolverGlobal(t *testing.T) {
	prev := Default()
	t.Cleanup(func() { SetDefaultResolver(prev) })

	custom := fixture()
	SetDefaultResolver(custom)
	if Default() != custom {
		t.Fatal("SetDefaultResolver did not install the custom resolver")
	}

	SetDefaultResolver(nil)
	if Default() == nil {
		t.Fatal("nil reset must leave a non-nil resolver installed")
	}
	// Empty default returns the zero pair for any input.
	text, key := Default().Resolve(ErrorRequest{Code: "policy_denied", Locale: "pt-BR"})
	if text != "" || key != "" {
		t.Fatalf("empty resolver returned (%q, %q), want zero pair", text, key)
	}
}

func TestMessageRefQualified(t *testing.T) {
	t.Run("with_feature", func(t *testing.T) {
		got := MessageRef{Feature: "account", Key: "k"}.Qualified()
		if want := "account.k"; got != want {
			t.Fatalf("Qualified() = %q, want %q", got, want)
		}
	})
	t.Run("without_feature", func(t *testing.T) {
		got := MessageRef{Key: "k"}.Qualified()
		if want := "k"; got != want {
			t.Fatalf("Qualified() = %q, want %q", got, want)
		}
	})
	t.Run("is_zero", func(t *testing.T) {
		if !(MessageRef{}).IsZero() {
			t.Fatal("zero MessageRef must report IsZero=true")
		}
		if (MessageRef{Key: "k"}).IsZero() {
			t.Fatal("non-empty MessageRef must report IsZero=false")
		}
	})
}

func TestShouldExposeRules(t *testing.T) {
	contract := FeatureErrorContract{
		Default:         ExposureHide,
		ExposeClient4xx: []string{"message", "code", "message_key"},
		ExposeClient5xx: []string{"code"},
	}
	cases := []struct {
		name   string
		status int
		field  string
		want   bool
	}{
		{"4xx_message_exposed", 403, "message", true},
		{"4xx_message_key_exposed", 403, "message_key", true},
		{"4xx_data_hidden_by_default", 403, "data", false},
		{"4xx_code_always_exposed", 403, "code", true},
		{"5xx_message_force_hidden", 500, "message", false},
		{"5xx_code_exposed", 500, "code", true},
		{"5xx_data_hidden", 500, "data", false},
	}
	for _, c := range cases {
		c := c
		t.Run(c.name, func(t *testing.T) {
			if got := ShouldExpose(contract, c.status, c.field); got != c.want {
				t.Fatalf("ShouldExpose(%d, %q) = %v, want %v", c.status, c.field, got, c.want)
			}
		})
	}
	t.Run("expose_default_opens_unlisted_fields", func(t *testing.T) {
		c := FeatureErrorContract{Default: ExposureExpose}
		if !ShouldExpose(c, 403, "data") {
			t.Fatal("ExposureExpose default must expose data")
		}
		if !ShouldExpose(c, 403, "message_key") {
			t.Fatal("ExposureExpose default must expose message_key")
		}
		if ShouldExpose(c, 500, "message") {
			t.Fatal("5xx message stays force-hidden even under ExposureExpose")
		}
	})
}
