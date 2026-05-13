package maps

import (
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"strings"
	"unicode"
)

const (
	// DefaultHereGeocodeEndpoint is HERE's forward geocoding endpoint.
	DefaultHereGeocodeEndpoint = "https://geocode.search.hereapi.com/v1/geocode"

	hereGeocodePath = "/v1/geocode"
)

var (
	// ErrHereAPIKeyRequired is returned when a HERE request helper is asked to
	// plan a request without the required API key.
	ErrHereAPIKeyRequired = errors.New("lazuli/maps/here: api_key_required")

	// ErrInvalidHereEndpoint is returned when a HERE endpoint is not an
	// absolute http(s) URL without embedded credentials.
	ErrInvalidHereEndpoint = errors.New("lazuli/maps/here: invalid_endpoint")

	// ErrInvalidHereMetadata is returned when provider-neutral metadata cannot
	// be represented safely in a HERE request descriptor.
	ErrInvalidHereMetadata = errors.New("lazuli/maps/here: invalid_metadata")
)

// HereGeocodeOptions are HERE-specific forward geocoding request options.
type HereGeocodeOptions struct {
	// APIKey is the HERE API key added as apiKey.
	APIKey string

	// Endpoint defaults to DefaultHereGeocodeEndpoint. Tests or private routing
	// layers can set a custom absolute URL; if the path is empty, /v1/geocode is
	// appended.
	Endpoint string

	// BBox optionally restricts results to a WGS84 bounding box.
	BBox *HereBoundingBox
}

// HereBoundingBox is a WGS84 bbox in west,south,east,north order.
type HereBoundingBox struct {
	West  float64
	South float64
	East  float64
	North float64
}

// Validate validates that the bounding box uses WGS84 coordinates and ordered
// southwest/northeast corners.
func (b HereBoundingBox) Validate() error {
	if err := (Coordinates{Latitude: b.South, Longitude: b.West}).Validate(); err != nil {
		return fieldError("southwest", err)
	}
	if err := (Coordinates{Latitude: b.North, Longitude: b.East}).Validate(); err != nil {
		return fieldError("northeast", err)
	}
	if b.West > b.East {
		return fmt.Errorf("%w: west must be less than or equal to east", ErrInvalidBoundingBox)
	}
	if b.South > b.North {
		return fmt.Errorf("%w: south must be less than or equal to north", ErrInvalidBoundingBox)
	}
	return nil
}

// QueryValue renders b in the comma-separated order expected by HERE's bbox
// spatial filter.
func (b HereBoundingBox) QueryValue() string {
	return strings.Join([]string{
		formatHereFloat(b.West),
		formatHereFloat(b.South),
		formatHereFloat(b.East),
		formatHereFloat(b.North),
	}, ",")
}

// HereGeocodeMetadata carries normalized optional request metadata.
type HereGeocodeMetadata struct {
	CountryCode string
	Language    string
	BBox        string
}

// HereGeocodePlan is a deterministic request descriptor. It performs no HTTP
// calls and carries redacted fields suitable for logs or diagnostics.
type HereGeocodePlan struct {
	Method         string
	URL            string
	RedactedURL    string
	Endpoint       string
	APIKeyRedacted string
	Metadata       HereGeocodeMetadata
}

// PlanHereGeocodeRequest builds a HERE forward-geocoding request descriptor
// from the provider-neutral GeocodeRequest shape. It does not perform a network
// call.
func PlanHereGeocodeRequest(req GeocodeRequest, options HereGeocodeOptions) (HereGeocodePlan, error) {
	if err := req.Validate(); err != nil {
		return HereGeocodePlan{}, err
	}

	apiKey := strings.TrimSpace(options.APIKey)
	if apiKey == "" {
		return HereGeocodePlan{}, fieldError("api_key", ErrHereAPIKeyRequired)
	}

	endpoint, err := NormalizeHereEndpoint(options.Endpoint)
	if err != nil {
		return HereGeocodePlan{}, err
	}
	language, err := NormalizeHereLanguage(req.Language)
	if err != nil {
		return HereGeocodePlan{}, fieldError("language", err)
	}
	countryCode, err := NormalizeHereCountryCode(req.CountryCode)
	if err != nil {
		return HereGeocodePlan{}, fieldError("country_code", err)
	}
	metadata := HereGeocodeMetadata{
		CountryCode: countryCode,
		Language:    language,
	}
	if options.BBox != nil {
		if err := options.BBox.Validate(); err != nil {
			return HereGeocodePlan{}, fieldError("bbox", err)
		}
		metadata.BBox = options.BBox.QueryValue()
	}

	u, err := url.Parse(endpoint)
	if err != nil {
		return HereGeocodePlan{}, fmt.Errorf("%w: %v", ErrInvalidHereEndpoint, err)
	}
	q := u.Query()
	q.Set("q", req.NormalizedAddress())
	q.Set("apiKey", apiKey)
	if metadata.Language != "" {
		q.Set("lang", metadata.Language)
	}
	if metadata.CountryCode != "" {
		q.Set("in", "countryCode:"+metadata.CountryCode)
	}
	if metadata.BBox != "" {
		q.Add("in", "bbox:"+metadata.BBox)
	}
	u.RawQuery = q.Encode()

	target := u.String()
	return HereGeocodePlan{
		Method:         "GET",
		URL:            target,
		RedactedURL:    RedactHereGeocodeURL(target),
		Endpoint:       endpoint,
		APIKeyRedacted: RedactHereAPIKey(apiKey),
		Metadata:       metadata,
	}, nil
}

// NormalizeHereEndpoint trims and canonicalizes a HERE geocoding endpoint.
func NormalizeHereEndpoint(endpoint string) (string, error) {
	raw := strings.TrimSpace(endpoint)
	if raw == "" {
		raw = DefaultHereGeocodeEndpoint
	}

	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrInvalidHereEndpoint, err)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return "", fmt.Errorf("%w: scheme must be http or https", ErrInvalidHereEndpoint)
	}
	if u.Host == "" {
		return "", fmt.Errorf("%w: host is required", ErrInvalidHereEndpoint)
	}
	if u.User != nil {
		return "", fmt.Errorf("%w: credentials are not allowed", ErrInvalidHereEndpoint)
	}
	if u.Fragment != "" {
		return "", fmt.Errorf("%w: fragment is not allowed", ErrInvalidHereEndpoint)
	}
	if u.Path == "" || u.Path == "/" {
		u.Path = hereGeocodePath
	}
	u.Path = strings.TrimRight(u.Path, "/")
	u.RawQuery = ""
	return u.String(), nil
}

// ValidateHereEndpoint checks whether endpoint can be used as a HERE geocoding
// endpoint descriptor.
func ValidateHereEndpoint(endpoint string) error {
	_, err := NormalizeHereEndpoint(endpoint)
	return err
}

// NormalizeHereCountryCode trims and canonicalizes an optional ISO 3166-1
// alpha-2 country hint.
func NormalizeHereCountryCode(countryCode string) (string, error) {
	clean := strings.ToUpper(strings.TrimSpace(countryCode))
	if clean == "" {
		return "", nil
	}
	if len(clean) != 2 {
		return "", fmt.Errorf("%w: country code must be ISO 3166-1 alpha-2", ErrInvalidHereMetadata)
	}
	for _, r := range clean {
		if r < 'A' || r > 'Z' {
			return "", fmt.Errorf("%w: country code must contain only letters", ErrInvalidHereMetadata)
		}
	}
	return clean, nil
}

// NormalizeHereLanguage trims an optional language hint and rejects control
// characters so the value can be safely placed on a URL.
func NormalizeHereLanguage(language string) (string, error) {
	clean := strings.TrimSpace(language)
	if clean == "" {
		return "", nil
	}
	for _, r := range clean {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			return "", fmt.Errorf("%w: language contains invalid characters", ErrInvalidHereMetadata)
		}
	}
	return clean, nil
}

// RedactHereAPIKey returns a stable placeholder for non-empty HERE API keys.
func RedactHereAPIKey(apiKey string) string {
	if strings.TrimSpace(apiKey) == "" {
		return ""
	}
	return "[REDACTED]"
}

// RedactHereGeocodeURL redacts the apiKey query parameter from a HERE request
// URL. Invalid URLs are returned unchanged.
func RedactHereGeocodeURL(rawURL string) string {
	u, err := url.Parse(rawURL)
	if err != nil {
		return rawURL
	}
	q := u.Query()
	if q.Get("apiKey") != "" {
		q.Set("apiKey", RedactHereAPIKey(q.Get("apiKey")))
		u.RawQuery = q.Encode()
	}
	return u.String()
}

func formatHereFloat(value float64) string {
	return strconv.FormatFloat(value, 'f', -1, 64)
}
