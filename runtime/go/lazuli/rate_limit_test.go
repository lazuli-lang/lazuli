package lazuli

import (
	"os"
	"testing"
)

// `ir-rate-limit-env-aware` Cell 2 — runtime resolution helper tests.
// Each test snapshots `LAZULI_ENV`, sets the env it needs, and
// restores the snapshot afterward so other tests in the package
// (which assume the default `local` env) keep their behaviour.

func withEnv(t *testing.T, key, value string) {
	t.Helper()
	prev, had := os.LookupEnv(key)
	if value == "" {
		if err := os.Unsetenv(key); err != nil {
			t.Fatalf("unset %s: %v", key, err)
		}
	} else {
		if err := os.Setenv(key, value); err != nil {
			t.Fatalf("set %s=%s: %v", key, value, err)
		}
	}
	t.Cleanup(func() {
		if had {
			_ = os.Setenv(key, prev)
		} else {
			_ = os.Unsetenv(key)
		}
	})
}

func TestRateLimit_Resolve_EnvMatchReturnsByEnvLimit(t *testing.T) {
	withEnv(t, "LAZULI_ENV", "test")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			{Envs: []string{"dev", "staging", "test"}, Limit: "1000 per 10 minutes per ip"},
		},
	}
	if got := r.Resolve(); got != "1000 per 10 minutes per ip" {
		t.Fatalf("Resolve(LAZULI_ENV=test) = %q, want the test by_env limit", got)
	}
}

func TestRateLimit_Resolve_FallsBackToDefaultWhenEnvAbsentFromByEnv(t *testing.T) {
	withEnv(t, "LAZULI_ENV", "production")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			{Envs: []string{"dev", "test"}, Limit: "1000 per 10 minutes per ip"},
		},
	}
	if got := r.Resolve(); got != "5 per 10 minutes per ip" {
		t.Fatalf("Resolve(LAZULI_ENV=production) = %q, want the default", got)
	}
}

func TestRateLimit_Resolve_UnsetEnvTreatedAsLocal(t *testing.T) {
	withEnv(t, "LAZULI_ENV", "")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			{Envs: []string{"local"}, Limit: "unlimited resolved"},
		},
	}
	// `LAZULI_ENV` unset == "local" — the local entry wins.
	if got := r.Resolve(); got != "unlimited resolved" {
		t.Fatalf("Resolve(LAZULI_ENV unset) = %q, want the local by_env limit", got)
	}
}

func TestRateLimit_Resolve_UnsetEnvWithNoLocalEntryFallsBackToDefault(t *testing.T) {
	withEnv(t, "LAZULI_ENV", "")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			{Envs: []string{"production"}, Limit: "10 per 10 minutes per ip"},
		},
	}
	if got := r.Resolve(); got != "5 per 10 minutes per ip" {
		t.Fatalf("Resolve(LAZULI_ENV unset, no local entry) = %q, want default", got)
	}
}

func TestRateLimit_IsUnlimited_EmptyResolvedReturnsTrue(t *testing.T) {
	withEnv(t, "LAZULI_ENV", "test")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			// "unlimited" keyword lowered to empty string at parse time.
			{Envs: []string{"test"}, Limit: ""},
		},
	}
	if !r.IsUnlimited() {
		t.Fatalf("IsUnlimited() = false, want true for empty resolved limit")
	}
}

func TestRateLimit_IsUnlimited_NonEmptyResolvedReturnsFalse(t *testing.T) {
	withEnv(t, "LAZULI_ENV", "production")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
	}
	if r.IsUnlimited() {
		t.Fatalf("IsUnlimited() = true, want false for non-empty resolved limit")
	}
}

func TestRateLimit_BackwardCompat_LegacyDefaultOnlyShape(t *testing.T) {
	// Legacy single-`rate_limit "X"` source lowers to
	// `RateLimit{Default: "X"}` with no by_env. `Resolve()` returns
	// the default regardless of env — the pre-`ir-rate-limit-env-aware`
	// behaviour.
	r := RateLimit{Default: "5 per 10 minutes"}
	for _, env := range []string{"", "production", "staging", "test", "dev", "local"} {
		withEnv(t, "LAZULI_ENV", env)
		if got := r.Resolve(); got != "5 per 10 minutes" {
			t.Fatalf("Resolve(LAZULI_ENV=%q) = %q, want %q (backward-compat)",
				env, got, "5 per 10 minutes")
		}
	}
}

func TestRateLimit_Resolve_SourceOrderFirstMatchWins(t *testing.T) {
	// Proposal §9.1 — when two entries share an env (forward-compat,
	// doctor catches it), the first in source order wins. Doctor
	// rejects this in well-formed source, but the resolver is
	// deterministic if it ever reaches here.
	withEnv(t, "LAZULI_ENV", "test")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			{Envs: []string{"test"}, Limit: "first wins"},
			{Envs: []string{"test"}, Limit: "second loses"},
		},
	}
	if got := r.Resolve(); got != "first wins" {
		t.Fatalf("Resolve() = %q, want first source-order match", got)
	}
}

func TestRateLimit_Resolve_UnknownEnvIdentifierFallsBackToDefault(t *testing.T) {
	// Forward-compat: unknown env identifier in source parses OK but
	// never matches `LAZULI_ENV` (proposal §9.2). Resolution falls
	// through to the default.
	withEnv(t, "LAZULI_ENV", "qa")
	r := RateLimit{
		Default: "5 per 10 minutes per ip",
		ByEnv: []RateLimitByEnv{
			{Envs: []string{"dev", "test"}, Limit: "1000 per 10 minutes per ip"},
		},
	}
	if got := r.Resolve(); got != "5 per 10 minutes per ip" {
		t.Fatalf("Resolve(LAZULI_ENV=qa unknown) = %q, want default", got)
	}
}

func TestRateLimitFromDefault_BackwardCompatHelper(t *testing.T) {
	r := RateLimitFromDefault("3 per minute")
	if r.Default != "3 per minute" {
		t.Fatalf("Default = %q, want %q", r.Default, "3 per minute")
	}
	if len(r.ByEnv) != 0 {
		t.Fatalf("ByEnv = %v, want empty slice", r.ByEnv)
	}
}
