package lazuli

import (
	"fmt"

	"lazuli.dev/runtime/lazuli/maps"
)

// Geocoder resolves the binding-name → `maps.Geocoder` mapping
// declared in `registry.lzi` (the `bindings.<name>: Geocoder` shape
// lowered by the codegen `registry` parser). Handlers call this
// instead of importing a specific provider package — the runtime
// owns the wiring; the plugin owns the implementation.
//
// Spec: parity with `lazuli.ObjectStore` / `lazuli.PaymentGateway`.
//
// Returns `ErrAppIntegrationMissing` when no
// `RegisterAppIntegration(<name>, ...)` ran at boot (codegen emits
// these in `app/app_integrations.gen.go`). Returns a typed error
// when the resolved adapter does not implement `maps.Geocoder` —
// that means the bound plugin does not satisfy the geocoder
// contract and the operator must rebind.
//
// Example (the canonical pilot property pin confirmation):
//
//	geo, err := lazuli.Geocoder("geocoder")
//	if err != nil { return Result{}, err }
//	resp, err := geo.Geocode(ctx, maps.GeocodeRequest{Address: addr, Country: "BR"})
func Geocoder(binding string) (maps.Geocoder, error) {
	impl, err := ResolveAppIntegration(binding)
	if err != nil {
		return nil, err
	}
	geo, ok := impl.(maps.Geocoder)
	if !ok {
		return nil, fmt.Errorf("lazuli: app integration %q does not implement maps.Geocoder", binding)
	}
	return geo, nil
}
