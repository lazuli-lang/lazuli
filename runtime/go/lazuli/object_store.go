package lazuli

import (
	"fmt"

	"lazuli.dev/runtime/lazuli/storage"
)

// ObjectStore resolves the binding-name → `storage.Provider` mapping
// declared in `registry.lzi` (the `bindings.<name>: ObjectStore`
// shape lowered by the codegen `registry` parser). Handlers call this
// instead of reading `S3_ENDPOINT` / `S3_BUCKET_DEFAULT` env vars
// directly — the runtime owns the wiring; the plugin owns the
// implementation.
//
// Spec: docs/proposals/hostpoint-complete-roadmap-2026-05-18.md §3.5.
//
// Returns `ErrAppIntegrationMissing` when no
// `RegisterAppIntegration(<name>, ...)` ran at boot (codegen emits
// these in `app/app_integrations.gen.go`). Returns a typed error
// when the resolved adapter does not implement `storage.Provider` —
// that means the bound plugin does not satisfy the object-store
// contract and the operator must rebind.
//
// Example (hostpoint traveler photo upload):
//
//	store, err := lazuli.ObjectStore("object_store")
//	if err != nil { return travelergen.TravelerPhotoUploadIntent{}, err }
//	url, err := store.PresignedURL(ctx, "media", key, time.Hour, "PUT")
func ObjectStore(binding string) (storage.Provider, error) {
	impl, err := ResolveAppIntegration(binding)
	if err != nil {
		return nil, err
	}
	provider, ok := impl.(storage.Provider)
	if !ok {
		return nil, fmt.Errorf("lazuli: app integration %q does not implement storage.Provider", binding)
	}
	return provider, nil
}
