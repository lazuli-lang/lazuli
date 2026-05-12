package lazuli

import (
	"errors"
	"testing"
)

type pluginRegistryTestAdapter struct {
	name string
}

type pluginRegistryExpected interface {
	Expected()
}

func withEmptyAdapterRegistry(t *testing.T) {
	t.Helper()

	adapterMu.Lock()
	previous := adapterRegistry
	adapterRegistry = map[AdapterRef]Adapter{}
	adapterMu.Unlock()

	t.Cleanup(func() {
		adapterMu.Lock()
		adapterRegistry = previous
		adapterMu.Unlock()
	})
}

func TestRegisterAndResolveAdapter(t *testing.T) {
	withEmptyAdapterRegistry(t)

	want := &pluginRegistryTestAdapter{name: "mercadopago"}
	RegisterAdapter("@plugin/mercadopago", want)

	got, err := ResolveAdapter("@plugin/mercadopago")
	if err != nil {
		t.Fatalf("ResolveAdapter returned error: %v", err)
	}
	if got != want {
		t.Fatalf("ResolveAdapter = %v, want %v", got, want)
	}
}

func TestResolveAdapterMissing(t *testing.T) {
	withEmptyAdapterRegistry(t)

	_, err := ResolveAdapter("@plugin/missing")
	if !errors.Is(err, ErrAdapterMissing) {
		t.Fatalf("ResolveAdapter missing error = %v, want ErrAdapterMissing", err)
	}
}

func TestResolveTypedMismatch(t *testing.T) {
	withEmptyAdapterRegistry(t)

	RegisterAdapter("@plugin/wrong", &pluginRegistryTestAdapter{name: "wrong"})

	_, err := ResolveTyped[pluginRegistryExpected]("@plugin/wrong")
	if err == nil {
		t.Fatal("ResolveTyped mismatch returned nil error")
	}
}
