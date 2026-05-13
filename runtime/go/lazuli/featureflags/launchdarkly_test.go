package featureflags_test

import (
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/featureflags"
)

func TestNormalizeLaunchDarklyConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	config, err := featureflags.NormalizeLaunchDarklyConfig(featureflags.LaunchDarklyConfig{
		SDKKey:      " sdk-server-123 ",
		ClientKey:   " client-side-456 ",
		Environment: " production ",
		Project:     " checkout ",
		Tags:        []string{" web ", "checkout", "web", " release "},
		ServiceURL:  " HTTPS://relay.example.test/sdk/ ",
	})
	if err != nil {
		t.Fatalf("NormalizeLaunchDarklyConfig() error = %v", err)
	}
	if config.SDKKey != "sdk-server-123" || config.ClientKey != "client-side-456" {
		t.Fatalf("keys were not trimmed: %+v", config)
	}
	if config.Environment != "production" || config.Project != "checkout" {
		t.Fatalf("metadata was not trimmed: %+v", config)
	}
	if config.Mode != featureflags.LaunchDarklyModeStream {
		t.Fatalf("Mode = %q, want default stream", config.Mode)
	}
	if config.RefreshInterval != featureflags.DefaultLaunchDarklyRefreshInterval {
		t.Fatalf("RefreshInterval = %v, want default", config.RefreshInterval)
	}
	if got, want := strings.Join(config.Tags, ","), "checkout,release,web"; got != want {
		t.Fatalf("Tags = %q, want %q", got, want)
	}
	if config.ServiceURL != "https://relay.example.test/sdk" {
		t.Fatalf("ServiceURL = %q, want normalized URL", config.ServiceURL)
	}
}

func TestPlanLaunchDarklyConfigRedactsDiagnostics(t *testing.T) {
	t.Parallel()

	plan, err := featureflags.PlanLaunchDarklyConfig(featureflags.LaunchDarklyConfig{
		SDKKey:          "sdk-server-key-123456",
		ClientKey:       "client-side-id-abcdef",
		Environment:     "prod",
		Project:         "checkout",
		Tags:            []string{"web", "release"},
		Mode:            featureflags.LaunchDarklyModePoll,
		RefreshInterval: time.Minute,
		ServiceURL:      "https://relay.example.test/sdk",
	})
	if err != nil {
		t.Fatalf("PlanLaunchDarklyConfig() error = %v", err)
	}
	if plan.Provider != featureflags.LaunchDarklyProviderName {
		t.Fatalf("Provider = %q, want %q", plan.Provider, featureflags.LaunchDarklyProviderName)
	}
	if !plan.Polling || plan.Streaming || plan.Offline || !plan.RequiresNetwork {
		t.Fatalf("mode flags = stream:%v poll:%v offline:%v network:%v", plan.Streaming, plan.Polling, plan.Offline, plan.RequiresNetwork)
	}
	if plan.RedactedSDKKey != "sdk-...3456" {
		t.Fatalf("RedactedSDKKey = %q, want sdk-...3456", plan.RedactedSDKKey)
	}
	if plan.RedactedClientKey != "clie...cdef" {
		t.Fatalf("RedactedClientKey = %q, want clie...cdef", plan.RedactedClientKey)
	}
	if plan.RedactedServiceURL != "https://relay.example.test/sdk" {
		t.Fatalf("RedactedServiceURL = %q", plan.RedactedServiceURL)
	}
	if strings.Contains(plan.Summary, "server-key") || strings.Contains(plan.Summary, "side-id") {
		t.Fatalf("Summary leaked unredacted key material: %q", plan.Summary)
	}
	if !strings.Contains(plan.Summary, "mode=poll") || !strings.Contains(plan.Summary, "refresh=1m0s") {
		t.Fatalf("Summary missing poll metadata: %q", plan.Summary)
	}
}

func TestPlanLaunchDarklyConfigOfflineAllowsMissingKeys(t *testing.T) {
	t.Parallel()

	plan, err := featureflags.PlanLaunchDarklyConfig(featureflags.LaunchDarklyConfig{
		Environment: "local",
		Project:     "checkout",
		Mode:        " OFFLINE ",
	})
	if err != nil {
		t.Fatalf("PlanLaunchDarklyConfig() error = %v", err)
	}
	if !plan.Offline || plan.RequiresNetwork {
		t.Fatalf("offline plan = offline:%v network:%v", plan.Offline, plan.RequiresNetwork)
	}
	if strings.Contains(plan.Summary, "sdkKey=") || strings.Contains(plan.Summary, "clientKey=") {
		t.Fatalf("Summary included absent key metadata: %q", plan.Summary)
	}
}

func TestValidateLaunchDarklyConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config featureflags.LaunchDarklyConfig
		want   error
	}{
		{
			name: "missing key outside offline mode",
			config: featureflags.LaunchDarklyConfig{
				Mode: featureflags.LaunchDarklyModeStream,
			},
			want: featureflags.ErrLaunchDarklyKeyMissing,
		},
		{
			name: "sdk key whitespace",
			config: featureflags.LaunchDarklyConfig{
				SDKKey: "sdk key",
			},
			want: featureflags.ErrLaunchDarklySDKKeyInvalid,
		},
		{
			name: "client key control",
			config: featureflags.LaunchDarklyConfig{
				ClientKey: "client\x00key",
			},
			want: featureflags.ErrLaunchDarklyClientKeyInvalid,
		},
		{
			name: "environment control",
			config: featureflags.LaunchDarklyConfig{
				SDKKey:      "sdk-key",
				Environment: "prod\x00",
			},
			want: featureflags.ErrLaunchDarklyEnvironmentInvalid,
		},
		{
			name: "project control",
			config: featureflags.LaunchDarklyConfig{
				SDKKey:  "sdk-key",
				Project: "checkout\x00",
			},
			want: featureflags.ErrLaunchDarklyProjectInvalid,
		},
		{
			name: "tag control",
			config: featureflags.LaunchDarklyConfig{
				SDKKey: "sdk-key",
				Tags:   []string{"web\x00"},
			},
			want: featureflags.ErrLaunchDarklyTagInvalid,
		},
		{
			name: "invalid mode",
			config: featureflags.LaunchDarklyConfig{
				SDKKey: "sdk-key",
				Mode:   "daemon",
			},
			want: featureflags.ErrLaunchDarklyModeInvalid,
		},
		{
			name: "poll interval below minimum",
			config: featureflags.LaunchDarklyConfig{
				SDKKey:          "sdk-key",
				Mode:            featureflags.LaunchDarklyModePoll,
				RefreshInterval: time.Second,
			},
			want: featureflags.ErrLaunchDarklyRefreshIntervalInvalid,
		},
		{
			name: "poll interval above maximum",
			config: featureflags.LaunchDarklyConfig{
				SDKKey:          "sdk-key",
				Mode:            featureflags.LaunchDarklyModePoll,
				RefreshInterval: 25 * time.Hour,
			},
			want: featureflags.ErrLaunchDarklyRefreshIntervalInvalid,
		},
		{
			name: "invalid service url",
			config: featureflags.LaunchDarklyConfig{
				SDKKey:     "sdk-key",
				ServiceURL: "ftp://relay.example.test",
			},
			want: featureflags.ErrLaunchDarklyServiceURLInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := featureflags.ValidateLaunchDarklyConfig(tt.config)
			if !errors.Is(err, featureflags.ErrLaunchDarklyConfigInvalid) {
				t.Fatalf("ValidateLaunchDarklyConfig() error = %v, want ErrLaunchDarklyConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateLaunchDarklyConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestLaunchDarklyServiceURLAndRedactionHelpers(t *testing.T) {
	t.Parallel()

	serviceURL, err := featureflags.NormalizeLaunchDarklyServiceURL(" HTTP://localhost:8765/sdk/ ")
	if err != nil {
		t.Fatalf("NormalizeLaunchDarklyServiceURL() error = %v", err)
	}
	if serviceURL != "http://localhost:8765/sdk" {
		t.Fatalf("NormalizeLaunchDarklyServiceURL() = %q", serviceURL)
	}

	if err := featureflags.ValidateLaunchDarklyServiceURL("https://relay.example.test?key=secret"); !errors.Is(err, featureflags.ErrLaunchDarklyServiceURLInvalid) {
		t.Fatalf("ValidateLaunchDarklyServiceURL(query) error = %v, want ErrLaunchDarklyServiceURLInvalid", err)
	}

	redactedURL := featureflags.RedactLaunchDarklyServiceURL("https://user:pass@example.test/sdk?key=secret#fragment")
	if redactedURL != "https://example.test/sdk" {
		t.Fatalf("RedactLaunchDarklyServiceURL() = %q", redactedURL)
	}

	tests := []struct {
		key  string
		want string
	}{
		{"", ""},
		{"short", "****"},
		{"sdk-server-key-123456", "sdk-...3456"},
	}
	for _, tt := range tests {
		if got := featureflags.RedactLaunchDarklySDKKey(tt.key); got != tt.want {
			t.Fatalf("RedactLaunchDarklySDKKey(%q) = %q, want %q", tt.key, got, tt.want)
		}
	}
}
