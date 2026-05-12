// Package maps defines provider-neutral geocoding contracts.
//
// Concrete adapters such as Google Maps, Mapbox, Nominatim, or private
// provider packs implement these interfaces outside Lazuli core. Generated
// application code depends on this package, not on provider SDKs.
package maps

import (
	"context"
	"errors"
	"fmt"
	"math"
	"strings"
)

// MapsProvider is the integration contract bound by features that need maps
// services. The first runtime surface is forward geocoding; future maps
// operations can extend this interface through narrower embedded contracts.
type MapsProvider interface {
	Geocoder
}

// Geocoder converts a human-readable address into one or more coordinates.
type Geocoder interface {
	Geocode(ctx context.Context, req GeocodeRequest) (GeocodeResponse, error)
}

// GeocodeRequest is the provider-neutral forward geocoding request shape.
type GeocodeRequest struct {
	// Address is the user-entered or normalized address to geocode.
	Address string

	// CountryCode optionally scopes results to an ISO 3166-1 alpha-2 country
	// code, such as "BR". Empty leaves the provider unrestricted.
	CountryCode string

	// Language optionally hints the response language, such as "pt-BR".
	Language string
}

// NormalizedAddress returns Address trimmed for validation and fake lookup.
func (r GeocodeRequest) NormalizedAddress() string {
	return strings.TrimSpace(r.Address)
}

// Validate validates the request without applying provider-specific rules.
func (r GeocodeRequest) Validate() error {
	if r.NormalizedAddress() == "" {
		return fieldError("address", ErrEmptyAddress)
	}
	return nil
}

// Coordinates is a WGS84 latitude/longitude pair.
type Coordinates struct {
	Latitude  float64
	Longitude float64
}

// Validate validates that the coordinate pair is finite and in WGS84 range.
func (c Coordinates) Validate() error {
	if math.IsNaN(c.Latitude) || math.IsInf(c.Latitude, 0) || c.Latitude < -90 || c.Latitude > 90 {
		return fieldError("latitude", fmt.Errorf("%w: latitude must be between -90 and 90", ErrInvalidCoordinates))
	}
	if math.IsNaN(c.Longitude) || math.IsInf(c.Longitude, 0) || c.Longitude < -180 || c.Longitude > 180 {
		return fieldError("longitude", fmt.Errorf("%w: longitude must be between -180 and 180", ErrInvalidCoordinates))
	}
	return nil
}

// GeocodeResult is one provider-neutral geocoding candidate.
type GeocodeResult struct {
	// Address is the provider-normalized formatted address, when available.
	Address string

	// Coordinates is the WGS84 location for this candidate.
	Coordinates Coordinates

	// PlaceID is an optional provider-stable place identifier. It is opaque to
	// Lazuli runtime code and may be empty for providers that do not expose one.
	PlaceID string
}

// Validate validates the result shape returned by an adapter.
func (r GeocodeResult) Validate() error {
	if err := r.Coordinates.Validate(); err != nil {
		return fieldError("coordinates", err)
	}
	return nil
}

// GeocodeResponse carries zero or more geocoding candidates.
type GeocodeResponse struct {
	Results []GeocodeResult
}

// Validate validates all returned candidates.
func (r GeocodeResponse) Validate() error {
	for i, result := range r.Results {
		if err := result.Validate(); err != nil {
			return fieldError(fmt.Sprintf("results[%d]", i), err)
		}
	}
	return nil
}

// First returns the first geocoding candidate, if one exists.
func (r GeocodeResponse) First() (GeocodeResult, bool) {
	if len(r.Results) == 0 {
		return GeocodeResult{}, false
	}
	return r.Results[0], true
}

// Geocode validates req, invokes provider, and validates the provider response.
func Geocode(ctx context.Context, provider Geocoder, req GeocodeRequest) (GeocodeResponse, error) {
	if err := req.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	if provider == nil {
		return GeocodeResponse{}, ErrProviderUnavailable
	}
	resp, err := provider.Geocode(ctx, req)
	if err != nil {
		return GeocodeResponse{}, err
	}
	if err := resp.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	return resp, nil
}

// Typed error sentinels surfaced by provider-neutral geocoding helpers.
var (
	// ErrEmptyAddress is returned when a geocode request has no address text.
	ErrEmptyAddress = errors.New("lazuli/maps: empty_address")

	// ErrInvalidCoordinates is wrapped when a coordinate pair is outside WGS84
	// bounds or contains NaN/Inf values.
	ErrInvalidCoordinates = errors.New("lazuli/maps: invalid_coordinates")

	// ErrAddressNotFound can be returned by adapters when an address resolves
	// to no candidates and that condition should be distinguished from an empty
	// successful response.
	ErrAddressNotFound = errors.New("lazuli/maps: address_not_found")

	// ErrProviderUnavailable is returned when no geocoder is bound.
	ErrProviderUnavailable = errors.New("lazuli/maps: provider_unavailable")
)

// ValidationError adds a field path to a geocoding validation failure.
type ValidationError struct {
	Field string
	Err   error
}

// Error implements error.
func (e *ValidationError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Field == "" {
		return e.Err.Error()
	}
	return e.Field + ": " + e.Err.Error()
}

// Unwrap exposes the underlying sentinel for errors.Is.
func (e *ValidationError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

func fieldError(field string, err error) error {
	if err == nil {
		return nil
	}
	return &ValidationError{Field: field, Err: err}
}
