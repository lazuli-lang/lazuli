package maps_test

import (
	"context"
	"errors"
	"math"
	"testing"

	"lazuli.dev/runtime/lazuli/maps"
)

func TestGeocodeRequestRejectsEmptyAddress(t *testing.T) {
	t.Parallel()

	err := maps.GeocodeRequest{Address: " \t\n "}.Validate()
	if !errors.Is(err, maps.ErrEmptyAddress) {
		t.Fatalf("Validate() error = %v, want ErrEmptyAddress", err)
	}
}

func TestCoordinatesValidateRange(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name        string
		coordinates maps.Coordinates
	}{
		{name: "latitude too high", coordinates: maps.Coordinates{Latitude: 91, Longitude: 0}},
		{name: "latitude nan", coordinates: maps.Coordinates{Latitude: math.NaN(), Longitude: 0}},
		{name: "longitude too low", coordinates: maps.Coordinates{Latitude: 0, Longitude: -181}},
		{name: "longitude inf", coordinates: maps.Coordinates{Latitude: 0, Longitude: math.Inf(1)}},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			if err := tc.coordinates.Validate(); !errors.Is(err, maps.ErrInvalidCoordinates) {
				t.Fatalf("Validate() error = %v, want ErrInvalidCoordinates", err)
			}
		})
	}
}

func TestCoordinatesAcceptBoundaryValues(t *testing.T) {
	t.Parallel()

	coordinates := maps.Coordinates{Latitude: -90, Longitude: 180}
	if err := coordinates.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
}

func TestGeocodeHelperValidatesProviderResponse(t *testing.T) {
	t.Parallel()

	provider := &maps.FakeProvider{}
	provider.SetResponse("Rua Oscar Freire, Sao Paulo", maps.GeocodeResponse{
		Results: []maps.GeocodeResult{{
			Address:     "Rua Oscar Freire, Sao Paulo, SP, Brazil",
			Coordinates: maps.Coordinates{Latitude: -23.561414, Longitude: -46.669633},
			PlaceID:     "place-1",
		}},
	})

	resp, err := maps.Geocode(context.Background(), provider, maps.GeocodeRequest{
		Address:     " Rua Oscar Freire, Sao Paulo ",
		CountryCode: "BR",
		Language:    "pt-BR",
	})
	if err != nil {
		t.Fatalf("Geocode() error = %v", err)
	}

	result, ok := resp.First()
	if !ok {
		t.Fatal("First() ok = false, want true")
	}
	if result.PlaceID != "place-1" {
		t.Fatalf("PlaceID = %q, want place-1", result.PlaceID)
	}

	requests := provider.Requests()
	if len(requests) != 1 {
		t.Fatalf("Requests len = %d, want 1", len(requests))
	}
	if requests[0].NormalizedAddress() != "Rua Oscar Freire, Sao Paulo" {
		t.Fatalf("NormalizedAddress = %q", requests[0].NormalizedAddress())
	}
}

func TestGeocodeHelperRejectsInvalidResponseCoordinates(t *testing.T) {
	t.Parallel()

	provider := &maps.FakeProvider{}
	provider.SetResponse("bad", maps.GeocodeResponse{
		Results: []maps.GeocodeResult{{
			Coordinates: maps.Coordinates{Latitude: 0, Longitude: 181},
		}},
	})

	_, err := maps.Geocode(context.Background(), provider, maps.GeocodeRequest{Address: "bad"})
	if !errors.Is(err, maps.ErrInvalidCoordinates) {
		t.Fatalf("Geocode() error = %v, want ErrInvalidCoordinates", err)
	}
}

func TestGeocodeHelperRejectsNilProvider(t *testing.T) {
	t.Parallel()

	_, err := maps.Geocode(context.Background(), nil, maps.GeocodeRequest{Address: "Avenida Paulista"})
	if !errors.Is(err, maps.ErrProviderUnavailable) {
		t.Fatalf("Geocode() error = %v, want ErrProviderUnavailable", err)
	}
}

func TestNoopProviderReturnsEmptyResponse(t *testing.T) {
	t.Parallel()

	resp, err := maps.NoopProvider{}.Geocode(context.Background(), maps.GeocodeRequest{Address: "Avenida Paulista"})
	if err != nil {
		t.Fatalf("Geocode() error = %v", err)
	}
	if _, ok := resp.First(); ok {
		t.Fatal("First() ok = true, want false")
	}
}

func TestFakeProviderPropagatesContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := (&maps.FakeProvider{}).Geocode(ctx, maps.GeocodeRequest{Address: "Avenida Paulista"})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Geocode() error = %v, want context.Canceled", err)
	}
}
