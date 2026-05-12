package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderEnvPreviewRedactsSecretsAndShowsRefs(t *testing.T) {
	specs := []deploy.EnvSpec{
		deploy.SecretRefEnv("DATABASE_URL", deploy.VersionedSecret("lazuli/database-url", "active")),
		deploy.EnvValue("GREETING", "hello world"),
		deploy.EnvValue("APP_ENV", "production"),
		deploy.EnvValue("EMPTY", ""),
	}

	got, err := deploy.RenderEnvPreview(specs)
	if err != nil {
		t.Fatalf("RenderEnvPreview() error = %v", err)
	}

	want := "APP_ENV=production\n" +
		"DATABASE_URL=[REDACTED] # secret: lazuli/database-url@active\n" +
		"EMPTY=\n" +
		"GREETING=\"hello world\"\n"
	if got != want {
		t.Fatalf("RenderEnvPreview() = %q, want %q", got, want)
	}
}

func TestRenderEnvDotenvExcludesSecretBackedVars(t *testing.T) {
	specs := []deploy.EnvSpec{
		deploy.SecretEnv("DATABASE_URL", "lazuli/database-url"),
		deploy.EnvValue("GREETING", "hello world"),
		deploy.EnvValue("APP_ENV", "production"),
		deploy.EnvValue("EMPTY", ""),
	}

	got, err := deploy.RenderEnvDotenv(specs)
	if err != nil {
		t.Fatalf("RenderEnvDotenv() error = %v", err)
	}

	want := "APP_ENV=production\n" +
		"EMPTY=\n" +
		"GREETING=\"hello world\"\n"
	if got != want {
		t.Fatalf("RenderEnvDotenv() = %q, want %q", got, want)
	}
	for _, fragment := range []string{"DATABASE_URL", "lazuli/database-url", "[REDACTED]"} {
		if strings.Contains(got, fragment) {
			t.Fatalf("RenderEnvDotenv() = %q, want no secret fragment %q", got, fragment)
		}
	}
}

func TestRenderEnvPreviewWithMaskUsesCustomMask(t *testing.T) {
	got, err := deploy.RenderEnvPreviewWithMask([]deploy.EnvSpec{
		deploy.SecretEnv("API_TOKEN", "api-token"),
	}, "***")
	if err != nil {
		t.Fatalf("RenderEnvPreviewWithMask() error = %v", err)
	}

	want := "API_TOKEN=*** # secret: api-token\n"
	if got != want {
		t.Fatalf("RenderEnvPreviewWithMask() = %q, want %q", got, want)
	}
}

func TestValidateEnvSpecsRejectsInvalidSpecs(t *testing.T) {
	err := deploy.ValidateEnvSpecs([]deploy.EnvSpec{
		deploy.EnvValue("APP_ENV", "production"),
		deploy.EnvValue("APP_ENV", "duplicate"),
		deploy.EnvValue("1BAD", "bad"),
		deploy.SecretRefEnv("API_TOKEN", deploy.SecretRef{Version: "active"}),
		deploy.SecretEnv("MISSING_SECRET", ""),
		{
			Name:      "DATABASE_URL",
			Value:     "postgres://secret@example.test/app",
			SecretRef: deploy.Secret("database-url"),
		},
		deploy.SecretEnv("COOKIE_SECRET", "bad ref"),
	})
	if !errors.Is(err, deploy.ErrInvalidEnvSpec) {
		t.Fatalf("ValidateEnvSpecs() error = %v, want ErrInvalidEnvSpec", err)
	}
	for _, fragment := range []string{
		"duplicate",
		"1BAD",
		"secret_ref",
		"name is required",
		"value must be empty",
		"bad ref",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateEnvSpecs() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestRenderEnvPreviewNormalizesEnvSecretRefs(t *testing.T) {
	got, err := deploy.RenderEnvPreview([]deploy.EnvSpec{
		deploy.SecretEnv("API_TOKEN", "env.API_TOKEN"),
	})
	if err != nil {
		t.Fatalf("RenderEnvPreview() error = %v", err)
	}

	want := "API_TOKEN=[REDACTED] # secret: API_TOKEN\n"
	if got != want {
		t.Fatalf("RenderEnvPreview() = %q, want %q", got, want)
	}
}
