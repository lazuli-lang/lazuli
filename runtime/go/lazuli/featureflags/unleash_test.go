package featureflags_test

import (
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/featureflags"
)

func TestNormalizeUnleashConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	fetchedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	config, err := featureflags.NormalizeUnleashConfig(featureflags.UnleashConfig{
		APIHost:     " HTTPS://flags.example.test/api/ ",
		ClientToken: " client-token-placeholder-123456 ",
		AppName:     " checkout-api ",
		Environment: " production ",
		Project:     " checkout ",
		Bootstrap: featureflags.UnleashBootstrap{
			Source:       " file://unleash.json ",
			Version:      " v1 ",
			ETag:         " etag-a ",
			FetchedAt:    fetchedAt,
			PayloadBytes: 4096,
			FeatureCount: 8,
		},
	})
	if err != nil {
		t.Fatalf("NormalizeUnleashConfig() error = %v", err)
	}
	if config.APIHost != "https://flags.example.test/api" {
		t.Fatalf("APIHost = %q, want normalized URL", config.APIHost)
	}
	if config.ClientToken != "client-token-placeholder-123456" {
		t.Fatalf("ClientToken = %q, want trimmed token", config.ClientToken)
	}
	if config.AppName != "checkout-api" || config.Environment != "production" || config.Project != "checkout" {
		t.Fatalf("metadata was not trimmed: %+v", config)
	}
	if config.RefreshInterval != featureflags.DefaultUnleashRefreshInterval {
		t.Fatalf("RefreshInterval = %v, want default", config.RefreshInterval)
	}
	if config.Bootstrap.Source != "file://unleash.json" ||
		config.Bootstrap.Version != "v1" ||
		config.Bootstrap.ETag != "etag-a" ||
		!config.Bootstrap.FetchedAt.Equal(fetchedAt) {
		t.Fatalf("Bootstrap was not normalized: %+v", config.Bootstrap)
	}
}

func TestPlanUnleashConfigRedactsDiagnostics(t *testing.T) {
	t.Parallel()

	plan, err := featureflags.PlanUnleashConfig(featureflags.UnleashConfig{
		APIHost:         "https://flags.example.test/api",
		ClientToken:     "client-token-placeholder-123456",
		AppName:         "checkout-api",
		Environment:     "prod",
		Project:         "checkout",
		RefreshInterval: time.Minute,
		Bootstrap: featureflags.UnleashBootstrap{
			Source:       "embedded",
			PayloadBytes: 256,
			FeatureCount: 3,
		},
	})
	if err != nil {
		t.Fatalf("PlanUnleashConfig() error = %v", err)
	}
	if plan.Provider != featureflags.UnleashProviderName {
		t.Fatalf("Provider = %q, want %q", plan.Provider, featureflags.UnleashProviderName)
	}
	if plan.RedactedAPIHost != "https://flags.example.test/api" {
		t.Fatalf("RedactedAPIHost = %q", plan.RedactedAPIHost)
	}
	if plan.RedactedToken != "clie...3456" {
		t.Fatalf("RedactedToken = %q, want clie...3456", plan.RedactedToken)
	}
	if !plan.HasBootstrap || plan.Bootstrap.FeatureCount != 3 {
		t.Fatalf("Bootstrap metadata = %+v, has=%v", plan.Bootstrap, plan.HasBootstrap)
	}
	if strings.Contains(plan.Summary, "token-placeholder") {
		t.Fatalf("Summary leaked unredacted token material: %q", plan.Summary)
	}
	if !strings.Contains(plan.Summary, "provider=unleash") ||
		!strings.Contains(plan.Summary, "refresh=1m0s") ||
		!strings.Contains(plan.Summary, "bootstrap=true") {
		t.Fatalf("Summary missing expected metadata: %q", plan.Summary)
	}
}

func TestValidateUnleashConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	valid := featureflags.UnleashConfig{
		APIHost:         "https://flags.example.test/api",
		ClientToken:     "client-token-placeholder",
		RefreshInterval: time.Minute,
	}

	tests := []struct {
		name   string
		config featureflags.UnleashConfig
		want   error
	}{
		{
			name: "missing api host",
			config: featureflags.UnleashConfig{
				ClientToken: "client-token-placeholder",
			},
			want: featureflags.ErrUnleashAPIHostMissing,
		},
		{
			name: "invalid api host",
			config: featureflags.UnleashConfig{
				APIHost:     "ftp://flags.example.test",
				ClientToken: "client-token-placeholder",
			},
			want: featureflags.ErrUnleashAPIHostInvalid,
		},
		{
			name: "missing client token",
			config: featureflags.UnleashConfig{
				APIHost: "https://flags.example.test/api",
			},
			want: featureflags.ErrUnleashClientTokenMissing,
		},
		{
			name: "client token whitespace",
			config: featureflags.UnleashConfig{
				APIHost:     "https://flags.example.test/api",
				ClientToken: "client token",
			},
			want: featureflags.ErrUnleashClientTokenInvalid,
		},
		{
			name: "app name control",
			config: func() featureflags.UnleashConfig {
				config := valid
				config.AppName = "checkout\x00api"
				return config
			}(),
			want: featureflags.ErrUnleashAppNameInvalid,
		},
		{
			name: "environment control",
			config: func() featureflags.UnleashConfig {
				config := valid
				config.Environment = "prod\x00"
				return config
			}(),
			want: featureflags.ErrUnleashEnvironmentInvalid,
		},
		{
			name: "project control",
			config: func() featureflags.UnleashConfig {
				config := valid
				config.Project = "checkout\x00"
				return config
			}(),
			want: featureflags.ErrUnleashProjectInvalid,
		},
		{
			name: "refresh interval below minimum",
			config: func() featureflags.UnleashConfig {
				config := valid
				config.RefreshInterval = time.Second
				return config
			}(),
			want: featureflags.ErrUnleashRefreshIntervalInvalid,
		},
		{
			name: "refresh interval above maximum",
			config: func() featureflags.UnleashConfig {
				config := valid
				config.RefreshInterval = 25 * time.Hour
				return config
			}(),
			want: featureflags.ErrUnleashRefreshIntervalInvalid,
		},
		{
			name: "bootstrap negative payload",
			config: func() featureflags.UnleashConfig {
				config := valid
				config.Bootstrap = featureflags.UnleashBootstrap{PayloadBytes: -1}
				return config
			}(),
			want: featureflags.ErrUnleashBootstrapInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := featureflags.ValidateUnleashConfig(tt.config)
			if !errors.Is(err, featureflags.ErrUnleashConfigInvalid) {
				t.Fatalf("ValidateUnleashConfig() error = %v, want ErrUnleashConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateUnleashConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestUnleashAPIHostAndRedactionHelpers(t *testing.T) {
	t.Parallel()

	apiHost, err := featureflags.NormalizeUnleashAPIHost(" HTTP://localhost:4242/api/client/ ")
	if err != nil {
		t.Fatalf("NormalizeUnleashAPIHost() error = %v", err)
	}
	if apiHost != "http://localhost:4242/api/client" {
		t.Fatalf("NormalizeUnleashAPIHost() = %q", apiHost)
	}

	if err := featureflags.ValidateUnleashAPIHost("https://flags.example.test/api?token=placeholder"); !errors.Is(err, featureflags.ErrUnleashAPIHostInvalid) {
		t.Fatalf("ValidateUnleashAPIHost(query) error = %v, want ErrUnleashAPIHostInvalid", err)
	}

	redactedHost := featureflags.RedactUnleashAPIHost("https://user:pass@example.test/api?token=placeholder#fragment")
	if redactedHost != "https://example.test/api" {
		t.Fatalf("RedactUnleashAPIHost() = %q", redactedHost)
	}

	tests := []struct {
		token string
		want  string
	}{
		{"", ""},
		{"short", "****"},
		{"client-token-placeholder-123456", "clie...3456"},
	}
	for _, tt := range tests {
		if got := featureflags.RedactUnleashClientToken(tt.token); got != tt.want {
			t.Fatalf("RedactUnleashClientToken(%q) = %q, want %q", tt.token, got, tt.want)
		}
	}
}

func TestUnleashBootstrapValidation(t *testing.T) {
	t.Parallel()

	if err := (featureflags.UnleashBootstrap{}).Validate(); err != nil {
		t.Fatalf("empty bootstrap Validate() error = %v", err)
	}
	if !((featureflags.UnleashBootstrap{}).Empty()) {
		t.Fatal("empty bootstrap Empty() = false")
	}
	if err := (featureflags.UnleashBootstrap{
		Source:       "cache",
		PayloadBytes: 1,
		FeatureCount: -1,
	}).Validate(); !errors.Is(err, featureflags.ErrUnleashBootstrapInvalid) {
		t.Fatalf("invalid bootstrap Validate() error = %v, want ErrUnleashBootstrapInvalid", err)
	}
}
