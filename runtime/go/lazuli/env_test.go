package lazuli

import "testing"

func TestLoadEnvReadsAllStandardKeys(t *testing.T) {
	t.Setenv("LAZULI_PORT", "9090")
	t.Setenv("LAZULI_DB", "postgres://lazuli:lazuli@example.test/app")
	t.Setenv("LAZULI_LOG_LEVEL", "debug")
	t.Setenv("LAZULI_LOG_FORMAT", "text")
	t.Setenv("LAZULI_PPROF", "true")
	t.Setenv("LAZULI_OTEL_EXPORTER", "otlphttp")
	t.Setenv("LAZULI_GRACE_PERIOD", "45")

	cfg := LoadEnv()

	if cfg.Port != 9090 {
		t.Fatalf("Port = %d, want 9090", cfg.Port)
	}
	if cfg.DBURL != "postgres://lazuli:lazuli@example.test/app" {
		t.Fatalf("DBURL = %q, want env value", cfg.DBURL)
	}
	if cfg.LogLevel != "debug" {
		t.Fatalf("LogLevel = %q, want debug", cfg.LogLevel)
	}
	if cfg.LogFormat != "text" {
		t.Fatalf("LogFormat = %q, want text", cfg.LogFormat)
	}
	if !cfg.Pprof {
		t.Fatal("Pprof = false, want true")
	}
	if cfg.OtelExporter != "otlphttp" {
		t.Fatalf("OtelExporter = %q, want otlphttp", cfg.OtelExporter)
	}
	if cfg.GracePeriodSecs != 45 {
		t.Fatalf("GracePeriodSecs = %d, want 45", cfg.GracePeriodSecs)
	}
}

func TestLoadEnvUsesDefaultsWhenUnset(t *testing.T) {
	for _, key := range []string{
		"LAZULI_PORT",
		"LAZULI_DB",
		"LAZULI_LOG_LEVEL",
		"LAZULI_LOG_FORMAT",
		"LAZULI_PPROF",
		"LAZULI_OTEL_EXPORTER",
		"LAZULI_GRACE_PERIOD",
	} {
		t.Setenv(key, "")
	}

	cfg := LoadEnv()

	if cfg.Port != 8080 {
		t.Fatalf("Port = %d, want 8080", cfg.Port)
	}
	if cfg.DBURL != "" {
		t.Fatalf("DBURL = %q, want empty", cfg.DBURL)
	}
	if cfg.LogLevel != "info" {
		t.Fatalf("LogLevel = %q, want info", cfg.LogLevel)
	}
	if cfg.LogFormat != "json" {
		t.Fatalf("LogFormat = %q, want json", cfg.LogFormat)
	}
	if cfg.Pprof {
		t.Fatal("Pprof = true, want false")
	}
	if cfg.OtelExporter != "noop" {
		t.Fatalf("OtelExporter = %q, want noop", cfg.OtelExporter)
	}
	if cfg.GracePeriodSecs != 30 {
		t.Fatalf("GracePeriodSecs = %d, want 30", cfg.GracePeriodSecs)
	}
}

func TestLoadEnvFallsBackForInvalidValues(t *testing.T) {
	t.Setenv("LAZULI_PORT", "many")
	t.Setenv("LAZULI_LOG_LEVEL", "verbose")
	t.Setenv("LAZULI_LOG_FORMAT", "xml")
	t.Setenv("LAZULI_PPROF", "enabled")
	t.Setenv("LAZULI_OTEL_EXPORTER", "zipkin")
	t.Setenv("LAZULI_GRACE_PERIOD", "soon")

	cfg := LoadEnv()

	if cfg.Port != 8080 {
		t.Fatalf("Port = %d, want default 8080", cfg.Port)
	}
	if cfg.LogLevel != "info" {
		t.Fatalf("LogLevel = %q, want default info", cfg.LogLevel)
	}
	if cfg.LogFormat != "json" {
		t.Fatalf("LogFormat = %q, want default json", cfg.LogFormat)
	}
	if cfg.Pprof {
		t.Fatal("Pprof = true, want default false")
	}
	if cfg.OtelExporter != "noop" {
		t.Fatalf("OtelExporter = %q, want default noop", cfg.OtelExporter)
	}
	if cfg.GracePeriodSecs != 30 {
		t.Fatalf("GracePeriodSecs = %d, want default 30", cfg.GracePeriodSecs)
	}
}
