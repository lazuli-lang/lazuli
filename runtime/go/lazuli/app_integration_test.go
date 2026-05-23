package lazuli

import (
	"errors"
	"testing"
)

type fakeIntegration struct {
	id string
}

func TestRegisterAndResolveAppIntegration(t *testing.T) {
	withEmptyAdapterRegistry(t)

	want := &fakeIntegration{id: "object_store"}
	RegisterAdapter("@lazuli/plugin-object_store_test_register", want)
	RegisterAppIntegration("object_store_test_register", "@lazuli/plugin-object_store_test_register")

	got, err := ResolveAppIntegration("object_store_test_register")
	if err != nil {
		t.Fatalf("ResolveAppIntegration: %v", err)
	}
	if got != want {
		t.Errorf("ResolveAppIntegration: want %v, got %v", want, got)
	}
}

func TestResolveAppIntegrationMissingBinding(t *testing.T) {
	_, err := ResolveAppIntegration("never_registered_test_xyz")
	if !errors.Is(err, ErrAppIntegrationMissing) {
		t.Errorf("ResolveAppIntegration: want ErrAppIntegrationMissing, got %v", err)
	}
}

// Deferred resolution invariant: registering an app integration whose
// adapter is NOT yet registered must NOT panic at registration time.
// The error surfaces only when the facade resolves the binding.
func TestRegisterAppIntegrationWithMissingAdapterDeferresError(t *testing.T) {
	withEmptyAdapterRegistry(t)

	// Register the binding before any RegisterAdapter call — this is
	// exactly the init-order panic class we are closing. Registration
	// must succeed even though the adapter ref is not yet present.
	RegisterAppIntegration("deferred_missing_adapter", "@lazuli/plugin-not-yet-here")

	_, err := ResolveAppIntegration("deferred_missing_adapter")
	if !errors.Is(err, ErrAdapterMissing) {
		t.Errorf("ResolveAppIntegration: want ErrAdapterMissing, got %v", err)
	}
}

// Deferred resolution invariant: register-then-register-adapter
// resolves correctly when the plugin's `RegisterAdapter` lands AFTER
// the codegen-emitted `RegisterAppIntegration`. Mirrors the real
// init-order condition between `app/app_integrations.gen.go` and
// each plugin package's `init()`.
func TestRegisterAppIntegrationResolvesAdapterRegisteredLater(t *testing.T) {
	withEmptyAdapterRegistry(t)

	RegisterAppIntegration("late_adapter_binding", "@lazuli/plugin-late-adapter")
	want := &fakeIntegration{id: "late"}
	RegisterAdapter("@lazuli/plugin-late-adapter", want)

	got, err := ResolveAppIntegration("late_adapter_binding")
	if err != nil {
		t.Fatalf("ResolveAppIntegration: %v", err)
	}
	if got != want {
		t.Errorf("ResolveAppIntegration: want %v, got %v", got, want)
	}
}

func TestRegisterAppIntegrationLastWriteWins(t *testing.T) {
	withEmptyAdapterRegistry(t)

	first := &fakeIntegration{id: "first"}
	second := &fakeIntegration{id: "second"}
	RegisterAdapter("@lazuli/plugin-last-write-first", first)
	RegisterAdapter("@lazuli/plugin-last-write-second", second)
	RegisterAppIntegration("test_last_write_wins", "@lazuli/plugin-last-write-first")
	RegisterAppIntegration("test_last_write_wins", "@lazuli/plugin-last-write-second")

	got, err := ResolveAppIntegration("test_last_write_wins")
	if err != nil {
		t.Fatalf("ResolveAppIntegration: %v", err)
	}
	if got != second {
		t.Errorf("idempotent registry: last write should win, got %v", got)
	}
}
