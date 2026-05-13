package featureflags_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/featureflags"
)

func TestNormalizeGrowthBookConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	fetchedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	config, err := featureflags.NormalizeGrowthBookConfig(featureflags.GrowthBookConfig{
		APIHost:     " https://cdn.growthbook.io/api/features/ ",
		ClientKey:   " sdk-client-123 ",
		Environment: " production ",
		Project:     " project-a ",
		Bootstrap: featureflags.GrowthBookBootstrap{
			Source:       " file://flags.json ",
			Version:      " v1 ",
			ETag:         " abc ",
			FetchedAt:    fetchedAt,
			PayloadBytes: 2048,
			FeatureCount: 12,
		},
	})
	if err != nil {
		t.Fatalf("NormalizeGrowthBookConfig() error = %v", err)
	}
	if config.APIHost != "https://cdn.growthbook.io/api/features" {
		t.Fatalf("APIHost = %q, want trimmed host without trailing slash", config.APIHost)
	}
	if config.ClientKey != "sdk-client-123" {
		t.Fatalf("ClientKey = %q, want trimmed key", config.ClientKey)
	}
	if config.Environment != "production" || config.Project != "project-a" {
		t.Fatalf("metadata was not trimmed: %+v", config)
	}
	if config.RefreshInterval != featureflags.DefaultGrowthBookRefreshInterval {
		t.Fatalf("RefreshInterval = %v, want default", config.RefreshInterval)
	}
	if config.Bootstrap.Source != "file://flags.json" ||
		config.Bootstrap.Version != "v1" ||
		config.Bootstrap.ETag != "abc" ||
		!config.Bootstrap.FetchedAt.Equal(fetchedAt) {
		t.Fatalf("Bootstrap was not normalized: %+v", config.Bootstrap)
	}
}

func TestPlanGrowthBookConfigRedactsDiagnostics(t *testing.T) {
	t.Parallel()

	plan, err := featureflags.PlanGrowthBookConfig(featureflags.GrowthBookConfig{
		APIHost:         "https://user:pass@cdn.growthbook.io/path?debug=1#secret",
		ClientKey:       "sdk-client-key-123456",
		RefreshInterval: time.Minute,
		Bootstrap: featureflags.GrowthBookBootstrap{
			Source:       "embedded",
			PayloadBytes: 128,
			FeatureCount: 2,
		},
	})
	if !errors.Is(err, featureflags.ErrGrowthBookAPIHostInvalid) {
		t.Fatalf("PlanGrowthBookConfig(credentials) error = %v, want ErrGrowthBookAPIHostInvalid", err)
	}

	plan, err = featureflags.PlanGrowthBookConfig(featureflags.GrowthBookConfig{
		APIHost:         "https://cdn.growthbook.io/path",
		ClientKey:       "sdk-client-key-123456",
		Environment:     "prod",
		Project:         "checkout",
		RefreshInterval: time.Minute,
		LocalEvaluation: true,
		Bootstrap: featureflags.GrowthBookBootstrap{
			Source:       "embedded",
			PayloadBytes: 128,
			FeatureCount: 2,
		},
	})
	if err != nil {
		t.Fatalf("PlanGrowthBookConfig() error = %v", err)
	}
	if plan.Provider != featureflags.GrowthBookProviderName {
		t.Fatalf("Provider = %q, want %q", plan.Provider, featureflags.GrowthBookProviderName)
	}
	if plan.RedactedAPIHost != "https://cdn.growthbook.io/path" {
		t.Fatalf("RedactedAPIHost = %q", plan.RedactedAPIHost)
	}
	if plan.RedactedKey != "sdk-...3456" {
		t.Fatalf("RedactedKey = %q, want sdk-...3456", plan.RedactedKey)
	}
	if !plan.LocalEvaluation {
		t.Fatal("LocalEvaluation = false, want true")
	}
	if !plan.HasBootstrap || plan.Bootstrap.FeatureCount != 2 {
		t.Fatalf("Bootstrap metadata = %+v, has=%v", plan.Bootstrap, plan.HasBootstrap)
	}
}

func TestValidateGrowthBookConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config featureflags.GrowthBookConfig
		want   error
	}{
		{
			name: "missing client key",
			config: featureflags.GrowthBookConfig{
				APIHost: "https://cdn.growthbook.io",
			},
			want: featureflags.ErrGrowthBookClientKeyMissing,
		},
		{
			name: "client key whitespace",
			config: featureflags.GrowthBookConfig{
				ClientKey: "sdk client",
			},
			want: featureflags.ErrGrowthBookClientKeyInvalid,
		},
		{
			name: "invalid api host",
			config: featureflags.GrowthBookConfig{
				APIHost:   "ftp://cdn.growthbook.io",
				ClientKey: "sdk-client",
			},
			want: featureflags.ErrGrowthBookAPIHostInvalid,
		},
		{
			name: "environment control",
			config: featureflags.GrowthBookConfig{
				ClientKey:   "sdk-client",
				Environment: "prod\x00",
			},
			want: featureflags.ErrGrowthBookEnvironmentInvalid,
		},
		{
			name: "project control",
			config: featureflags.GrowthBookConfig{
				ClientKey: "sdk-client",
				Project:   "checkout\x00",
			},
			want: featureflags.ErrGrowthBookProjectInvalid,
		},
		{
			name: "refresh interval below minimum",
			config: featureflags.GrowthBookConfig{
				ClientKey:       "sdk-client",
				RefreshInterval: time.Second,
			},
			want: featureflags.ErrGrowthBookRefreshIntervalInvalid,
		},
		{
			name: "refresh interval above maximum",
			config: featureflags.GrowthBookConfig{
				ClientKey:       "sdk-client",
				RefreshInterval: 25 * time.Hour,
			},
			want: featureflags.ErrGrowthBookRefreshIntervalInvalid,
		},
		{
			name: "bootstrap negative payload",
			config: featureflags.GrowthBookConfig{
				ClientKey: "sdk-client",
				Bootstrap: featureflags.GrowthBookBootstrap{
					PayloadBytes: -1,
				},
			},
			want: featureflags.ErrGrowthBookBootstrapInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := featureflags.ValidateGrowthBookConfig(tt.config)
			if !errors.Is(err, featureflags.ErrGrowthBookConfigInvalid) {
				t.Fatalf("ValidateGrowthBookConfig() error = %v, want ErrGrowthBookConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateGrowthBookConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestGrowthBookAPIHostAndRedactionHelpers(t *testing.T) {
	t.Parallel()

	apiHost, err := featureflags.NormalizeGrowthBookAPIHost(" HTTP://localhost:3100/features/ ")
	if err != nil {
		t.Fatalf("NormalizeGrowthBookAPIHost() error = %v", err)
	}
	if apiHost != "http://localhost:3100/features" {
		t.Fatalf("NormalizeGrowthBookAPIHost() = %q", apiHost)
	}

	if err := featureflags.ValidateGrowthBookAPIHost("https://cdn.growthbook.io?debug=1"); !errors.Is(err, featureflags.ErrGrowthBookAPIHostInvalid) {
		t.Fatalf("ValidateGrowthBookAPIHost(query) error = %v, want ErrGrowthBookAPIHostInvalid", err)
	}

	redactedHost := featureflags.RedactGrowthBookAPIHost("https://user:pass@example.test/path?key=secret#fragment")
	if redactedHost != "https://example.test/path" {
		t.Fatalf("RedactGrowthBookAPIHost() = %q", redactedHost)
	}

	tests := []struct {
		key  string
		want string
	}{
		{"", ""},
		{"short", "****"},
		{"sdk-client-key-123456", "sdk-...3456"},
	}
	for _, tt := range tests {
		if got := featureflags.RedactGrowthBookClientKey(tt.key); got != tt.want {
			t.Fatalf("RedactGrowthBookClientKey(%q) = %q, want %q", tt.key, got, tt.want)
		}
	}
}

func TestGrowthBookBootstrapValidation(t *testing.T) {
	t.Parallel()

	if err := (featureflags.GrowthBookBootstrap{}).Validate(); err != nil {
		t.Fatalf("empty bootstrap Validate() error = %v", err)
	}
	if !((featureflags.GrowthBookBootstrap{}).Empty()) {
		t.Fatal("empty bootstrap Empty() = false")
	}
	if err := (featureflags.GrowthBookBootstrap{
		Source:       "cache",
		PayloadBytes: 1,
		FeatureCount: -1,
	}).Validate(); !errors.Is(err, featureflags.ErrGrowthBookBootstrapInvalid) {
		t.Fatalf("invalid bootstrap Validate() error = %v, want ErrGrowthBookBootstrapInvalid", err)
	}
}
