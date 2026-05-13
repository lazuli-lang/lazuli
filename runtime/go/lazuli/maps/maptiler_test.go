package maps_test

import (
	"errors"
	"net/url"
	"testing"

	"lazuli.dev/runtime/lazuli/maps"
)

func TestMapTilerGeocodeDescriptorNormalizeValidateAndPlan(t *testing.T) {
	t.Parallel()

	descriptor := maps.MapTilerGeocodeDescriptor{
		APIKey:      " invalid-placeholder-key ",
		EndpointURL: " https://maptiler.test/api/ ",
		Language:    "en",
		Proximity: &maps.MapTilerProximity{
			Latitude:  -23.561414,
			Longitude: -46.655881,
		},
		BBox: &maps.MapTilerBoundingBox{
			West:  -46.8,
			South: -23.7,
			East:  -46.5,
			North: -23.4,
		},
	}

	normalized, err := descriptor.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if normalized.EndpointURL != "https://maptiler.test/api" {
		t.Fatalf("EndpointURL = %q, want normalized endpoint", normalized.EndpointURL)
	}
	if normalized.APIKey != "invalid-placeholder-key" {
		t.Fatalf("APIKey = %q, want trimmed placeholder key", normalized.APIKey)
	}
	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	plan, err := normalized.PlanGeocodeRequest(maps.GeocodeRequest{
		Address:     " Avenida Paulista ",
		CountryCode: " br ",
		Language:    "pt",
	})
	if err != nil {
		t.Fatalf("PlanGeocodeRequest() error = %v", err)
	}

	u, err := url.Parse(plan.URL)
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if u.Scheme != "https" || u.Host != "maptiler.test" {
		t.Fatalf("base = %s://%s, want https://maptiler.test", u.Scheme, u.Host)
	}
	if u.EscapedPath() != "/api/geocoding/Avenida%20Paulista.json" {
		t.Fatalf("path = %q, want escaped MapTiler forward geocode path", u.EscapedPath())
	}

	q := u.Query()
	if got := q.Get("key"); got != "invalid-placeholder-key" {
		t.Fatalf("key = %q, want placeholder key", got)
	}
	if got := q.Get("country"); got != "BR" {
		t.Fatalf("country = %q, want BR", got)
	}
	if got := q.Get("language"); got != "pt" {
		t.Fatalf("language = %q, want request language override", got)
	}
	if got := q.Get("proximity"); got != "-46.655881,-23.561414" {
		t.Fatalf("proximity = %q, want lon,lat", got)
	}
	if got := q.Get("bbox"); got != "-46.8,-23.7,-46.5,-23.4" {
		t.Fatalf("bbox = %q, want west,south,east,north", got)
	}
	if q.Get("key") == "[redacted]" {
		t.Fatalf("planned URL key was redacted unexpectedly")
	}

	redacted, err := url.Parse(plan.RedactedURL)
	if err != nil {
		t.Fatalf("Parse(redacted) error = %v", err)
	}
	if got := redacted.Query().Get("key"); got != "[redacted]" {
		t.Fatalf("redacted key = %q, want [redacted]", got)
	}
	if plan.Query != "Avenida Paulista" || plan.Language != "pt" || plan.CountryCode != "BR" {
		t.Fatalf("plan metadata = %+v", plan)
	}
}

func TestMapTilerGeocodePlanUsesDefaultsAndExistingGeocodingPath(t *testing.T) {
	t.Parallel()

	plan, err := maps.PlanMapTilerGeocodeRequest(
		maps.GeocodeRequest{Address: "Zurich"},
		maps.MapTilerGeocodeDescriptor{
			APIKey:      "invalid-placeholder-key",
			EndpointURL: "http://localhost:3650/api/geocoding",
			Language:    "de",
			Proximity:   &maps.MapTilerProximity{UseIP: true},
		},
	)
	if err != nil {
		t.Fatalf("PlanMapTilerGeocodeRequest() error = %v", err)
	}

	u, err := url.Parse(plan.URL)
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if u.Path != "/api/geocoding/Zurich.json" {
		t.Fatalf("path = %q, want no duplicate geocoding segment", u.Path)
	}
	if got := u.Query().Get("language"); got != "de" {
		t.Fatalf("language = %q, want descriptor default", got)
	}
	if got := u.Query().Get("proximity"); got != "ip" {
		t.Fatalf("proximity = %q, want ip", got)
	}
}

func TestMapTilerRedactionHelpers(t *testing.T) {
	t.Parallel()

	if got := maps.RedactMapTilerAPIKey(""); got != "" {
		t.Fatalf("RedactMapTilerAPIKey(empty) = %q, want empty", got)
	}
	if got := maps.RedactMapTilerAPIKey("invalid-placeholder-key"); got != "[redacted]" {
		t.Fatalf("RedactMapTilerAPIKey() = %q, want [redacted]", got)
	}

	redacted := maps.RedactMapTilerURL("https://api.maptiler.com/geocoding/Zurich.json?key=invalid-placeholder-key&language=de")
	u, err := url.Parse(redacted)
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if got := u.Query().Get("key"); got != "[redacted]" {
		t.Fatalf("redacted key = %q, want [redacted]", got)
	}
	if got := u.Query().Get("language"); got != "de" {
		t.Fatalf("language = %q, want preserved query metadata", got)
	}
}

func TestMapTilerGeocodeDescriptorRedactedSummary(t *testing.T) {
	t.Parallel()

	summary := maps.MapTilerGeocodeDescriptor{
		APIKey:      "invalid-placeholder-key",
		EndpointURL: "https://maptiler.test/",
		Language:    "en",
		Proximity: &maps.MapTilerProximity{
			Latitude:  47.3774434,
			Longitude: 8.528509,
		},
		BBox: &maps.MapTilerBoundingBox{
			West:  5.9559,
			South: 45.818,
			East:  10.4921,
			North: 47.8084,
		},
	}.RedactedSummary()

	if summary.EndpointURL != "https://maptiler.test" {
		t.Fatalf("EndpointURL = %q, want normalized endpoint", summary.EndpointURL)
	}
	if summary.APIKey != "[redacted]" {
		t.Fatalf("APIKey = %q, want [redacted]", summary.APIKey)
	}
	if summary.Proximity != "8.528509,47.3774434" || summary.BBox != "5.9559,45.818,10.4921,47.8084" {
		t.Fatalf("summary metadata = %+v", summary)
	}
}

func TestMapTilerValidationRejectsInvalidDescriptors(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name       string
		descriptor maps.MapTilerGeocodeDescriptor
		want       error
	}{
		{
			name:       "missing key",
			descriptor: maps.MapTilerGeocodeDescriptor{},
			want:       maps.ErrMapTilerAPIKeyRequired,
		},
		{
			name: "invalid proximity",
			descriptor: maps.MapTilerGeocodeDescriptor{
				APIKey:    "invalid-placeholder-key",
				Proximity: &maps.MapTilerProximity{Latitude: 91},
			},
			want: maps.ErrInvalidCoordinates,
		},
		{
			name: "invalid bbox ordering",
			descriptor: maps.MapTilerGeocodeDescriptor{
				APIKey: "invalid-placeholder-key",
				BBox: &maps.MapTilerBoundingBox{
					West:  -46.5,
					South: -23.7,
					East:  -46.8,
					North: -23.4,
				},
			},
			want: maps.ErrInvalidBoundingBox,
		},
		{
			name: "endpoint query",
			descriptor: maps.MapTilerGeocodeDescriptor{
				APIKey:      "invalid-placeholder-key",
				EndpointURL: "https://maptiler.test?key=bad",
			},
			want: maps.ErrInvalidMapTilerRequest,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := tc.descriptor.Validate()
			if !errors.Is(err, tc.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tc.want)
			}
		})
	}
}
