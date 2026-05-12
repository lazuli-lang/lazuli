package lazuli

import (
	"errors"
	"fmt"
	"sync"
)

// AdapterRef is the DSL-level reference used to bind generated code to a
// concrete plugin or adapter implementation.
type AdapterRef = string

// Adapter accepts any concrete adapter shape. Consumers type-assert to the
// interface or struct they require.
type Adapter interface{}

var (
	adapterMu       sync.RWMutex
	adapterRegistry = map[AdapterRef]Adapter{}
)

// ErrAdapterMissing signals that generated code referenced an adapter that was
// not registered by a plugin package at boot.
var ErrAdapterMissing = errors.New("lazuli: adapter not registered")

// RegisterAdapter records a concrete adapter under its DSL-level reference.
// Plugin packages call this from init():
//
//	func init() { lazuli.RegisterAdapter("@plugin/mercadopago", &MercadoPagoAdapter{}) }
func RegisterAdapter(ref AdapterRef, impl Adapter) {
	adapterMu.Lock()
	defer adapterMu.Unlock()
	adapterRegistry[ref] = impl
}

// ResolveAdapter returns the adapter for a ref, or an error when unregistered.
// Generated code calls this when materialising a command/auth/api that
// references the adapter.
func ResolveAdapter(ref AdapterRef) (Adapter, error) {
	adapterMu.RLock()
	defer adapterMu.RUnlock()
	impl, ok := adapterRegistry[ref]
	if !ok {
		return nil, fmt.Errorf("lazuli: adapter %q not registered: %w", ref, ErrAdapterMissing)
	}
	return impl, nil
}

// MustResolveAdapter panics if missing, which is useful in init paths.
func MustResolveAdapter(ref AdapterRef) Adapter {
	impl, err := ResolveAdapter(ref)
	if err != nil {
		panic(err)
	}
	return impl
}

// ResolveTyped returns a typed view of a registered adapter.
func ResolveTyped[T any](ref AdapterRef) (T, error) {
	var zero T
	impl, err := ResolveAdapter(ref)
	if err != nil {
		return zero, err
	}
	typed, ok := impl.(T)
	if !ok {
		return zero, fmt.Errorf("lazuli: adapter %q wrong type", ref)
	}
	return typed, nil
}
