package lazuli

import (
	"errors"
	"fmt"
	"sync"
)

// AppIntegrationName is the binding name declared in `registry.lzi` as
// `bindings.<name>: <Kind>` (e.g. `object_store`, `payment_gateway`,
// `email_sender`). It identifies an app-level integration slot; the
// concrete adapter behind the slot is resolved through `RegisterAdapter`
// against the plugin reference (`@plugin/<name>`).
//
// Codegen emits one `RegisterAppIntegration` call per binding at boot
// (in `app/app_integrations.gen.go`) so the runtime can resolve the
// binding-name → adapter pair without re-parsing `registry.lzi`.
type AppIntegrationName = string

var (
	appIntegrationMu       sync.RWMutex
	appIntegrationRegistry = map[AppIntegrationName]AdapterRef{}
)

// ErrAppIntegrationMissing signals that a handler asked for a binding
// (`lazuli.ObjectStore("<name>")` / similar facades) that no
// `RegisterAppIntegration` call has populated yet. The runtime resolver
// returns this so handlers can short-circuit cleanly at boot rather
// than panicking on the first request.
var ErrAppIntegrationMissing = errors.New("lazuli: app integration not registered")

// RegisterAppIntegration records the adapter REF STRING behind a
// binding slot. Codegen calls this at boot:
//
//	func init() {
//	    lazuli.RegisterAppIntegration("object_store", "@plugin/object-store")
//	}
//
// Deferred resolution: the registry stores the binding-name → adapter
// ref pair. The actual adapter lookup runs lazily inside
// `ResolveAppIntegration` so plugin-package init() can register its
// adapter AFTER the codegen-emitted `RegisterAppIntegration` runs.
// This eliminates the init-order panic class entirely — every package
// init() finishes before the first facade call lands.
//
// Idempotent — the last registration wins. Call before `Mux()` is
// built; runtime lookups after boot are read-only on the binding map
// (the adapter registry is still safe under sync.RWMutex).
func RegisterAppIntegration(name AppIntegrationName, ref AdapterRef) {
	appIntegrationMu.Lock()
	defer appIntegrationMu.Unlock()
	appIntegrationRegistry[name] = ref
}

// ResolveAppIntegration returns the adapter behind a binding slot.
// Performs deferred adapter resolution: looks up the binding's
// registered adapter ref then calls `ResolveAdapter` at call time.
// Returns `ErrAppIntegrationMissing` when the binding has no
// registration; returns `ErrAdapterMissing` (via `ResolveAdapter`)
// when the plugin's `RegisterAdapter` never ran.
//
// Facades (`lazuli.ObjectStore`, `lazuli.PaymentGateway`, ...) call
// this then type-assert against their typed contract.
func ResolveAppIntegration(name AppIntegrationName) (Adapter, error) {
	appIntegrationMu.RLock()
	ref, ok := appIntegrationRegistry[name]
	appIntegrationMu.RUnlock()
	if !ok {
		return nil, fmt.Errorf("lazuli: app integration %q: %w", name, ErrAppIntegrationMissing)
	}
	adapter, err := ResolveAdapter(ref)
	if err != nil {
		return nil, fmt.Errorf("lazuli: app integration %q -> %w", name, err)
	}
	return adapter, nil
}
