package config_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/config"
)

func TestSecretMapBuildsSchemaRedactionHintsAndRefMaps(t *testing.T) {
	mappings := config.SecretMap{
		config.RequiredSecretRefEnv("database.url", "DATABASE_URL", config.SecretRef{
			Name:    "lazuli/database-url",
			Version: "active",
		}),
		config.OptionalSecretEnv("stripe.webhook_secret", "STRIPE_WEBHOOK_SECRET"),
	}

	schema, err := mappings.Schema()
	if err != nil {
		t.Fatalf("Schema() error = %v", err)
	}
	wantSchema := config.Schema{
		{Name: "database.url", Env: "DATABASE_URL", Type: config.String, Required: true, Redact: true},
		{Name: "stripe.webhook_secret", Env: "STRIPE_WEBHOOK_SECRET", Type: config.String, Redact: true},
	}
	if !reflect.DeepEqual(schema, wantSchema) {
		t.Fatalf("Schema() = %#v, want %#v", schema, wantSchema)
	}

	redactKeys, err := mappings.RedactKeys()
	if err != nil {
		t.Fatalf("RedactKeys() error = %v", err)
	}
	wantRedactKeys := []string{"DATABASE_URL", "STRIPE_WEBHOOK_SECRET", "database.url", "stripe.webhook_secret"}
	if !reflect.DeepEqual(redactKeys, wantRedactKeys) {
		t.Fatalf("RedactKeys() = %#v, want %#v", redactKeys, wantRedactKeys)
	}

	envVars, err := mappings.EnvVars()
	if err != nil {
		t.Fatalf("EnvVars() error = %v", err)
	}
	wantEnvVars := map[string]string{
		"database.url":          "DATABASE_URL",
		"stripe.webhook_secret": "STRIPE_WEBHOOK_SECRET",
	}
	if !reflect.DeepEqual(envVars, wantEnvVars) {
		t.Fatalf("EnvVars() = %#v, want %#v", envVars, wantEnvVars)
	}

	refs, err := mappings.SecretRefs()
	if err != nil {
		t.Fatalf("SecretRefs() error = %v", err)
	}
	wantRefs := map[string]config.SecretRef{
		"database.url": {
			Name:    "lazuli/database-url",
			Version: "active",
		},
		"stripe.webhook_secret": {
			Name: "STRIPE_WEBHOOK_SECRET",
		},
	}
	if !reflect.DeepEqual(refs, wantRefs) {
		t.Fatalf("SecretRefs() = %#v, want %#v", refs, wantRefs)
	}

	envRefs, err := mappings.EnvSecretRefs()
	if err != nil {
		t.Fatalf("EnvSecretRefs() error = %v", err)
	}
	wantEnvRefs := map[string]config.SecretRef{
		"DATABASE_URL": {
			Name:    "lazuli/database-url",
			Version: "active",
		},
		"STRIPE_WEBHOOK_SECRET": {
			Name: "STRIPE_WEBHOOK_SECRET",
		},
	}
	if !reflect.DeepEqual(envRefs, wantEnvRefs) {
		t.Fatalf("EnvSecretRefs() = %#v, want %#v", envRefs, wantEnvRefs)
	}
}

func TestSecretMapLoadEnvValidatesRequiredAndOptional(t *testing.T) {
	mappings := config.SecretMap{
		config.RequiredSecretEnv("api.key", "API_KEY"),
		config.OptionalSecretEnv("webhook.secret", "WEBHOOK_SECRET"),
	}

	values, err := mappings.LoadEnv(mapLookup(map[string]string{
		"API_KEY": "api-secret",
	}))
	if err != nil {
		t.Fatalf("LoadEnv() error = %v", err)
	}
	if got, ok := values.String("api.key"); !ok || got != "api-secret" {
		t.Fatalf("api.key = %q, %v; want api-secret, true", got, ok)
	}
	if _, ok := values.Get("webhook.secret"); ok {
		t.Fatal("webhook.secret was loaded; want absent optional mapping omitted")
	}
	value, ok := values.Get("api.key")
	if !ok || !value.Redact || value.Env != "API_KEY" {
		t.Fatalf("api.key metadata = %#v, %v; want redacted API_KEY value", value, ok)
	}

	values, err = mappings.LoadEnv(mapLookup(map[string]string{}))
	if err == nil {
		t.Fatal("LoadEnv(missing required) error = nil, want ErrRequired")
	}
	if !errors.Is(err, config.ErrRequired) {
		t.Fatalf("LoadEnv(missing required) error = %v, want ErrRequired", err)
	}
	if len(values) != 0 {
		t.Fatalf("LoadEnv(missing required) values = %#v, want none", values)
	}
}

func TestSecretMapNormalizesDefaultsAndEnvSecretRefs(t *testing.T) {
	mappings, err := config.NewSecretMap(
		config.SecretRefEnv("API_TOKEN", "", config.SecretRef{Name: "env.API_TOKEN"}),
		config.OptionalSecretEnv("", "COOKIE_SECRET"),
	)
	if err != nil {
		t.Fatalf("NewSecretMap() error = %v", err)
	}

	want := config.SecretMap{
		{Key: "API_TOKEN", Env: "API_TOKEN", SecretRef: config.SecretRef{Name: "API_TOKEN"}, Required: true},
		{Key: "COOKIE_SECRET", Env: "COOKIE_SECRET", SecretRef: config.SecretRef{Name: "COOKIE_SECRET"}},
	}
	if !reflect.DeepEqual(mappings, want) {
		t.Fatalf("NewSecretMap() = %#v, want %#v", mappings, want)
	}
}

func TestSecretMapRejectsInvalidMappingsAndCollisions(t *testing.T) {
	mappings := config.SecretMap{
		{},
		config.RequiredSecretEnv("bad env", "1BAD"),
		config.RequiredSecretRefEnv("database.url", "DATABASE_URL", config.SecretRef{Name: "lazuli/database-url"}),
		config.OptionalSecretRefEnv("database.url", "OTHER_DATABASE_URL", config.SecretRef{Name: "lazuli/other-database-url"}),
		config.OptionalSecretEnv("api.key", "DATABASE_URL"),
		config.OptionalSecretRefEnv("cookie.secret", "COOKIE_SECRET", config.SecretRef{Name: "bad ref"}),
	}

	err := mappings.Validate()
	if err == nil {
		t.Fatal("Validate() error = nil, want validation errors")
	}
	if !errors.Is(err, config.ErrInvalidSecretMapping) {
		t.Fatalf("Validate() error = %v, want ErrInvalidSecretMapping", err)
	}
	if !errors.Is(err, config.ErrSecretMapCollision) {
		t.Fatalf("Validate() error = %v, want ErrSecretMapCollision", err)
	}
	for _, fragment := range []string{
		"secret_map[0].key",
		"secret_map[1].key",
		"secret_map[1].env",
		"secret_map[3].key \"database.url\"",
		"secret_map[4].env \"DATABASE_URL\"",
		"secret_map[5].secret_ref",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("Validate() error = %v; want fragment %q", err, fragment)
		}
	}
}
