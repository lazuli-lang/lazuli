package config_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/config"
)

func TestLoadEnvParsesValuesDefaultsAndRedaction(t *testing.T) {
	env := map[string]string{
		"DATABASE_URL":    "postgres://secret@example.test/app",
		"APP_DEBUG":       "true",
		"REQUEST_TIMEOUT": "250ms",
		"EXTRA":           "ignored",
	}
	called := map[string]bool{}
	lookup := func(name string) (string, bool) {
		called[name] = true
		value, ok := env[name]
		return value, ok
	}

	values, err := config.LoadEnv(config.Schema{
		{Name: "database.url", Env: "DATABASE_URL", Type: config.String, Required: true, Redact: true},
		{Name: "http.port", Env: "PORT", Type: config.Int, Default: "8080", HasDefault: true},
		{Name: "debug", Env: "APP_DEBUG", Type: config.Bool},
		{Name: "request.timeout", Env: "REQUEST_TIMEOUT", Type: config.Duration},
		{Name: "optional", Env: "OPTIONAL"},
	}, lookup)
	if err != nil {
		t.Fatalf("LoadEnv() error = %v", err)
	}

	if got, ok := values.String("database.url"); !ok || got != env["DATABASE_URL"] {
		t.Fatalf("database.url = %q, %v; want env value, true", got, ok)
	}
	port, ok := values.Int("http.port")
	if !ok || port != 8080 {
		t.Fatalf("http.port = %d, %v; want 8080, true", port, ok)
	}
	debug, ok := values.Bool("debug")
	if !ok || !debug {
		t.Fatalf("debug = %v, %v; want true, true", debug, ok)
	}
	timeout, ok := values.Duration("request.timeout")
	if !ok || timeout != 250*time.Millisecond {
		t.Fatalf("request.timeout = %v, %v; want 250ms, true", timeout, ok)
	}
	if _, ok := values.Get("optional"); ok {
		t.Fatal("optional value was loaded; want absent optional field omitted")
	}

	secret, ok := values.Get("database.url")
	if !ok {
		t.Fatal("database.url metadata missing")
	}
	if !secret.Redact || secret.Defaulted {
		t.Fatalf("database.url metadata = Redact %v Defaulted %v; want true false", secret.Redact, secret.Defaulted)
	}
	portValue, ok := values.Get("http.port")
	if !ok || !portValue.Defaulted {
		t.Fatalf("http.port Defaulted = %v, %v; want true, true", portValue.Defaulted, ok)
	}

	wantMap := map[string]any{
		"database.url":    env["DATABASE_URL"],
		"http.port":       8080,
		"debug":           true,
		"request.timeout": 250 * time.Millisecond,
	}
	if got := values.Map(); !reflect.DeepEqual(got, wantMap) {
		t.Fatalf("Map() = %#v, want %#v", got, wantMap)
	}

	redacted := values.RedactedMap("[redacted]")
	if redacted["database.url"] != "[redacted]" {
		t.Fatalf("redacted database.url = %#v, want mask", redacted["database.url"])
	}
	if redacted["http.port"] != 8080 {
		t.Fatalf("redacted http.port = %#v, want 8080", redacted["http.port"])
	}

	if called["EXTRA"] {
		t.Fatal("lookup called for undeclared EXTRA env")
	}
	for _, name := range []string{"DATABASE_URL", "PORT", "APP_DEBUG", "REQUEST_TIMEOUT", "OPTIONAL"} {
		if !called[name] {
			t.Fatalf("lookup was not called for declared env %s", name)
		}
	}
}

func TestLoadEnvUsesNameAndEnvFallbacks(t *testing.T) {
	values, err := config.LoadEnv(config.Schema{
		{Name: "SERVICE_NAME", Required: true},
		{Env: "ONLY_ENV", Default: "fallback", HasDefault: true},
	}, mapLookup(map[string]string{
		"SERVICE_NAME": "api",
	}))
	if err != nil {
		t.Fatalf("LoadEnv() error = %v", err)
	}

	if got, ok := values.String("SERVICE_NAME"); !ok || got != "api" {
		t.Fatalf("SERVICE_NAME = %q, %v; want api, true", got, ok)
	}
	if got, ok := values.String("ONLY_ENV"); !ok || got != "fallback" {
		t.Fatalf("ONLY_ENV = %q, %v; want fallback, true", got, ok)
	}
	value, ok := values.Get("ONLY_ENV")
	if !ok || value.Env != "ONLY_ENV" || !value.Defaulted {
		t.Fatalf("ONLY_ENV metadata = %#v, %v; want env fallback and defaulted", value, ok)
	}
}

func TestLoadEnvReportsRequiredAndInvalidValues(t *testing.T) {
	values, err := config.LoadEnv(config.Schema{
		{Name: "api.key", Env: "API_KEY", Type: config.String, Required: true, Redact: true},
		{Name: "workers", Env: "WORKERS", Type: config.Int},
		{Name: "enabled", Env: "ENABLED", Type: config.Bool},
		{Name: "timeout", Env: "TIMEOUT", Type: config.Duration, Default: "not-a-duration", HasDefault: true},
	}, mapLookup(map[string]string{
		"API_KEY": "",
		"WORKERS": "many",
		"ENABLED": "maybe",
	}))
	if err == nil {
		t.Fatal("LoadEnv() error = nil, want validation errors")
	}
	if !errors.Is(err, config.ErrRequired) {
		t.Fatalf("LoadEnv() error = %v; want ErrRequired", err)
	}
	if !errors.Is(err, config.ErrInvalidValue) {
		t.Fatalf("LoadEnv() error = %v; want ErrInvalidValue", err)
	}
	for _, fragment := range []string{"api.key env API_KEY", "workers env WORKERS", "enabled env ENABLED", "timeout env TIMEOUT"} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("LoadEnv() error = %v; want fragment %q", err, fragment)
		}
	}
	if len(values) != 0 {
		t.Fatalf("values = %#v, want no successfully parsed values", values)
	}
}

func TestLoadEnvValidatesSchema(t *testing.T) {
	values, err := config.LoadEnv(config.Schema{
		{Name: "dup"},
		{Name: "dup"},
		{Name: "bad", Type: config.Kind("float")},
		{},
	}, mapLookup(map[string]string{
		"dup": "ok",
		"bad": "1.25",
	}))
	if err == nil {
		t.Fatal("LoadEnv() error = nil, want schema errors")
	}
	if !errors.Is(err, config.ErrInvalidField) {
		t.Fatalf("LoadEnv() error = %v; want ErrInvalidField", err)
	}
	if got, ok := values.String("dup"); !ok || got != "ok" {
		t.Fatalf("first dup value = %q, %v; want ok, true", got, ok)
	}
	for _, fragment := range []string{"duplicate field name", "unsupported type", "name or env is required"} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("LoadEnv() error = %v; want fragment %q", err, fragment)
		}
	}
}

func mapLookup(env map[string]string) config.LookupFunc {
	return func(name string) (string, bool) {
		value, ok := env[name]
		return value, ok
	}
}
