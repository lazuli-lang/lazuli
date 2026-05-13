package featureflags_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/featureflags"
)

func TestNormalizeOpenFeatureConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	config, err := featureflags.NormalizeOpenFeatureConfig(featureflags.OpenFeatureConfig{
		ProviderName:      " custom-provider ",
		ProviderVersion:   " v1.2.3 ",
		EvaluationContext: map[string]string{" tenant ": " acme ", " ": "ignored"},
		HookNames:         []string{" audit ", "trace", "audit", " meter "},
		EndpointURL:       " HTTPS://flags.example.test/api/ ",
	})
	if err != nil {
		t.Fatalf("NormalizeOpenFeatureConfig() error = %v", err)
	}
	if config.ProviderName != "custom-provider" || config.ProviderVersion != "v1.2.3" {
		t.Fatalf("provider metadata was not trimmed: %+v", config)
	}
	if got, want := config.EvaluationContext["tenant"], "acme"; got != want {
		t.Fatalf("EvaluationContext[tenant] = %q, want %q", got, want)
	}
	if _, ok := config.EvaluationContext[""]; ok {
		t.Fatalf("EvaluationContext kept empty attribute key: %+v", config.EvaluationContext)
	}
	if got, want := strings.Join(config.HookNames, ","), "audit,meter,trace"; got != want {
		t.Fatalf("HookNames = %q, want %q", got, want)
	}
	if config.Mode != featureflags.OpenFeatureModeDefault {
		t.Fatalf("Mode = %q, want default", config.Mode)
	}
	if config.RefreshInterval != featureflags.DefaultOpenFeatureRefreshInterval {
		t.Fatalf("RefreshInterval = %v, want default", config.RefreshInterval)
	}
	if config.EndpointURL != "https://flags.example.test/api" {
		t.Fatalf("EndpointURL = %q, want normalized URL", config.EndpointURL)
	}
}

func TestPlanOpenFeatureConfigRedactsDiagnostics(t *testing.T) {
	t.Parallel()

	plan, err := featureflags.PlanOpenFeatureConfig(featureflags.OpenFeatureConfig{
		ProviderName:    "go-provider",
		ProviderVersion: "1.0.0",
		EvaluationContext: map[string]string{
			"tenant":       "tenant-a",
			"apiToken":     "placeholder-token-value",
			"profileURL":   "https://user:pass@app.example.test/profile?debug=yes#frag",
			"targetingKey": "user-123",
		},
		HookNames:           []string{"trace", "audit"},
		Mode:                featureflags.OpenFeatureModeDefault,
		RefreshInterval:     time.Minute,
		EndpointURL:         "https://relay.example.test/ofrep",
		DefaultFlagFallback: true,
	})
	if err != nil {
		t.Fatalf("PlanOpenFeatureConfig() error = %v", err)
	}
	if plan.ProviderName != "go-provider" || plan.ProviderVersion != "1.0.0" {
		t.Fatalf("provider metadata = %q %q", plan.ProviderName, plan.ProviderVersion)
	}
	if plan.Offline || !plan.RequiresNetwork {
		t.Fatalf("mode flags = offline:%v network:%v", plan.Offline, plan.RequiresNetwork)
	}
	if plan.RedactedEvaluationContext["apiToken"] != "[redacted]" {
		t.Fatalf("apiToken was not redacted: %+v", plan.RedactedEvaluationContext)
	}
	if got, want := plan.RedactedEvaluationContext["profileURL"], "https://app.example.test/profile"; got != want {
		t.Fatalf("profileURL redaction = %q, want %q", got, want)
	}
	if plan.RedactedEndpointURL != "https://relay.example.test/ofrep" {
		t.Fatalf("RedactedEndpointURL = %q", plan.RedactedEndpointURL)
	}
	for _, leaked := range []string{"placeholder-token-value", "user:pass", "debug=yes", "user-123"} {
		if strings.Contains(plan.Summary, leaked) {
			t.Fatalf("Summary leaked %q: %q", leaked, plan.Summary)
		}
	}
	for _, want := range []string{
		"provider=go-provider",
		"mode=default",
		"context=apiToken,profileURL,targetingKey,tenant",
		"endpointURL=https://relay.example.test/ofrep",
		"defaultFallback=true",
	} {
		if !strings.Contains(plan.Summary, want) {
			t.Fatalf("Summary missing %q: %q", want, plan.Summary)
		}
	}

	plan.EvaluationContext["tenant"] = "mutated"
	again, err := featureflags.PlanOpenFeatureConfig(featureflags.OpenFeatureConfig{
		ProviderName:        "go-provider",
		EvaluationContext:   map[string]string{"tenant": "tenant-a"},
		RefreshInterval:     time.Minute,
		DefaultFlagFallback: true,
	})
	if err != nil {
		t.Fatalf("PlanOpenFeatureConfig(copy check) error = %v", err)
	}
	if again.EvaluationContext["tenant"] != "tenant-a" {
		t.Fatalf("PlanOpenFeatureConfig did not return defensive context copies: %+v", again.EvaluationContext)
	}
}

func TestPlanOpenFeatureConfigOfflineAllowsMissingEndpoint(t *testing.T) {
	t.Parallel()

	plan, err := featureflags.PlanOpenFeatureConfig(featureflags.OpenFeatureConfig{
		Mode:            " OFFLINE ",
		RefreshInterval: time.Minute,
	})
	if err != nil {
		t.Fatalf("PlanOpenFeatureConfig() error = %v", err)
	}
	if plan.ProviderName != featureflags.DefaultOpenFeatureProviderName {
		t.Fatalf("ProviderName = %q, want default", plan.ProviderName)
	}
	if !plan.Offline || plan.RequiresNetwork {
		t.Fatalf("offline plan = offline:%v network:%v", plan.Offline, plan.RequiresNetwork)
	}
	if !strings.Contains(plan.Summary, "mode=offline") || strings.Contains(plan.Summary, "endpointURL=") {
		t.Fatalf("offline Summary = %q", plan.Summary)
	}
}

func TestValidateOpenFeatureConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config featureflags.OpenFeatureConfig
		want   error
	}{
		{
			name: "provider name whitespace",
			config: featureflags.OpenFeatureConfig{
				ProviderName: "custom provider",
			},
			want: featureflags.ErrOpenFeatureProviderNameInvalid,
		},
		{
			name: "provider version control",
			config: featureflags.OpenFeatureConfig{
				ProviderVersion: "1.0\x00",
			},
			want: featureflags.ErrOpenFeatureProviderVersionInvalid,
		},
		{
			name: "context key whitespace",
			config: featureflags.OpenFeatureConfig{
				EvaluationContext: map[string]string{"tenant id": "tenant-a"},
			},
			want: featureflags.ErrOpenFeatureEvaluationContextInvalid,
		},
		{
			name: "context value control",
			config: featureflags.OpenFeatureConfig{
				EvaluationContext: map[string]string{"tenant": "tenant-a\x00"},
			},
			want: featureflags.ErrOpenFeatureEvaluationContextInvalid,
		},
		{
			name: "hook name whitespace",
			config: featureflags.OpenFeatureConfig{
				HookNames: []string{"audit hook"},
			},
			want: featureflags.ErrOpenFeatureHookNameInvalid,
		},
		{
			name: "invalid mode",
			config: featureflags.OpenFeatureConfig{
				Mode: "stream",
			},
			want: featureflags.ErrOpenFeatureModeInvalid,
		},
		{
			name: "refresh interval below minimum",
			config: featureflags.OpenFeatureConfig{
				RefreshInterval: time.Second,
			},
			want: featureflags.ErrOpenFeatureRefreshIntervalInvalid,
		},
		{
			name: "refresh interval above maximum",
			config: featureflags.OpenFeatureConfig{
				RefreshInterval: 25 * time.Hour,
			},
			want: featureflags.ErrOpenFeatureRefreshIntervalInvalid,
		},
		{
			name: "invalid endpoint url",
			config: featureflags.OpenFeatureConfig{
				EndpointURL: "ftp://relay.example.test",
			},
			want: featureflags.ErrOpenFeatureEndpointURLInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := featureflags.ValidateOpenFeatureConfig(tt.config)
			if !errors.Is(err, featureflags.ErrOpenFeatureConfigInvalid) {
				t.Fatalf("ValidateOpenFeatureConfig() error = %v, want ErrOpenFeatureConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateOpenFeatureConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestOpenFeatureEndpointURLAndRedactionHelpers(t *testing.T) {
	t.Parallel()

	endpointURL, err := featureflags.NormalizeOpenFeatureEndpointURL(" HTTP://localhost:8080/ofrep/ ")
	if err != nil {
		t.Fatalf("NormalizeOpenFeatureEndpointURL() error = %v", err)
	}
	if endpointURL != "http://localhost:8080/ofrep" {
		t.Fatalf("NormalizeOpenFeatureEndpointURL() = %q", endpointURL)
	}

	if err := featureflags.ValidateOpenFeatureEndpointURL("https://relay.example.test?credential=value"); !errors.Is(err, featureflags.ErrOpenFeatureEndpointURLInvalid) {
		t.Fatalf("ValidateOpenFeatureEndpointURL(query) error = %v, want ErrOpenFeatureEndpointURLInvalid", err)
	}

	redactedURL := featureflags.RedactOpenFeatureEndpointURL("https://user:pass@example.test/sdk?credential=value#fragment")
	if redactedURL != "https://example.test/sdk" {
		t.Fatalf("RedactOpenFeatureEndpointURL() = %q", redactedURL)
	}
}

func TestOpenFeatureEvaluationContextAndHookHelpersAreDeterministic(t *testing.T) {
	t.Parallel()

	context := featureflags.NormalizeOpenFeatureEvaluationContext(map[string]string{
		" region ": " us-east-1 ",
		"tenant":   " tenant-a ",
	})
	wantContext := map[string]string{"region": "us-east-1", "tenant": "tenant-a"}
	if !reflect.DeepEqual(context, wantContext) {
		t.Fatalf("NormalizeOpenFeatureEvaluationContext() = %+v, want %+v", context, wantContext)
	}

	hooks := featureflags.NormalizeOpenFeatureHookNames([]string{"meter", " audit ", "meter", "trace"})
	if got, want := strings.Join(hooks, ","), "audit,meter,trace"; got != want {
		t.Fatalf("NormalizeOpenFeatureHookNames() = %q, want %q", got, want)
	}

	redacted := featureflags.RedactOpenFeatureEvaluationContext(map[string]string{
		"clientKey": "invalid-placeholder-key",
		"homeURL":   "https://user:pass@example.test/path?debug=1#frag",
		"region":    "us-east-1",
	})
	if redacted["clientKey"] != "[redacted]" ||
		redacted["homeURL"] != "https://example.test/path" ||
		redacted["region"] != "us-east-1" {
		t.Fatalf("RedactOpenFeatureEvaluationContext() = %+v", redacted)
	}
}
