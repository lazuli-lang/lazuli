package maps_test

import (
	"errors"
	"net/url"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/maps"
)

func TestPlanHereGeocodeRequestBuildsDescriptor(t *testing.T) {
	t.Parallel()

	plan, err := maps.PlanHereGeocodeRequest(
		maps.GeocodeRequest{
			Address:     " Avenida Paulista ",
			CountryCode: " br ",
			Language:    "pt-BR",
		},
		maps.HereGeocodeOptions{
			APIKey:   "clearly-invalid-here-key",
			Endpoint: "https://here.test",
			BBox: &maps.HereBoundingBox{
				West:  -46.8,
				South: -23.7,
				East:  -46.5,
				North: -23.4,
			},
		},
	)
	if err != nil {
		t.Fatalf("PlanHereGeocodeRequest() error = %v", err)
	}
	if plan.Method != "GET" {
		t.Fatalf("method = %q, want GET", plan.Method)
	}
	if plan.Endpoint != "https://here.test/v1/geocode" {
		t.Fatalf("endpoint = %q", plan.Endpoint)
	}
	if plan.APIKeyRedacted != "[REDACTED]" {
		t.Fatalf("APIKeyRedacted = %q", plan.APIKeyRedacted)
	}
	if plan.Metadata != (maps.HereGeocodeMetadata{
		CountryCode: "BR",
		Language:    "pt-BR",
		BBox:        "-46.8,-23.7,-46.5,-23.4",
	}) {
		t.Fatalf("metadata = %+v", plan.Metadata)
	}

	u, err := url.Parse(plan.URL)
	if err != nil {
		t.Fatalf("Parse(URL) error = %v", err)
	}
	if u.Scheme != "https" || u.Host != "here.test" || u.Path != "/v1/geocode" {
		t.Fatalf("target = %s://%s%s", u.Scheme, u.Host, u.Path)
	}
	q := u.Query()
	if got := q.Get("q"); got != "Avenida Paulista" {
		t.Fatalf("q = %q, want normalized address", got)
	}
	if got := q.Get("apiKey"); got != "clearly-invalid-here-key" {
		t.Fatalf("apiKey = %q", got)
	}
	if got := q.Get("lang"); got != "pt-BR" {
		t.Fatalf("lang = %q, want pt-BR", got)
	}
	if got := q["in"]; !reflect.DeepEqual(got, []string{"countryCode:BR", "bbox:-46.8,-23.7,-46.5,-23.4"}) {
		t.Fatalf("in = %#v", got)
	}
	if plan.RedactedURL == plan.URL {
		t.Fatalf("RedactedURL was not redacted")
	}
	redacted, err := url.Parse(plan.RedactedURL)
	if err != nil {
		t.Fatalf("Parse(RedactedURL) error = %v", err)
	}
	if got := redacted.Query().Get("apiKey"); got != "[REDACTED]" {
		t.Fatalf("redacted apiKey = %q", got)
	}
}

func TestNormalizeHereEndpoint(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name     string
		endpoint string
		want     string
	}{
		{
			name: "default",
			want: maps.DefaultHereGeocodeEndpoint,
		},
		{
			name:     "base URL",
			endpoint: " https://here.test/ ",
			want:     "https://here.test/v1/geocode",
		},
		{
			name:     "custom path",
			endpoint: "https://proxy.test/maps/here/geocode/?debug=1",
			want:     "https://proxy.test/maps/here/geocode",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			got, err := maps.NormalizeHereEndpoint(tc.endpoint)
			if err != nil {
				t.Fatalf("NormalizeHereEndpoint() error = %v", err)
			}
			if got != tc.want {
				t.Fatalf("NormalizeHereEndpoint() = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestValidateHereEndpointRejectsInvalidEndpoint(t *testing.T) {
	t.Parallel()

	cases := []string{
		"here.test/v1/geocode",
		"ftp://here.test/v1/geocode",
		"https://user:pass@here.test/v1/geocode",
		"https://here.test/v1/geocode#fragment",
	}
	for _, endpoint := range cases {
		t.Run(endpoint, func(t *testing.T) {
			t.Parallel()

			err := maps.ValidateHereEndpoint(endpoint)
			if !errors.Is(err, maps.ErrInvalidHereEndpoint) {
				t.Fatalf("ValidateHereEndpoint() error = %v, want ErrInvalidHereEndpoint", err)
			}
		})
	}
}

func TestPlanHereGeocodeRequestRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name    string
		req     maps.GeocodeRequest
		options maps.HereGeocodeOptions
		want    error
	}{
		{
			name:    "missing API key",
			req:     maps.GeocodeRequest{Address: "Avenida Paulista"},
			options: maps.HereGeocodeOptions{},
			want:    maps.ErrHereAPIKeyRequired,
		},
		{
			name: "invalid country",
			req: maps.GeocodeRequest{
				Address:     "Avenida Paulista",
				CountryCode: "BRA",
			},
			options: maps.HereGeocodeOptions{APIKey: "clearly-invalid-here-key"},
			want:    maps.ErrInvalidHereMetadata,
		},
		{
			name: "invalid language",
			req: maps.GeocodeRequest{
				Address:  "Avenida Paulista",
				Language: "pt BR",
			},
			options: maps.HereGeocodeOptions{APIKey: "clearly-invalid-here-key"},
			want:    maps.ErrInvalidHereMetadata,
		},
		{
			name: "invalid bbox",
			req:  maps.GeocodeRequest{Address: "Avenida Paulista"},
			options: maps.HereGeocodeOptions{
				APIKey: "clearly-invalid-here-key",
				BBox: &maps.HereBoundingBox{
					West:  -46.5,
					South: -23.7,
					East:  -46.8,
					North: -23.4,
				},
			},
			want: maps.ErrInvalidBoundingBox,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, err := maps.PlanHereGeocodeRequest(tc.req, tc.options)
			if !errors.Is(err, tc.want) {
				t.Fatalf("PlanHereGeocodeRequest() error = %v, want %v", err, tc.want)
			}
		})
	}
}

func TestNormalizeHereMetadata(t *testing.T) {
	t.Parallel()

	country, err := maps.NormalizeHereCountryCode(" br ")
	if err != nil {
		t.Fatalf("NormalizeHereCountryCode() error = %v", err)
	}
	if country != "BR" {
		t.Fatalf("country = %q, want BR", country)
	}
	language, err := maps.NormalizeHereLanguage(" pt-BR ")
	if err != nil {
		t.Fatalf("NormalizeHereLanguage() error = %v", err)
	}
	if language != "pt-BR" {
		t.Fatalf("language = %q, want pt-BR", language)
	}
}

func TestRedactHereGeocodeURL(t *testing.T) {
	t.Parallel()

	target := "https://here.test/v1/geocode?apiKey=clearly-invalid-here-key&q=Avenida+Paulista"
	redacted := maps.RedactHereGeocodeURL(target)
	u, err := url.Parse(redacted)
	if err != nil {
		t.Fatalf("Parse() error = %v", err)
	}
	if got := u.Query().Get("apiKey"); got != "[REDACTED]" {
		t.Fatalf("apiKey = %q", got)
	}
	if got := u.Query().Get("q"); got != "Avenida Paulista" {
		t.Fatalf("q = %q", got)
	}
}
