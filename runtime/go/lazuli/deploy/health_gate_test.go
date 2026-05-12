package deploy_test

import (
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderHealthGateDryRunPlan(t *testing.T) {
	got, err := deploy.RenderHealthGateDryRunPlan(deploy.HealthGateConfig{
		Timeout: 3 * time.Second,
		Retry: deploy.HealthGateRetry{
			Attempts: 4,
			Interval: 500 * time.Millisecond,
		},
		Endpoints: []deploy.HealthGateEndpoint{
			deploy.HealthEndpoint("ready", "https://api.example.test/ready?verbose=1").
				ExpectBodySubstring(`"status":"ok"`),
			deploy.HealthEndpoint("live", "https://api.example.test/live").
				WithMethod("head").
				ExpectStatus(204),
		},
	})
	if err != nil {
		t.Fatalf("RenderHealthGateDryRunPlan() error = %v", err)
	}

	want := `health_gate:
  dry_run: true
  timeout: "3s"
  retry:
    attempts: 4
    interval: "500ms"
  endpoints:
    - name: "ready"
      method: "GET"
      url: "https://api.example.test/ready?verbose=1"
      expect:
        status: 200
        body_substring: "\"status\":\"ok\""
    - name: "live"
      method: "HEAD"
      url: "https://api.example.test/live"
      expect:
        status: 204
        body_substring: ""
`
	if got != want {
		t.Fatalf("RenderHealthGateDryRunPlan() =\n%s\nwant\n%s", got, want)
	}
}

func TestBuildHealthGatePlanDefaultsAndCopiesInput(t *testing.T) {
	config := deploy.HealthGateConfig{
		Endpoints: []deploy.HealthGateEndpoint{
			{URL: "http://localhost:8080/healthz"},
		},
	}

	plan, err := deploy.BuildHealthGatePlan(config)
	if err != nil {
		t.Fatalf("BuildHealthGatePlan() error = %v", err)
	}
	if !plan.DryRun {
		t.Fatal("BuildHealthGatePlan() DryRun = false, want true")
	}
	if plan.Timeout != deploy.DefaultHealthGateTimeout {
		t.Fatalf("Timeout = %s, want %s", plan.Timeout, deploy.DefaultHealthGateTimeout)
	}
	if plan.Retry.Attempts != deploy.DefaultHealthGateRetryAttempts {
		t.Fatalf("Retry.Attempts = %d, want %d", plan.Retry.Attempts, deploy.DefaultHealthGateRetryAttempts)
	}
	if plan.Retry.Interval != deploy.DefaultHealthGateRetryInterval {
		t.Fatalf("Retry.Interval = %s, want %s", plan.Retry.Interval, deploy.DefaultHealthGateRetryInterval)
	}

	got := plan.Endpoints[0]
	if got.Name != "endpoint-1" || got.Method != "GET" || got.ExpectedStatus != 200 || got.URL != "http://localhost:8080/healthz" {
		t.Fatalf("endpoint = %#v, want default name/method/status with original URL", got)
	}

	config.Endpoints[0].URL = "http://localhost:8080/changed"
	if plan.Endpoints[0].URL != "http://localhost:8080/healthz" {
		t.Fatalf("plan endpoint URL changed after input mutation: %q", plan.Endpoints[0].URL)
	}
}

func TestValidateHealthGateConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name     string
		config   deploy.HealthGateConfig
		fragment string
	}{
		{
			name:     "no endpoints",
			config:   deploy.HealthGateConfig{},
			fragment: "endpoints",
		},
		{
			name: "invalid name",
			config: deploy.HealthGateConfig{
				Endpoints: []deploy.HealthGateEndpoint{{Name: "bad name", URL: "https://api.example.test/healthz"}},
			},
			fragment: "endpoints[0].name",
		},
		{
			name: "duplicate names",
			config: deploy.HealthGateConfig{
				Endpoints: []deploy.HealthGateEndpoint{
					deploy.HealthEndpoint("ready", "https://api.example.test/ready"),
					deploy.HealthEndpoint("ready", "https://api.example.test/ready-2"),
				},
			},
			fragment: "duplicates",
		},
		{
			name: "invalid method",
			config: deploy.HealthGateConfig{
				Endpoints: []deploy.HealthGateEndpoint{{Method: "GET /x", URL: "https://api.example.test/healthz"}},
			},
			fragment: "endpoints[0].method",
		},
		{
			name: "invalid url scheme",
			config: deploy.HealthGateConfig{
				Endpoints: []deploy.HealthGateEndpoint{{URL: "ftp://api.example.test/healthz"}},
			},
			fragment: "endpoints[0].url",
		},
		{
			name: "url credentials",
			config: deploy.HealthGateConfig{
				Endpoints: []deploy.HealthGateEndpoint{{URL: "https://user:pass@api.example.test/healthz"}},
			},
			fragment: "credentials",
		},
		{
			name: "invalid expected status",
			config: deploy.HealthGateConfig{
				Endpoints: []deploy.HealthGateEndpoint{{URL: "https://api.example.test/healthz", ExpectedStatus: 99}},
			},
			fragment: "expected_status",
		},
		{
			name: "negative timeout",
			config: deploy.HealthGateConfig{
				Timeout:   -time.Second,
				Endpoints: []deploy.HealthGateEndpoint{deploy.HealthEndpoint("ready", "https://api.example.test/ready")},
			},
			fragment: "timeout",
		},
		{
			name: "negative retry attempts",
			config: deploy.HealthGateConfig{
				Retry:     deploy.HealthGateRetry{Attempts: -1},
				Endpoints: []deploy.HealthGateEndpoint{deploy.HealthEndpoint("ready", "https://api.example.test/ready")},
			},
			fragment: "retry.attempts",
		},
		{
			name: "negative retry interval",
			config: deploy.HealthGateConfig{
				Retry:     deploy.HealthGateRetry{Interval: -time.Millisecond},
				Endpoints: []deploy.HealthGateEndpoint{deploy.HealthEndpoint("ready", "https://api.example.test/ready")},
			},
			fragment: "retry.interval",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateHealthGateConfig(tt.config)
			if !errors.Is(err, deploy.ErrInvalidHealthGateConfig) {
				t.Fatalf("ValidateHealthGateConfig() error = %v, want ErrInvalidHealthGateConfig", err)
			}
			if !strings.Contains(err.Error(), tt.fragment) {
				t.Fatalf("ValidateHealthGateConfig() error = %v, want fragment %q", err, tt.fragment)
			}
		})
	}
}
