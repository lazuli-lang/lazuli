package lazuli

import (
	"errors"
	"testing"
)

type fakeIntegration struct {
	id string
}

func TestRegisterAndResolveAppIntegration(t *testing.T) {
	want := &fakeIntegration{id: "object_store"}
	RegisterAppIntegration("object_store_test_register", want)

	got, err := ResolveAppIntegration("object_store_test_register")
	if err != nil {
		t.Fatalf("ResolveAppIntegration: %v", err)
	}
	if got != want {
		t.Errorf("ResolveAppIntegration: want %v, got %v", want, got)
	}
}

func TestResolveAppIntegrationMissing(t *testing.T) {
	_, err := ResolveAppIntegration("never_registered_test_xyz")
	if !errors.Is(err, ErrAppIntegrationMissing) {
		t.Errorf("ResolveAppIntegration: want ErrAppIntegrationMissing, got %v", err)
	}
}

func TestRegisterAppIntegrationLastWriteWins(t *testing.T) {
	first := &fakeIntegration{id: "first"}
	second := &fakeIntegration{id: "second"}
	RegisterAppIntegration("test_last_write_wins", first)
	RegisterAppIntegration("test_last_write_wins", second)

	got, err := ResolveAppIntegration("test_last_write_wins")
	if err != nil {
		t.Fatalf("ResolveAppIntegration: %v", err)
	}
	if got != second {
		t.Errorf("idempotent registry: last write should win, got %v", got)
	}
}
