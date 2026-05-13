package deploy_test

import (
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestPlanFlyAppNormalizesAndRedacts(t *testing.T) {
	plan, err := deploy.PlanFlyApp(deploy.FlyAppConfig{
		App:           " Lazuli-Api ",
		PrimaryRegion: " GRU ",
		Image:         "registry.example.test/lazuli/api:build-1",
		Port:          9090,
		Env: []deploy.EnvSpec{
			deploy.EnvValue("PUBLIC_URL", "https://user:placeholder@example.test/app/path?debug=true#frag"),
			deploy.SecretRefEnv("DATABASE_URL", deploy.VersionedSecret("lazuli/database-url", "active")),
			deploy.EnvValue("APP_ENV", "production"),
		},
		Resources: deploy.FlyResources{CPUKind: " Performance ", CPUs: 2, MemoryMB: 1024},
		Processes: []deploy.FlyProcessGroup{
			{Name: "worker", Command: []string{"/app/app", "worker"}, Count: 2},
			{Name: "app", Command: []string{"/app/app", "serve", "--port", "9090"}},
		},
		HealthChecks: []deploy.FlyHealthCheck{
			{Name: "ready", Method: "get", Path: "/readyz", Interval: 15 * time.Second},
		},
	})
	if err != nil {
		t.Fatalf("PlanFlyApp() error = %v", err)
	}

	if plan.App != "lazuli-api" || plan.PrimaryRegion != "gru" {
		t.Fatalf("PlanFlyApp() normalized app/region = %q/%q", plan.App, plan.PrimaryRegion)
	}
	if plan.Resources.CPUKind != "performance" || plan.Resources.MemoryMB != 1024 {
		t.Fatalf("top-level resources = %#v, want normalized resources", plan.Resources)
	}
	if plan.Processes[0].Name != "app" || plan.Processes[1].Name != "worker" {
		t.Fatalf("process order = %#v, want sorted by name", plan.Processes)
	}
	if plan.Processes[1].Count != 2 || plan.Processes[1].Resources.MemoryMB != 1024 {
		t.Fatalf("worker process metadata = %#v, want count and inherited resources", plan.Processes[1])
	}
	if len(plan.Secrets) != 1 || plan.Secrets[0].Name != "DATABASE_URL" {
		t.Fatalf("secrets = %#v, want DATABASE_URL secret metadata", plan.Secrets)
	}
	if len(plan.Env) != 2 {
		t.Fatalf("env = %#v, want literal env only", plan.Env)
	}
	envByName := map[string]deploy.FlyEnvBinding{}
	for _, env := range plan.Env {
		envByName[env.Name] = env
	}
	if envByName["PUBLIC_URL"].RedactedValue != "https://example.test/app/..." {
		t.Fatalf("PUBLIC_URL redaction = %q, want URL redacted", envByName["PUBLIC_URL"].RedactedValue)
	}
	if plan.HealthChecks[0].Method != "GET" || plan.HealthChecks[0].Protocol != deploy.FlyHealthProtocolHTTP {
		t.Fatalf("health check = %#v, want normalized method/protocol", plan.HealthChecks[0])
	}

	summary := plan.RedactedSummary()
	if summary.Env[1].Value != "https://example.test/app/..." {
		t.Fatalf("summary env = %#v, want redacted URL value", summary.Env)
	}
	if len(summary.Secrets) != 1 || summary.Secrets[0].SecretRef.Name != "lazuli/database-url" {
		t.Fatalf("summary secrets = %#v, want secret refs without values", summary.Secrets)
	}
	if summary.Processes[0].Name != "app" || summary.Processes[0].Count != 1 {
		t.Fatalf("summary processes = %#v, want metadata without command", summary.Processes)
	}
}

func TestRenderFlyTOML(t *testing.T) {
	plan, err := deploy.PlanFlyApp(deploy.FlyAppConfig{
		App:           "lazuli-api",
		PrimaryRegion: "iad",
		Image:         "registry.example.test/lazuli/api:build-1",
		Port:          8080,
		Env: []deploy.EnvSpec{
			deploy.SecretEnv("DATABASE_URL", "lazuli/database-url"),
			deploy.EnvValue("APP_ENV", "production"),
			deploy.EnvValue("GREETING", "hello world"),
		},
		Resources: deploy.FlyResources{CPUKind: deploy.FlyDefaultCPUKind, CPUs: 1, MemoryMB: 512},
		Processes: []deploy.FlyProcessGroup{
			{Name: "app", Command: []string{"/app/app", "serve"}},
		},
		HealthChecks: []deploy.FlyHealthCheck{
			{Name: "ready", Path: "/healthz", Timeout: 3 * time.Second},
		},
	})
	if err != nil {
		t.Fatalf("PlanFlyApp() error = %v", err)
	}

	got, err := plan.RenderTOML()
	if err != nil {
		t.Fatalf("RenderTOML() error = %v", err)
	}

	want := `app = "lazuli-api"
primary_region = "iad"

[build]
image = "registry.example.test/lazuli/api:build-1"

[env]
"APP_ENV" = "production"
"GREETING" = "hello world"

[processes]
"app" = "/app/app serve"

[http_service]
internal_port = 8080
force_https = true
auto_stop_machines = "stop"
auto_start_machines = true
min_machines_running = 0
processes = ["app"]

  [[http_service.checks]]
  name = "ready"
  protocol = "http"
  method = "GET"
  path = "/healthz"
  port = 8080
  expected_status = 200
  interval = "10s"
  timeout = "3s"
  grace_period = "5s"


[[vm]]
memory = "512mb"
cpu_kind = "shared"
cpus = 1
processes = ["app"]
`
	if got != want {
		t.Fatalf("RenderTOML() =\n%s\nwant:\n%s", got, want)
	}
	if strings.Contains(got, "DATABASE_URL") || strings.Contains(got, deploy.DefaultEnvSecretMask) {
		t.Fatalf("RenderTOML() = %q, want no secret env names or masks", got)
	}
}

func TestValidateFlyAppConfigRejectsInvalidValues(t *testing.T) {
	err := deploy.ValidateFlyAppConfig(deploy.FlyAppConfig{
		App:           "bad_app",
		PrimaryRegion: "bad-region",
		Image:         "image with spaces",
		Port:          70000,
		Resources:     deploy.FlyResources{CPUKind: "tiny", CPUs: 0, MemoryMB: 64},
		Env: []deploy.EnvSpec{
			deploy.EnvValue("APP_ENV", "production"),
			deploy.EnvValue("APP_ENV", "duplicate"),
			deploy.SecretRefEnv("TOKEN", deploy.SecretRef{}),
		},
		Processes: []deploy.FlyProcessGroup{
			{Name: "web process", Command: []string{"/app/app"}},
			{Name: "web", Command: []string{}},
			{Name: "worker", Command: []string{"/app/app"}, Count: -1, Resources: deploy.FlyResources{CPUKind: "shared", CPUs: 65, MemoryMB: 128}},
		},
		HealthChecks: []deploy.FlyHealthCheck{
			{Name: "ready", Protocol: "tcp", Method: "TRACE", Path: "healthz", Port: -1, ExpectedStatus: 99, Interval: -time.Second},
			{Name: "ready", Path: "/readyz"},
		},
	})
	if !errors.Is(err, deploy.ErrInvalidFlyConfig) {
		t.Fatalf("ValidateFlyAppConfig() error = %v, want ErrInvalidFlyConfig", err)
	}
	for _, fragment := range []string{
		"app",
		"primary_region",
		"image",
		"port",
		"cpu_kind",
		"memory_mb",
		"duplicate",
		"secret_ref",
		"processes[0].name",
		"processes[1].command",
		"processes[2].count",
		"processes[2].resources.cpus",
		"health_checks[0].protocol",
		"health_checks[0].method",
		"health_checks[0].path",
		"health_checks[0].port",
		"health_checks[0].expected_status",
		"health_checks[0].interval",
		"health_checks[1].name",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateFlyAppConfig() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestNormalizeFlyAppConfigAppliesDefaultsWithoutValidation(t *testing.T) {
	got := deploy.NormalizeFlyAppConfig(deploy.FlyAppConfig{
		App:   " Bad_App ",
		Image: " bad image ",
		Resources: deploy.FlyResources{
			CPUKind: " Performance ",
		},
	})

	if got.App != "bad_app" || got.PrimaryRegion != deploy.FlyDefaultRegion || got.Image != "bad image" {
		t.Fatalf("NormalizeFlyAppConfig() = %#v, want trimmed/lowercased fields", got)
	}
	if got.Port != deploy.FlyDefaultPort || got.Resources.CPUKind != "performance" || got.Resources.CPUs != deploy.FlyDefaultCPUs || got.Resources.MemoryMB != deploy.FlyDefaultMemoryMB {
		t.Fatalf("NormalizeFlyAppConfig() defaults = %#v, want port/resources defaults", got)
	}
}
