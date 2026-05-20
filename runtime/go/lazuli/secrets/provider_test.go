package secrets

import (
	"context"
	"errors"
	"testing"
)

func TestEnvProviderRoundtrip(t *testing.T) {
	t.Setenv("LAZULI_TEST_SECRET", "value")
	p := EnvProvider{}
	got, err := p.Get(context.Background(), "LAZULI_TEST_SECRET")
	if err != nil || got != "value" {
		t.Fatalf("Get = (%q, %v); want (value, nil)", got, err)
	}
}

func TestEnvProviderNotFound(t *testing.T) {
	p := EnvProvider{}
	_, err := p.Get(context.Background(), "NONEXISTENT_KEY_XYZ123")
	if !errors.Is(err, ErrSecretNotFound) {
		t.Fatalf("expected ErrSecretNotFound; got %v", err)
	}
}
