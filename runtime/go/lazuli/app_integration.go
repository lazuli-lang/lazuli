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
	appIntegrationRegistry = map[AppIntegrationName]Adapter{}
)

// ErrAppIntegrationMissing signals that a handler asked for a binding
// (`lazuli.ObjectStore("<name>")` / similar facades) that no
// `RegisterAppIntegration` call has populated yet. The runtime resolver
// returns this so handlers can short-circuit cleanly at boot rather
// than panicking on the first request.
var ErrAppIntegrationMissing = errors.New("lazuli: app integration not registered")

// RegisterAppIntegration records the resolved adapter behind a binding
// slot. Codegen calls this at boot:
//
//	func init() {
//	    lazuli.RegisterAppIntegration("object_store", lazuli.MustResolveAdapter("@plugin/object-store"))
//	}
//
// The two-step (adapter ref → binding name) keeps the codegen emit
// wire-thin: codegen knows the binding name + plugin ref pair from
// `registry.lzi`; the plugin's own `init()` registers itself with
// `RegisterAdapter`; the codegen-emitted call wires the two together.
//
// Idempotent — the last registration wins. Call before `Mux()` is
// built; runtime lookups after boot are read-only.
func RegisterAppIntegration(name AppIntegrationName, impl Adapter) {
	appIntegrationMu.Lock()
	defer appIntegrationMu.Unlock()
	appIntegrationRegistry[name] = impl
}

// ResolveAppIntegration returns the adapter behind a binding slot, or
// `ErrAppIntegrationMissing` when no registration matches. Facades
// (`lazuli.ObjectStore`, `lazuli.PaymentGateway`, ...) call this then
// type-assert against their typed contract.
func ResolveAppIntegration(name AppIntegrationName) (Adapter, error) {
	appIntegrationMu.RLock()
	defer appIntegrationMu.RUnlock()
	impl, ok := appIntegrationRegistry[name]
	if !ok {
		return nil, fmt.Errorf("lazuli: app integration %q: %w", name, ErrAppIntegrationMissing)
	}
	return impl, nil
}
