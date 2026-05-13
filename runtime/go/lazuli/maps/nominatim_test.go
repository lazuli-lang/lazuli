package maps_test

import (
	"errors"
	"net/url"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/maps"
)

func TestNominatimGeocodeRequestBuildsQueryURLAndHeaders(t *testing.T) {
	t.Parallel()

	request := maps.NominatimGeocodeRequest{
		GeocodeRequest: maps.GeocodeRequest{
			Address:     " Avenida Paulista ",
			CountryCode: "BR",
			Language:    "pt-BR",
		},
		UserAgent: "LazuliTest/1.0 (maps@example.test)",
		Limit:     2,
	}

	rawURL, err := request.QueryURL()
	if err != nil {
		t.Fatalf("QueryURL() error = %v", err)
	}
	parsed, err := url.Parse(rawURL)
	if err != nil {
		t.Fatalf("Parse(QueryURL()) error = %v", err)
	}
	if parsed.Scheme != "https" || parsed.Host != "nominatim.openstreetmap.org" || parsed.Path != "/search" {
		t.Fatalf("url = %s", rawURL)
	}

	query := parsed.Query()
	if got := query.Get("q"); got != "Avenida Paulista" {
		t.Fatalf("q = %q, want normalized address", got)
	}
	if got := query.Get("format"); got != "jsonv2" {
		t.Fatalf("format = %q, want jsonv2", got)
	}
	if got := query.Get("addressdetails"); got != "1" {
		t.Fatalf("addressdetails = %q, want 1", got)
	}
	if got := query.Get("countrycodes"); got != "br" {
		t.Fatalf("countrycodes = %q, want br", got)
	}
	if got := query.Get("accept-language"); got != "pt-BR" {
		t.Fatalf("accept-language = %q, want pt-BR", got)
	}
	if got := query.Get("limit"); got != "2" {
		t.Fatalf("limit = %q, want 2", got)
	}

	headers, err := request.Headers()
	if err != nil {
		t.Fatalf("Headers() error = %v", err)
	}
	if got := headers.Get("User-Agent"); got != "LazuliTest/1.0 (maps@example.test)" {
		t.Fatalf("User-Agent = %q", got)
	}
}

func TestNominatimGeocodeRequestPreservesCustomBaseURLPathAndQuery(t *testing.T) {
	t.Parallel()

	request := maps.NominatimGeocodeRequest{
		GeocodeRequest: maps.GeocodeRequest{Address: "Rua Oscar Freire"},
		UserAgent:      "LazuliTest/1.0 (maps@example.test)",
		BaseURL:        "https://nominatim.internal/search.php?email=ops%40example.test",
	}

	rawURL, err := request.QueryURL()
	if err != nil {
		t.Fatalf("QueryURL() error = %v", err)
	}
	parsed, err := url.Parse(rawURL)
	if err != nil {
		t.Fatalf("Parse(QueryURL()) error = %v", err)
	}
	if parsed.Path != "/search.php" {
		t.Fatalf("path = %q, want /search.php", parsed.Path)
	}
	if got := parsed.Query().Get("email"); got != "ops@example.test" {
		t.Fatalf("email = %q, want preserved query parameter", got)
	}
}

func TestNominatimUserAgentValidation(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name      string
		userAgent string
		wantErr   error
	}{
		{name: "empty", userAgent: " \t ", wantErr: maps.ErrNominatimUserAgentRequired},
		{name: "stock go", userAgent: "Go-http-client/1.1", wantErr: maps.ErrNominatimUserAgentRequired},
		{name: "control character", userAgent: "LazuliTest/1.0\nbad", wantErr: maps.ErrInvalidNominatimUserAgent},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := maps.ValidateNominatimUserAgent(tc.userAgent)
			if !errors.Is(err, tc.wantErr) {
				t.Fatalf("ValidateNominatimUserAgent() error = %v, want %v", err, tc.wantErr)
			}
		})
	}
}

func TestNominatimRateMetadata(t *testing.T) {
	t.Parallel()

	got := maps.DefaultNominatimRateMetadata()
	if got.MaxRequestsPerSecond != 1 {
		t.Fatalf("MaxRequestsPerSecond = %d, want 1", got.MaxRequestsPerSecond)
	}
	if got.MinInterval != time.Second {
		t.Fatalf("MinInterval = %s, want 1s", got.MinInterval)
	}
	if !got.SingleThreaded {
		t.Fatal("SingleThreaded = false, want true")
	}
	if !got.RequiresUserAgent {
		t.Fatal("RequiresUserAgent = false, want true")
	}
}

func TestNormalizeNominatimResults(t *testing.T) {
	t.Parallel()

	resp, err := maps.NormalizeNominatimResults([]maps.NominatimSearchResult{{
		PlaceID:     123,
		Lat:         " -23.561414 ",
		Lon:         " -46.655881 ",
		DisplayName: " Av. Paulista, Bela Vista, Sao Paulo - SP, Brazil ",
	}})
	if err != nil {
		t.Fatalf("NormalizeNominatimResults() error = %v", err)
	}
	if len(resp.Results) != 1 {
		t.Fatalf("len(results) = %d, want 1", len(resp.Results))
	}
	got := resp.Results[0]
	if got.Address != "Av. Paulista, Bela Vista, Sao Paulo - SP, Brazil" {
		t.Fatalf("Address = %q", got.Address)
	}
	if got.PlaceID != "123" {
		t.Fatalf("PlaceID = %q, want 123", got.PlaceID)
	}
	if got.Coordinates.Latitude != -23.561414 || got.Coordinates.Longitude != -46.655881 {
		t.Fatalf("Coordinates = %+v", got.Coordinates)
	}
}

func TestNormalizeNominatimResultFallsBackToAddressAndOSMID(t *testing.T) {
	t.Parallel()

	result, err := (maps.NominatimSearchResult{
		OSMType: "way",
		OSMID:   456,
		Lat:     "-23.561414",
		Lon:     "-46.655881",
		Address: map[string]string{
			"road":    "Avenida Paulista",
			"city":    "Sao Paulo",
			"state":   "Sao Paulo",
			"country": "Brazil",
		},
	}).Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if result.Address != "Avenida Paulista, Sao Paulo, Brazil" {
		t.Fatalf("Address = %q", result.Address)
	}
	if result.PlaceID != "way:456" {
		t.Fatalf("PlaceID = %q, want way:456", result.PlaceID)
	}
}

func TestNormalizeNominatimResultsRejectsInvalidCoordinates(t *testing.T) {
	t.Parallel()

	_, err := maps.NormalizeNominatimResults([]maps.NominatimSearchResult{{
		Lat: "91",
		Lon: "0",
	}})
	if !errors.Is(err, maps.ErrInvalidCoordinates) {
		t.Fatalf("NormalizeNominatimResults() error = %v, want ErrInvalidCoordinates", err)
	}
}

func TestDecodeNominatimSearchResponse(t *testing.T) {
	t.Parallel()

	resp, err := maps.DecodeNominatimSearchResponse(strings.NewReader(`[
		{
			"place_id": 987,
			"lat": "-23.561414",
			"lon": "-46.655881",
			"display_name": "Avenida Paulista, Sao Paulo, Brazil"
		}
	]`))
	if err != nil {
		t.Fatalf("DecodeNominatimSearchResponse() error = %v", err)
	}
	if len(resp.Results) != 1 {
		t.Fatalf("len(results) = %d, want 1", len(resp.Results))
	}
	if resp.Results[0].PlaceID != "987" {
		t.Fatalf("PlaceID = %q, want 987", resp.Results[0].PlaceID)
	}
}
