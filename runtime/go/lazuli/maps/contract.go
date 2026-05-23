// Package maps declares the runtime contract for `@lazuli/plugin-<name>`
// adapters that provide geocoding + reverse-geocoding services.
//
// The contract is intentionally minimal — wire-thin. Concrete adapters
// (Google Maps, Mapbox, HERE, Nominatim, etc.) live in separate
// `@lazuli/plugin-<name>` repos and register via `lazuli.RegisterAdapter`
// against this package's `Geocoder` interface.
//
// EXPERIMENTAL: shape may grow additive fields before 1.0. Stable
// promotion gated on first pilot consumer.
package maps

import "context"

// GeoPoint is the closed lat/lng tuple. Mirrors
// `lazuli_ir::BuiltinType::SemanticGeoPoint`.
type GeoPoint struct {
	Lat float64
	Lng float64
}

// GeocodeRequest is the address-to-coordinates query.
type GeocodeRequest struct {
	// Address is the free-form address string (street, city, country).
	Address string
	// Country is optional ISO 3166 alpha-2 code for biasing results
	// (e.g. "BR" for Brazil-bias). Empty = no bias.
	Country string
	// Locale is optional BCP-47 tag (e.g. "pt-BR") for localized
	// `Formatted` output. Empty = provider default.
	Locale string
}

// GeocodeResponse is the result of a successful geocode.
type GeocodeResponse struct {
	Location  GeoPoint
	Formatted string // provider's canonical formatted address
	// PlaceID is the provider's opaque identifier (Google place_id,
	// Mapbox mapbox_id, etc.) suitable for caching + reverse lookup.
	// Empty when the provider does not expose stable IDs.
	PlaceID string
}

// ReverseGeocodeRequest is the coordinates-to-address query.
type ReverseGeocodeRequest struct {
	Location GeoPoint
	Locale   string // optional BCP-47
}

// ReverseGeocodeResponse mirrors `GeocodeResponse` for the reverse direction.
type ReverseGeocodeResponse struct {
	Formatted string
	PlaceID   string
}

// Geocoder is the closed adapter contract. Implementations live in
// `@lazuli/plugin-<name>` repos.
//
// Implementations MUST be safe for concurrent use; the runtime calls
// `Geocode` / `ReverseGeocode` from many goroutines.
type Geocoder interface {
	Geocode(ctx context.Context, req GeocodeRequest) (GeocodeResponse, error)
	ReverseGeocode(ctx context.Context, req ReverseGeocodeRequest) (ReverseGeocodeResponse, error)
}
