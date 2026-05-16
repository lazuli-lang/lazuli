package webhooks

import "sync"

// Process-wide registry of webhook contracts + handlers. Generated
// `init()` blocks in `dist/go/<feature>/webhook.gen.go` will call
// `webhooks.Register(&contract, handler)` so `lazuli.Mux()` can
// auto-mount the routes alongside commands + APIs.
//
// Until the codegen ships the init blocks, capsules can call
// `webhooks.Register` from their own `register.go` workaround file
// (parallel to the WAR-RUNTIME-COMMAND-01 pattern).
var (
	registryMu       sync.RWMutex
	registeredHooks  []registeredWebhook
)

type registeredWebhook struct {
	Contract *WebhookContract
	Handler  HandlerFunc
}

// Register adds a webhook contract + its user handler to the global
// registry. Safe for concurrent use. Re-registration of the same
// (Feature, Name) pair panics — generated code should only call
// once per feature init.
func Register(contract *WebhookContract, handler HandlerFunc) {
	if contract == nil {
		panic("webhooks.Register: nil contract")
	}
	if handler == nil {
		panic("webhooks.Register: nil handler for " + contract.Feature + "." + contract.Name)
	}
	registryMu.Lock()
	defer registryMu.Unlock()
	for _, h := range registeredHooks {
		if h.Contract.Feature == contract.Feature && h.Contract.Name == contract.Name {
			panic("webhooks: " + contract.Feature + "." + contract.Name + " registered twice")
		}
	}
	registeredHooks = append(registeredHooks, registeredWebhook{Contract: contract, Handler: handler})
}

// Registered returns a snapshot of every registered webhook (used by
// `lazuli.Mux()` to auto-mount the routes).
func Registered() ([]WebhookContract, []HandlerFunc) {
	registryMu.RLock()
	defer registryMu.RUnlock()
	contracts := make([]WebhookContract, len(registeredHooks))
	handlers := make([]HandlerFunc, len(registeredHooks))
	for i, h := range registeredHooks {
		contracts[i] = *h.Contract
		handlers[i] = h.Handler
	}
	return contracts, handlers
}

// ResetRegistryForTesting clears the registry. Tests that exercise
// the auto-mount path call this in setup/teardown to avoid bleed
// between cases.
func ResetRegistryForTesting() {
	registryMu.Lock()
	defer registryMu.Unlock()
	registeredHooks = nil
}
