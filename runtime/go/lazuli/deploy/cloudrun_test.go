package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestPlanCloudRunServiceNormalizesAndRedacts(t *testing.T) {
	plan, err := deploy.PlanCloudRunService(deploy.CloudRunServiceConfig{
		Service:              "api",
		Region:               " Us-Central1 ",
		Project:              "Example-Project",
		Image:                "us-docker.pkg.dev/example-project/lazuli/api:build-1",
		CPUMilli:             1000,
		MemoryMiB:            512,
		AllowUnauthenticated: true,
		Env: []deploy.EnvSpec{
			deploy.EnvValue("APP_ENV", "production"),
			deploy.EnvValue("PUBLIC_URL", "https://user:placeholder@example.test/app/path?debug=true#frag"),
			deploy.SecretRefEnv("DATABASE_URL", deploy.VersionedSecret("lazuli/database-url", "active")),
		},
	})
	if err != nil {
		t.Fatalf("PlanCloudRunService() error = %v", err)
	}

	if plan.Region != "us-central1" || plan.Project != "example-project" {
		t.Fatalf("PlanCloudRunService() normalized region/project = %q/%q", plan.Region, plan.Project)
	}
	if plan.Port != deploy.CloudRunDefaultPort || plan.Concurrency != deploy.CloudRunDefaultConcurrency {
		t.Fatalf("PlanCloudRunService() defaults port/concurrency = %d/%d", plan.Port, plan.Concurrency)
	}
	if plan.Ingress != deploy.CloudRunIngressAll {
		t.Fatalf("PlanCloudRunService() ingress = %q, want default", plan.Ingress)
	}
	if len(plan.IAMBindings) != 1 || plan.IAMBindings[0].Role != "roles/run.invoker" || plan.IAMBindings[0].Member != "allUsers" {
		t.Fatalf("PlanCloudRunService() IAM bindings = %#v, want unauthenticated invoker", plan.IAMBindings)
	}

	got := map[string]deploy.CloudRunEnvBinding{}
	for _, env := range plan.Env {
		got[env.Name] = env
	}
	if got["DATABASE_URL"].RedactedValue != deploy.DefaultEnvSecretMask {
		t.Fatalf("DATABASE_URL redaction = %q, want mask", got["DATABASE_URL"].RedactedValue)
	}
	if got["PUBLIC_URL"].RedactedValue != "https://example.test/app/..." {
		t.Fatalf("PUBLIC_URL redaction = %q, want URL without userinfo/query/fragment", got["PUBLIC_URL"].RedactedValue)
	}
	if got["APP_ENV"].RedactedValue != "production" {
		t.Fatalf("APP_ENV redaction = %q, want literal", got["APP_ENV"].RedactedValue)
	}
}

func TestRenderCloudRunManifest(t *testing.T) {
	plan, err := deploy.PlanCloudRunService(deploy.CloudRunServiceConfig{
		Service:     "api",
		Region:      "us-central1",
		Project:     "example-project",
		Image:       "us-docker.pkg.dev/example-project/lazuli/api:build-1",
		Port:        9090,
		CPUMilli:    500,
		MemoryMiB:   256,
		Concurrency: 16,
		Ingress:     deploy.CloudRunIngressInternal,
		Env: []deploy.EnvSpec{
			deploy.SecretEnv("API_TOKEN", "lazuli/api-token"),
			deploy.EnvValue("APP_ENV", "production"),
		},
	})
	if err != nil {
		t.Fatalf("PlanCloudRunService() error = %v", err)
	}

	got, err := plan.RenderManifest()
	if err != nil {
		t.Fatalf("RenderManifest() error = %v", err)
	}

	want := `apiVersion: "serving.knative.dev/v1"
kind: "Service"
metadata:
  name: "api"
  annotations:
    run.googleapis.com/ingress: "internal"
spec:
  template:
    metadata:
      annotations:
        autoscaling.knative.dev/maxScale: "100"
        run.googleapis.com/cpu-throttling: "true"
    spec:
      containerConcurrency: 16
      containers:
      - image: "us-docker.pkg.dev/example-project/lazuli/api:build-1"
        ports:
        - containerPort: 9090
        resources:
          limits:
            cpu: "0.5"
            memory: "256Mi"
        env:
        - name: "API_TOKEN"
          valueFrom:
            secretKeyRef:
              name: "lazuli/api-token"
              key: "latest"
        - name: "APP_ENV"
          value: "production"
`
	if got != want {
		t.Fatalf("RenderManifest() =\n%s\nwant:\n%s", got, want)
	}
	if strings.Contains(got, deploy.DefaultEnvSecretMask) {
		t.Fatalf("RenderManifest() = %q, want secret refs not preview masks", got)
	}
}

func TestValidateCloudRunServiceConfigRejectsInvalidValues(t *testing.T) {
	err := deploy.ValidateCloudRunServiceConfig(deploy.CloudRunServiceConfig{
		Service:     "Bad_Service",
		Region:      "bad region",
		Project:     "bad",
		Image:       "image with spaces",
		Port:        70000,
		CPUMilli:    79,
		MemoryMiB:   64,
		Concurrency: 1001,
		Ingress:     "internet",
		Env: []deploy.EnvSpec{
			deploy.EnvValue("APP_ENV", "production"),
			deploy.EnvValue("APP_ENV", "duplicate"),
			deploy.SecretRefEnv("TOKEN", deploy.SecretRef{}),
		},
	})
	if !errors.Is(err, deploy.ErrInvalidCloudRunConfig) {
		t.Fatalf("ValidateCloudRunServiceConfig() error = %v, want ErrInvalidCloudRunConfig", err)
	}
	for _, fragment := range []string{
		"service",
		"region",
		"project",
		"image",
		"port",
		"cpu_milli",
		"memory_mib",
		"concurrency",
		"ingress",
		"duplicate",
		"secret_ref",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateCloudRunServiceConfig() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestCloudRunBoundsAcceptEdges(t *testing.T) {
	for _, tt := range []struct {
		name        string
		cpuMilli    int
		memoryMiB   int
		concurrency int
	}{
		{name: "minimums", cpuMilli: deploy.CloudRunMinCPUMilli, memoryMiB: deploy.CloudRunMinMemoryMiB, concurrency: deploy.CloudRunMinConcurrency},
		{name: "maximums", cpuMilli: deploy.CloudRunMaxCPUMilli, memoryMiB: deploy.CloudRunMaxMemoryMiB, concurrency: deploy.CloudRunMaxConcurrency},
	} {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateCloudRunServiceConfig(deploy.CloudRunServiceConfig{
				Service:     "api",
				Region:      "us-central1",
				Project:     "example-project",
				Image:       "us-docker.pkg.dev/example-project/lazuli/api:build-1",
				CPUMilli:    tt.cpuMilli,
				MemoryMiB:   tt.memoryMiB,
				Concurrency: tt.concurrency,
			})
			if err != nil {
				t.Fatalf("ValidateCloudRunServiceConfig() error = %v", err)
			}
		})
	}
}

func TestNormalizeCloudRunServiceConfigAppliesDefaultsWithoutValidation(t *testing.T) {
	got := deploy.NormalizeCloudRunServiceConfig(deploy.CloudRunServiceConfig{
		Service:     " Bad_Service ",
		Region:      " US-Central1 ",
		Project:     " Example-Project ",
		Image:       " bad image ",
		CPUMilli:    1,
		MemoryMiB:   1,
		Concurrency: 0,
	})

	if got.Service != "Bad_Service" || got.Region != "us-central1" || got.Project != "example-project" || got.Image != "bad image" {
		t.Fatalf("NormalizeCloudRunServiceConfig() = %#v, want trimmed/lowercased fields", got)
	}
	if got.Port != deploy.CloudRunDefaultPort || got.Concurrency != deploy.CloudRunDefaultConcurrency || got.Ingress != deploy.CloudRunIngressAll {
		t.Fatalf("NormalizeCloudRunServiceConfig() defaults = port %d concurrency %d ingress %q", got.Port, got.Concurrency, got.Ingress)
	}
}
