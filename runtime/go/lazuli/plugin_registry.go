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
	adapterMu         sync.RWMutex
	adapterRegistry   = map[AdapterRef]Adapter{}
	adapterBootLocked bool
)

// ErrAdapterMissing signals that generated code referenced an adapter that was
// not registered by a plugin package at boot.
var ErrAdapterMissing = errors.New("lazuli: adapter not registered")

// RegisterAdapter records a concrete adapter under its DSL-level reference.
// Plugin packages call this from init():
//
//	func init() { lazuli.RegisterAdapter("@plugin/mercadopago", &MercadoPagoAdapter{}) }
//
// SECURITY (FR SEC-C3): duplicate registrations panic to prevent
// supply-chain init()-shadowing hijack. After Boot() locks the
// registry, further RegisterAdapter calls also panic — the only
// valid registration window is the init() phase before main().
func RegisterAdapter(ref AdapterRef, impl Adapter) {
	adapterMu.Lock()
	defer adapterMu.Unlock()
	if adapterBootLocked {
		panic("lazuli: RegisterAdapter called after Boot() — supply-chain attempt blocked: " + ref)
	}
	if _, exists := adapterRegistry[ref]; exists {
		panic("lazuli: duplicate adapter registration for " + ref + " — likely supply-chain hijack")
	}
	adapterRegistry[ref] = impl
}

// LockAdapterRegistry seals the registry against further
// RegisterAdapter calls. Call from lazuli.Boot() after all
// init() blocks have run.
func LockAdapterRegistry() {
	adapterMu.Lock()
	defer adapterMu.Unlock()
	adapterBootLocked = true
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
