package lazuli

import (
	"os"
	"strconv"
	"strings"
)

// EnvConfig holds the standard env wiring Lazuli generated code expects.
type EnvConfig struct {
	Port            int    // LAZULI_PORT, default 8080
	DBURL           string // LAZULI_DB, may be empty (no-DB mode)
	LogLevel        string // LAZULI_LOG_LEVEL: debug/info/warn/error, default info
	LogFormat       string // LAZULI_LOG_FORMAT: json/text, default json
	Pprof           bool   // LAZULI_PPROF: 1/true to mount /debug/pprof
	OtelExporter    string // LAZULI_OTEL_EXPORTER: otlphttp/noop, default noop
	GracePeriodSecs int    // LAZULI_GRACE_PERIOD: shutdown grace, default 30
}

// LoadEnv reads LAZULI_* from os.Environ() and returns the parsed config.
// Invalid values fall back to defaults (no panic).
func LoadEnv() EnvConfig {
	return EnvConfig{
		Port:            envInt("LAZULI_PORT", 8080),
		DBURL:           os.Getenv("LAZULI_DB"),
		LogLevel:        envStr("LAZULI_LOG_LEVEL", "info"),
		LogFormat:       envStr("LAZULI_LOG_FORMAT", "json"),
		Pprof:           envBool("LAZULI_PPROF", false),
		OtelExporter:    envStr("LAZULI_OTEL_EXPORTER", "noop"),
		GracePeriodSecs: envInt("LAZULI_GRACE_PERIOD", 30),
	}
}

func envStr(key, def string) string {
	raw := strings.TrimSpace(os.Getenv(key))
	if raw == "" {
		return def
	}
	value := strings.ToLower(raw)
	switch key {
	case "LAZULI_LOG_LEVEL":
		if value == "debug" || value == "info" || value == "warn" || value == "error" {
			return value
		}
	case "LAZULI_LOG_FORMAT":
		if value == "json" || value == "text" {
			return value
		}
	case "LAZULI_OTEL_EXPORTER":
		if value == "otlphttp" || value == "noop" {
			return value
		}
	default:
		return raw
	}
	return def
}

func envInt(key string, def int) int {
	value, err := strconv.Atoi(strings.TrimSpace(os.Getenv(key)))
	if err != nil {
		return def
	}
	return value
}

func envBool(key string, def bool) bool {
	value, err := strconv.ParseBool(strings.TrimSpace(os.Getenv(key)))
	if err != nil {
		return def
	}
	return value
}
