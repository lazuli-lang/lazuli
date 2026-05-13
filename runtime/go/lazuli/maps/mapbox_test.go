package maps_test

import (
	"errors"
	"net/http"
	"net/url"
	"testing"

	"lazuli.dev/runtime/lazuli/maps"
)

func TestMapboxGeocodeURLBuildsForwardRequest(t *testing.T) {
	t.Parallel()

	target, err := maps.MapboxGeocodeURL(
		maps.GeocodeRequest{
			Address:     " Avenida Paulista ",
			CountryCode: " br ",
			Language:    "pt-BR",
		},
		maps.MapboxGeocodeOptions{
			AccessToken: "test-token",
			BaseURL:     "https://mapbox.test",
			BBox: &maps.MapboxBoundingBox{
				MinLongitude: -46.8,
				MinLatitude:  -23.7,
				MaxLongitude: -46.5,
				MaxLatitude:  -23.4,
			},
		},
	)
	if err != nil {
		t.Fatalf("MapboxGeocodeURL() error = %v", err)
	}

	u, err := url.Parse(target)
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if u.Scheme != "https" || u.Host != "mapbox.test" {
		t.Fatalf("base = %s://%s, want https://mapbox.test", u.Scheme, u.Host)
	}
	if u.Path != "/search/geocode/v6/forward" {
		t.Fatalf("path = %s, want /search/geocode/v6/forward", u.Path)
	}

	q := u.Query()
	if got := q.Get("q"); got != "Avenida Paulista" {
		t.Fatalf("q = %q, want normalized address", got)
	}
	if got := q.Get("access_token"); got != "test-token" {
		t.Fatalf("access_token = %q, want test-token", got)
	}
	if got := q.Get("country"); got != "BR" {
		t.Fatalf("country = %q, want BR", got)
	}
	if got := q.Get("language"); got != "pt-BR" {
		t.Fatalf("language = %q, want pt-BR", got)
	}
	if got := q.Get("bbox"); got != "-46.8,-23.7,-46.5,-23.4" {
		t.Fatalf("bbox = %q, want Mapbox bbox order", got)
	}
}

func TestMapboxGeocodeURLRejectsMissingAccessToken(t *testing.T) {
	t.Parallel()

	_, err := maps.MapboxGeocodeURL(
		maps.GeocodeRequest{Address: "Avenida Paulista"},
		maps.MapboxGeocodeOptions{},
	)
	if !errors.Is(err, maps.ErrMapboxAccessTokenRequired) {
		t.Fatalf("MapboxGeocodeURL() error = %v, want ErrMapboxAccessTokenRequired", err)
	}
}

func TestMapboxGeocodeURLRejectsInvalidBBox(t *testing.T) {
	t.Parallel()

	_, err := maps.MapboxGeocodeURL(
		maps.GeocodeRequest{Address: "Avenida Paulista"},
		maps.MapboxGeocodeOptions{
			AccessToken: "test-token",
			BBox: &maps.MapboxBoundingBox{
				MinLongitude: -46.5,
				MinLatitude:  -23.7,
				MaxLongitude: -46.8,
				MaxLatitude:  -23.4,
			},
		},
	)
	if !errors.Is(err, maps.ErrInvalidBoundingBox) {
		t.Fatalf("MapboxGeocodeURL() error = %v, want ErrInvalidBoundingBox", err)
	}
}

func TestMapboxGeocodeResponseMapsToNeutralResponse(t *testing.T) {
	t.Parallel()

	payload := maps.MapboxGeocodeResponse{
		Features: []maps.MapboxGeocodeFeature{{
			ID: "feature-fallback",
			Geometry: maps.MapboxGeometry{
				Type:        "Point",
				Coordinates: []float64{-46.655881, -23.561414},
			},
			Properties: maps.MapboxGeocodeProperties{
				MapboxID:       "mapbox-place-1",
				FullAddress:    "Av. Paulista, Bela Vista, Sao Paulo - SP, Brazil",
				Name:           "Av. Paulista",
				PlaceFormatted: "Sao Paulo - SP, Brazil",
				MatchCode:      maps.MapboxMatchCode{Confidence: "high"},
			},
		}},
	}

	resp, err := payload.GeocodeResponse()
	if err != nil {
		t.Fatalf("GeocodeResponse() error = %v", err)
	}
	if len(resp.Results) != 1 {
		t.Fatalf("len(results) = %d, want 1", len(resp.Results))
	}
	got := resp.Results[0]
	if got.Address != "Av. Paulista, Bela Vista, Sao Paulo - SP, Brazil" {
		t.Fatalf("address = %q", got.Address)
	}
	if got.PlaceID != "mapbox-place-1" {
		t.Fatalf("place id = %q", got.PlaceID)
	}
	if got.Coordinates.Latitude != -23.561414 || got.Coordinates.Longitude != -46.655881 {
		t.Fatalf("coordinates = %+v", got.Coordinates)
	}
	if score := payload.Features[0].ConfidenceScore(); score != 0.8 {
		t.Fatalf("ConfidenceScore() = %v, want 0.8", score)
	}
}

func TestMapboxGeocodeResponseFallsBackToNameAndFeatureID(t *testing.T) {
	t.Parallel()

	payload := maps.MapboxGeocodeResponse{
		Features: []maps.MapboxGeocodeFeature{{
			ID: "feature-1",
			Geometry: maps.MapboxGeometry{
				Type:        "Point",
				Coordinates: []float64{-46.655881, -23.561414},
			},
			Properties: maps.MapboxGeocodeProperties{
				Name:           "Avenida Paulista",
				PlaceFormatted: "Sao Paulo - SP, Brazil",
			},
		}},
	}

	resp, err := payload.GeocodeResponse()
	if err != nil {
		t.Fatalf("GeocodeResponse() error = %v", err)
	}
	got := resp.Results[0]
	if got.Address != "Avenida Paulista, Sao Paulo - SP, Brazil" {
		t.Fatalf("address = %q", got.Address)
	}
	if got.PlaceID != "feature-1" {
		t.Fatalf("place id = %q, want feature id fallback", got.PlaceID)
	}
}

func TestMapboxGeocodeResponseRejectsInvalidCoordinates(t *testing.T) {
	t.Parallel()

	payload := maps.MapboxGeocodeResponse{
		Features: []maps.MapboxGeocodeFeature{{
			Geometry: maps.MapboxGeometry{
				Type:        "Point",
				Coordinates: []float64{-46.655881, 91},
			},
		}},
	}

	_, err := payload.GeocodeResponse()
	if !errors.Is(err, maps.ErrInvalidCoordinates) {
		t.Fatalf("GeocodeResponse() error = %v, want ErrInvalidCoordinates", err)
	}
}

func TestNormalizeMapboxConfidence(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input string
		want  float64
	}{
		{input: "exact", want: 1},
		{input: " HIGH ", want: 0.8},
		{input: "medium", want: 0.5},
		{input: "low", want: 0.2},
		{input: "unknown", want: 0},
		{input: "", want: 0},
	}

	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			if got := maps.NormalizeMapboxConfidence(tc.input); got != tc.want {
				t.Fatalf("NormalizeMapboxConfidence(%q) = %v, want %v", tc.input, got, tc.want)
			}
		})
	}
}

func TestNewMapboxAPIErrorMapsProviderNeutralCauses(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name       string
		statusCode int
		message    string
		want       error
	}{
		{
			name:       "unauthorized",
			statusCode: http.StatusUnauthorized,
			message:    "Not Authorized - Invalid Token",
			want:       maps.ErrProviderUnavailable,
		},
		{
			name:       "missing query",
			statusCode: http.StatusNotFound,
			message:    "No search text or structured input parameters were provided in the query.",
			want:       maps.ErrEmptyAddress,
		},
		{
			name:       "invalid bbox",
			statusCode: http.StatusUnprocessableEntity,
			message:    "BBox is not valid. Must be an array of format [minX, minY, maxX, maxY]",
			want:       maps.ErrInvalidBoundingBox,
		},
		{
			name:       "invalid bbox coordinate",
			statusCode: http.StatusUnprocessableEntity,
			message:    "BBox minX value must be a number between -180 and 180",
			want:       maps.ErrInvalidCoordinates,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := maps.NewMapboxAPIError(tc.statusCode, tc.message)
			if !errors.Is(err, tc.want) {
				t.Fatalf("NewMapboxAPIError() error = %v, want %v", err, tc.want)
			}
			var apiErr *maps.MapboxAPIError
			if !errors.As(err, &apiErr) {
				t.Fatalf("error = %T %v, want MapboxAPIError", err, err)
			}
			if apiErr.StatusCode != tc.statusCode || apiErr.Message != tc.message {
				t.Fatalf("api error = %+v", apiErr)
			}
		})
	}
}

func TestNewMapboxAPIErrorReturnsNilForSuccess(t *testing.T) {
	t.Parallel()

	if err := maps.NewMapboxAPIError(http.StatusOK, ""); err != nil {
		t.Fatalf("NewMapboxAPIError() error = %v, want nil", err)
	}
}
