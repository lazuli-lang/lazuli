package maps_test

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"lazuli.dev/runtime/lazuli/maps"
)

func TestGoogleProviderGeocodeSuccess(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Fatalf("method = %s, want GET", r.Method)
		}
		if r.URL.Path != "/maps/api/geocode/json" {
			t.Fatalf("path = %s, want /maps/api/geocode/json", r.URL.Path)
		}
		q := r.URL.Query()
		if got := q.Get("address"); got != "Avenida Paulista" {
			t.Fatalf("address = %q, want normalized address", got)
		}
		if got := q.Get("key"); got != "test-key" {
			t.Fatalf("key = %q, want API key", got)
		}
		if got := q.Get("components"); got != "country:BR" {
			t.Fatalf("components = %q, want country:BR", got)
		}
		if got := q.Get("language"); got != "pt-BR" {
			t.Fatalf("language = %q, want pt-BR", got)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
			"status": "OK",
			"results": [{
				"formatted_address": "Av. Paulista, Bela Vista, Sao Paulo - SP, Brazil",
				"place_id": "place_123",
				"geometry": {"location": {"lat": -23.561414, "lng": -46.655881}}
			}]
		}`))
	}))
	defer server.Close()

	provider := &maps.GoogleProvider{
		APIKey:     "test-key",
		BaseURL:    server.URL,
		HTTPClient: server.Client(),
	}
	resp, err := provider.Geocode(context.Background(), maps.GeocodeRequest{
		Address:     " Avenida Paulista ",
		CountryCode: "br",
		Language:    "pt-BR",
	})
	if err != nil {
		t.Fatalf("Geocode() error = %v", err)
	}
	if len(resp.Results) != 1 {
		t.Fatalf("len(results) = %d, want 1", len(resp.Results))
	}
	got := resp.Results[0]
	if got.Address != "Av. Paulista, Bela Vista, Sao Paulo - SP, Brazil" {
		t.Fatalf("address = %q", got.Address)
	}
	if got.PlaceID != "place_123" {
		t.Fatalf("place id = %q", got.PlaceID)
	}
	if got.Coordinates.Latitude != -23.561414 || got.Coordinates.Longitude != -46.655881 {
		t.Fatalf("coordinates = %+v", got.Coordinates)
	}
}

func TestGoogleProviderGeocodeZeroResults(t *testing.T) {
	server := googleGeocodeServer(t, `{"status":"ZERO_RESULTS","results":[]}`)
	defer server.Close()

	resp, err := (&maps.GoogleProvider{
		BaseURL:    server.URL,
		HTTPClient: server.Client(),
	}).Geocode(context.Background(), maps.GeocodeRequest{Address: "not found"})
	if err != nil {
		t.Fatalf("Geocode() error = %v", err)
	}
	if len(resp.Results) != 0 {
		t.Fatalf("len(results) = %d, want 0", len(resp.Results))
	}
}

func TestGoogleProviderGeocodeAPIError(t *testing.T) {
	server := googleGeocodeServer(t, `{
		"status":"REQUEST_DENIED",
		"error_message":"The provided API key is invalid."
	}`)
	defer server.Close()

	_, err := (&maps.GoogleProvider{
		APIKey:     "bad-key",
		BaseURL:    server.URL,
		HTTPClient: server.Client(),
	}).Geocode(context.Background(), maps.GeocodeRequest{Address: "Avenida Paulista"})
	if err == nil {
		t.Fatalf("expected API error")
	}
	var apiErr *maps.GoogleAPIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("error = %T %v, want GoogleAPIError", err, err)
	}
	if apiErr.Status != "REQUEST_DENIED" || apiErr.Message != "The provided API key is invalid." {
		t.Fatalf("api error = %+v", apiErr)
	}
}

func TestGoogleProviderGeocodeInvalidCoordinateResponse(t *testing.T) {
	server := googleGeocodeServer(t, `{
		"status":"OK",
		"results":[{
			"formatted_address":"Invalid",
			"geometry":{"location":{"lat":91,"lng":0}}
		}]
	}`)
	defer server.Close()

	_, err := (&maps.GoogleProvider{
		BaseURL:    server.URL,
		HTTPClient: server.Client(),
	}).Geocode(context.Background(), maps.GeocodeRequest{Address: "bad coordinate"})
	if !errors.Is(err, maps.ErrInvalidCoordinates) {
		t.Fatalf("Geocode() error = %v, want ErrInvalidCoordinates", err)
	}
}

func googleGeocodeServer(t *testing.T, body string) *httptest.Server {
	t.Helper()

	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Fatalf("method = %s, want GET", r.Method)
		}
		if r.URL.Path != "/maps/api/geocode/json" {
			t.Fatalf("path = %s, want /maps/api/geocode/json", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(body))
	}))
}
