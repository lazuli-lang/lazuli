package config_test

import (
	"errors"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/config"
)

func TestInspectEnvReportsSourcesMissingRedactionAndSummary(t *testing.T) {
	report, err := config.InspectEnv(config.Schema{
		{Name: "optional", Env: "OPTIONAL"},
		{Name: "http.port", Env: "PORT", Type: config.Int, Default: "8080", HasDefault: true},
		{Name: "api.key", Env: "APP_API_KEY", Required: true},
		{Name: "debug", Env: "APP_DEBUG", Type: config.Bool},
		{Name: "database.url", Env: "DATABASE_URL", Required: true},
	}, envInspectionLookup(map[string]string{
		"APP_API_KEY": "secret-key",
		"APP_DEBUG":   "true",
	}), config.EnvInspectOptions{Mask: "***"})
	if err != nil {
		t.Fatalf("InspectEnv() error = %v", err)
	}

	wantEntries := []config.EnvInspectionEntry{
		{Name: "api.key", Env: "APP_API_KEY", Type: config.String, Required: true, Source: config.EnvSourceEnv, Value: "***", Redacted: true},
		{Name: "debug", Env: "APP_DEBUG", Type: config.Bool, Source: config.EnvSourceEnv, Value: true},
		{Name: "database.url", Env: "DATABASE_URL", Type: config.String, Required: true, Source: config.EnvSourceMissing, Missing: true},
		{Name: "optional", Env: "OPTIONAL", Type: config.String, Source: config.EnvSourceMissing, Missing: true},
		{Name: "http.port", Env: "PORT", Type: config.Int, Source: config.EnvSourceDefault, Value: 8080},
	}
	if !reflect.DeepEqual(report.Entries, wantEntries) {
		t.Fatalf("InspectEnv() entries = %#v, want %#v", report.Entries, wantEntries)
	}

	wantSummary := config.EnvInspectionSummary{
		Total:           5,
		Required:        2,
		Optional:        3,
		Present:         3,
		Missing:         2,
		MissingRequired: 1,
		FromEnv:         2,
		FromDefault:     1,
		Redacted:        1,
	}
	if !reflect.DeepEqual(report.Summary, wantSummary) {
		t.Fatalf("InspectEnv() summary = %#v, want %#v", report.Summary, wantSummary)
	}
}

func TestInspectEnvFiltersByEnvPrefixAndSorts(t *testing.T) {
	report, err := config.InspectEnv(config.Schema{
		{Name: "service.b", Env: "APP_B"},
		{Name: "database.url", Env: "DATABASE_URL"},
		{Name: "service.a", Env: "APP_A"},
	}, envInspectionLookup(map[string]string{
		"APP_A":        "a",
		"APP_B":        "b",
		"DATABASE_URL": "postgres://secret@example.test/app",
	}), config.EnvInspectOptions{Prefixes: []string{" APP_ "}})
	if err != nil {
		t.Fatalf("InspectEnv() error = %v", err)
	}

	want := []config.EnvInspectionEntry{
		{Name: "service.a", Env: "APP_A", Type: config.String, Source: config.EnvSourceEnv, Value: "a"},
		{Name: "service.b", Env: "APP_B", Type: config.String, Source: config.EnvSourceEnv, Value: "b"},
	}
	if !reflect.DeepEqual(report.Entries, want) {
		t.Fatalf("InspectEnv(prefix) entries = %#v, want %#v", report.Entries, want)
	}
	if report.Summary.Total != 2 || report.Summary.FromEnv != 2 {
		t.Fatalf("InspectEnv(prefix) summary = %#v, want total/from_env 2", report.Summary)
	}
}

func TestRequiredAndMissingEnvVars(t *testing.T) {
	schema := config.Schema{
		{Name: "debug", Env: "APP_DEBUG"},
		{Name: "database.url", Env: "DATABASE_URL", Required: true},
		{Name: "api.key", Env: "APP_API_KEY", Required: true},
		{Name: "service.token", Env: "APP_API_KEY", Required: true},
		{Name: "service.name", Env: "SERVICE_NAME", Required: true, Default: "api", HasDefault: true},
		{Name: "WORKERS", Required: true},
	}

	required, err := config.RequiredEnvVars(schema)
	if err != nil {
		t.Fatalf("RequiredEnvVars() error = %v", err)
	}
	wantRequired := []string{"APP_API_KEY", "DATABASE_URL", "SERVICE_NAME", "WORKERS"}
	if !reflect.DeepEqual(required, wantRequired) {
		t.Fatalf("RequiredEnvVars() = %#v, want %#v", required, wantRequired)
	}

	missing, err := config.MissingEnvVars(schema, envInspectionLookup(map[string]string{
		"APP_API_KEY": "",
		"WORKERS":     "4",
	}))
	if err != nil {
		t.Fatalf("MissingEnvVars() error = %v", err)
	}
	wantMissing := []string{"APP_API_KEY", "DATABASE_URL"}
	if !reflect.DeepEqual(missing, wantMissing) {
		t.Fatalf("MissingEnvVars() = %#v, want %#v", missing, wantMissing)
	}
}

func TestInspectEnvReportsInvalidValuesAndSchemaErrors(t *testing.T) {
	report, err := config.InspectEnv(config.Schema{
		{Name: "workers", Env: "WORKERS", Type: config.Int},
		{Name: "dup", Env: "DUP"},
		{Name: "dup", Env: "DUP_2"},
	}, envInspectionLookup(map[string]string{
		"WORKERS": "many",
		"DUP":     "first",
	}), config.EnvInspectOptions{})
	if err == nil {
		t.Fatal("InspectEnv() error = nil, want joined errors")
	}
	if !errors.Is(err, config.ErrInvalidValue) {
		t.Fatalf("InspectEnv() error = %v; want ErrInvalidValue", err)
	}
	if !errors.Is(err, config.ErrInvalidField) {
		t.Fatalf("InspectEnv() error = %v; want ErrInvalidField", err)
	}

	want := []config.EnvInspectionEntry{
		{Name: "dup", Env: "DUP", Type: config.String, Source: config.EnvSourceEnv, Value: "first"},
		{Name: "workers", Env: "WORKERS", Type: config.Int, Source: config.EnvSourceEnv, Value: "many"},
	}
	if !reflect.DeepEqual(report.Entries, want) {
		t.Fatalf("InspectEnv() entries = %#v, want %#v", report.Entries, want)
	}
}

func envInspectionLookup(env map[string]string) config.LookupFunc {
	return func(name string) (string, bool) {
		value, ok := env[name]
		return value, ok
	}
}
