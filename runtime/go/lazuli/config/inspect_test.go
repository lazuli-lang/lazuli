package config_test

import (
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/config"
)

func TestInspectMapFlattensSortsRedactsAndAddsSource(t *testing.T) {
	values := map[string]any{
		"service": map[string]any{
			"name":   "billing",
			"apiKey": "service-secret",
			"limits": map[string]int{
				"retries": 3,
			},
		},
		"database": map[string]any{
			"url": "postgres://secret@example.test/app",
			"pool": map[string]any{
				"max": 10,
			},
		},
		"auth": map[string]string{
			"session_token": "session-secret",
			"token_count":   "2",
		},
		"debug": true,
	}

	got := config.InspectMap(values, config.InspectOptions{
		Source:     "file",
		Provenance: "app.yaml",
		Mask:       "***",
	})
	want := []config.InspectionEntry{
		{Key: "auth.session_token", Value: "***", Source: "file", Provenance: "app.yaml", Redacted: true},
		{Key: "auth.token_count", Value: "2", Source: "file", Provenance: "app.yaml"},
		{Key: "database.pool.max", Value: 10, Source: "file", Provenance: "app.yaml"},
		{Key: "database.url", Value: "***", Source: "file", Provenance: "app.yaml", Redacted: true},
		{Key: "debug", Value: true, Source: "file", Provenance: "app.yaml"},
		{Key: "service.apiKey", Value: "***", Source: "file", Provenance: "app.yaml", Redacted: true},
		{Key: "service.limits.retries", Value: 3, Source: "file", Provenance: "app.yaml"},
		{Key: "service.name", Value: "billing", Source: "file", Provenance: "app.yaml"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("InspectMap() = %#v, want %#v", got, want)
	}
}

func TestInspectValuesUsesEnvProvenanceDefaultsAndExplicitRedaction(t *testing.T) {
	values, err := config.LoadEnv(config.Schema{
		{Name: "database.url", Env: "DATABASE_URL"},
		{Name: "http.port", Env: "PORT", Type: config.Int, Default: "8080", HasDefault: true},
		{Name: "service.name", Env: "SERVICE_NAME", Redact: true},
	}, inspectMapLookup(map[string]string{
		"DATABASE_URL": "postgres://secret@example.test/app",
		"SERVICE_NAME": "billing",
	}))
	if err != nil {
		t.Fatalf("LoadEnv() error = %v", err)
	}

	got := config.InspectValues(values, config.InspectOptions{Mask: "***"})
	want := []config.InspectionEntry{
		{Key: "database.url", Value: "***", Source: "env", Provenance: "DATABASE_URL", Redacted: true},
		{Key: "http.port", Value: 8080, Source: "default", Provenance: "PORT"},
		{Key: "service.name", Value: "***", Source: "env", Provenance: "SERVICE_NAME", Redacted: true},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("InspectValues() = %#v, want %#v", got, want)
	}
}

func TestFlattenMapPreservesLeavesAndEmptyMaps(t *testing.T) {
	empty := map[string]any{}
	values := map[string]any{
		"http": map[string]any{
			"port": 8080,
		},
		"empty": empty,
		"raw":   []string{"left", "right"},
	}

	got := config.FlattenMap(values)
	want := map[string]any{
		"http.port": 8080,
		"empty":     empty,
		"raw":       []string{"left", "right"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("FlattenMap() = %#v, want %#v", got, want)
	}
}

func TestShouldRedactKeyMatchesSensitiveSuffixes(t *testing.T) {
	tests := []struct {
		key  string
		want bool
	}{
		{key: "APP_API_KEY", want: true},
		{key: "database.url", want: true},
		{key: "service.password", want: true},
		{key: "token_count", want: false},
		{key: "service.token_count", want: false},
		{key: "request.timeout", want: false},
	}

	for _, tt := range tests {
		if got := config.ShouldRedactKey(tt.key); got != tt.want {
			t.Fatalf("ShouldRedactKey(%q) = %v, want %v", tt.key, got, tt.want)
		}
	}
}

func TestInspectMapUsesConfiguredRedactKeys(t *testing.T) {
	values := map[string]any{
		"password": "kept",
		"trace_id": "hidden",
	}

	got := config.InspectMap(values, config.InspectOptions{
		RedactKeys: []string{"trace_id"},
		Mask:       "***",
	})
	want := []config.InspectionEntry{
		{Key: "password", Value: "kept"},
		{Key: "trace_id", Value: "***", Redacted: true},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("InspectMap(custom keys) = %#v, want %#v", got, want)
	}

	disabled := config.InspectMap(values, config.InspectOptions{RedactKeys: []string{}})
	if disabled[0].Redacted || disabled[1].Redacted {
		t.Fatalf("InspectMap(disabled keys) = %#v, want no redaction", disabled)
	}
}

func inspectMapLookup(env map[string]string) config.LookupFunc {
	return func(name string) (string, bool) {
		value, ok := env[name]
		return value, ok
	}
}
