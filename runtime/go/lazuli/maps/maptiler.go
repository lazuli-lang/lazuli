package maps

import (
	"errors"
	"fmt"
	"net/url"
	"path"
	"strconv"
	"strings"
)

const (
	// DefaultMapTilerEndpointURL is the hosted MapTiler API root.
	DefaultMapTilerEndpointURL = "https://api.maptiler.com"

	mapTilerGeocodePath = "geocoding"
)

var (
	// ErrMapTilerAPIKeyRequired is returned when a MapTiler planning helper is
	// asked to build a request without the required API key.
	ErrMapTilerAPIKeyRequired = errors.New("lazuli/maps/maptiler: api_key_required")

	// ErrInvalidMapTilerRequest is returned when descriptor metadata is
	// structurally invalid.
	ErrInvalidMapTilerRequest = errors.New("lazuli/maps/maptiler: invalid_request")
)

// MapTilerGeocodeDescriptor describes deterministic MapTiler forward geocoding
// request metadata. It does not perform network calls.
type MapTilerGeocodeDescriptor struct {
	// APIKey is added to the planned URL as key.
	APIKey string

	// EndpointURL defaults to DefaultMapTilerEndpointURL. Set this to a custom
	// API root such as a local MapTiler Server /api endpoint when needed.
	EndpointURL string

	// Language optionally sets MapTiler's language query parameter. A request
	// Language overrides this descriptor default.
	Language string

	// Proximity optionally biases forward results near a coordinate or client IP.
	Proximity *MapTilerProximity

	// BBox optionally restricts results to a bounding box.
	BBox *MapTilerBoundingBox
}

// Normalize returns a descriptor copy with canonical endpoint and trimmed
// metadata.
func (d MapTilerGeocodeDescriptor) Normalize() (MapTilerGeocodeDescriptor, error) {
	endpoint, err := NormalizeMapTilerEndpointURL(d.EndpointURL)
	if err != nil {
		return MapTilerGeocodeDescriptor{}, err
	}
	d.EndpointURL = endpoint
	d.APIKey = strings.TrimSpace(d.APIKey)
	d.Language = strings.TrimSpace(d.Language)
	return d, nil
}

// Validate checks that descriptor metadata can produce a deterministic
// MapTiler geocoding request plan.
func (d MapTilerGeocodeDescriptor) Validate() error {
	normalized, err := d.Normalize()
	if err != nil {
		return err
	}
	if normalized.APIKey == "" {
		return fieldError("api_key", ErrMapTilerAPIKeyRequired)
	}
	if normalized.Proximity != nil {
		if err := normalized.Proximity.Validate(); err != nil {
			return fieldError("proximity", err)
		}
	}
	if normalized.BBox != nil {
		if err := normalized.BBox.Validate(); err != nil {
			return fieldError("bbox", err)
		}
	}
	return nil
}

// PlanGeocodeRequest builds a dry-run MapTiler forward geocoding request plan.
func (d MapTilerGeocodeDescriptor) PlanGeocodeRequest(req GeocodeRequest) (MapTilerGeocodePlan, error) {
	return PlanMapTilerGeocodeRequest(req, d)
}

// RedactedSummary returns descriptor metadata safe for diagnostics.
func (d MapTilerGeocodeDescriptor) RedactedSummary() MapTilerGeocodeDescriptorSummary {
	normalized, err := d.Normalize()
	if err != nil {
		normalized = d
		normalized.APIKey = strings.TrimSpace(d.APIKey)
		normalized.Language = strings.TrimSpace(d.Language)
	}

	summary := MapTilerGeocodeDescriptorSummary{
		EndpointURL: normalized.EndpointURL,
		APIKey:      RedactMapTilerAPIKey(normalized.APIKey),
		Language:    normalized.Language,
	}
	if normalized.Proximity != nil {
		summary.Proximity = normalized.Proximity.String()
	}
	if normalized.BBox != nil {
		summary.BBox = normalized.BBox.String()
	}
	return summary
}

// MapTilerGeocodeDescriptorSummary is safe to log or expose in diagnostics.
type MapTilerGeocodeDescriptorSummary struct {
	EndpointURL string
	APIKey      string
	Language    string
	Proximity   string
	BBox        string
}

// MapTilerGeocodePlan is a dry-run request plan for MapTiler geocoding.
type MapTilerGeocodePlan struct {
	URL         string
	RedactedURL string
	EndpointURL string
	Query       string
	Language    string
	CountryCode string
	Proximity   string
	BBox        string
}

// PlanMapTilerGeocodeRequest builds a MapTiler forward geocoding URL from the
// provider-neutral GeocodeRequest shape. It does not perform a network call.
func PlanMapTilerGeocodeRequest(req GeocodeRequest, descriptor MapTilerGeocodeDescriptor) (MapTilerGeocodePlan, error) {
	if err := req.Validate(); err != nil {
		return MapTilerGeocodePlan{}, err
	}
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return MapTilerGeocodePlan{}, err
	}
	if err := descriptor.Validate(); err != nil {
		return MapTilerGeocodePlan{}, err
	}

	endpoint, err := url.Parse(descriptor.EndpointURL)
	if err != nil {
		return MapTilerGeocodePlan{}, fmt.Errorf("lazuli/maps/maptiler: invalid endpoint url: %w", err)
	}

	query := req.NormalizedAddress()
	setMapTilerGeocodePath(endpoint, query)

	q := endpoint.Query()
	q.Set("key", descriptor.APIKey)
	if countryCode := strings.TrimSpace(req.CountryCode); countryCode != "" {
		q.Set("country", strings.ToUpper(countryCode))
	}
	language := strings.TrimSpace(req.Language)
	if language == "" {
		language = descriptor.Language
	}
	if language != "" {
		q.Set("language", language)
	}
	if descriptor.Proximity != nil {
		q.Set("proximity", descriptor.Proximity.String())
	}
	if descriptor.BBox != nil {
		q.Set("bbox", descriptor.BBox.String())
	}
	endpoint.RawQuery = q.Encode()

	target := endpoint.String()
	return MapTilerGeocodePlan{
		URL:         target,
		RedactedURL: RedactMapTilerURL(target),
		EndpointURL: descriptor.EndpointURL,
		Query:       query,
		Language:    language,
		CountryCode: strings.ToUpper(strings.TrimSpace(req.CountryCode)),
		Proximity:   queryStringValue(q, "proximity"),
		BBox:        queryStringValue(q, "bbox"),
	}, nil
}

// NormalizeMapTilerEndpointURL returns a canonical API root. Empty endpoint
// uses DefaultMapTilerEndpointURL.
func NormalizeMapTilerEndpointURL(endpoint string) (string, error) {
	raw := strings.TrimSpace(endpoint)
	if raw == "" {
		raw = DefaultMapTilerEndpointURL
	}
	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("lazuli/maps/maptiler: invalid endpoint url: %w", err)
	}
	if u.Scheme == "" || u.Host == "" {
		return "", fmt.Errorf("lazuli/maps/maptiler: invalid endpoint url %q", raw)
	}
	if u.RawQuery != "" || u.Fragment != "" {
		return "", fieldError("endpoint_url", fmt.Errorf("%w: endpoint must not include query or fragment", ErrInvalidMapTilerRequest))
	}

	cleanPath := path.Clean("/" + strings.TrimSpace(u.Path))
	if cleanPath == "/" {
		u.Path = ""
	} else {
		u.Path = cleanPath
	}
	u.RawPath = ""
	return u.String(), nil
}

// RedactMapTilerAPIKey returns a stable placeholder for non-empty API keys.
func RedactMapTilerAPIKey(apiKey string) string {
	if strings.TrimSpace(apiKey) == "" {
		return ""
	}
	return "[redacted]"
}

// RedactMapTilerURL redacts the key query parameter from a planned URL.
func RedactMapTilerURL(raw string) string {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
		return raw
	}
	q := u.Query()
	if q.Has("key") {
		q.Set("key", RedactMapTilerAPIKey(q.Get("key")))
		u.RawQuery = q.Encode()
	}
	return u.String()
}

// MapTilerProximity is a proximity bias for MapTiler geocoding. UseIP requests
// server-side IP geolocation; otherwise Latitude and Longitude are used.
type MapTilerProximity struct {
	Latitude  float64
	Longitude float64
	UseIP     bool
}

// Validate validates the proximity metadata.
func (p MapTilerProximity) Validate() error {
	if p.UseIP {
		return nil
	}
	return (Coordinates{Latitude: p.Latitude, Longitude: p.Longitude}).Validate()
}

// String renders p in the order expected by MapTiler.
func (p MapTilerProximity) String() string {
	if p.UseIP {
		return "ip"
	}
	return strings.Join([]string{
		formatMapTilerFloat(p.Longitude),
		formatMapTilerFloat(p.Latitude),
	}, ",")
}

// MapTilerBoundingBox is a bbox in [west,south,east,north] order.
type MapTilerBoundingBox struct {
	West  float64
	South float64
	East  float64
	North float64
}

// Validate validates that the bounding box uses WGS84 coordinates and ordered
// southwest/northeast corners.
func (b MapTilerBoundingBox) Validate() error {
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

// String renders b in the comma-separated order expected by MapTiler.
func (b MapTilerBoundingBox) String() string {
	return strings.Join([]string{
		formatMapTilerFloat(b.West),
		formatMapTilerFloat(b.South),
		formatMapTilerFloat(b.East),
		formatMapTilerFloat(b.North),
	}, ",")
}

func setMapTilerGeocodePath(u *url.URL, query string) {
	basePath := strings.TrimSuffix(u.Path, "/")
	if path.Base(basePath) != mapTilerGeocodePath {
		basePath = strings.TrimSuffix(basePath+"/"+mapTilerGeocodePath, "/")
	}

	escapedQuery := url.PathEscape(query) + ".json"
	u.Path = strings.TrimSuffix(basePath, "/") + "/" + query + ".json"
	u.RawPath = strings.TrimSuffix(basePath, "/") + "/" + escapedQuery
}

func queryStringValue(values url.Values, key string) string {
	if values == nil {
		return ""
	}
	return values.Get(key)
}

func formatMapTilerFloat(value float64) string {
	return strconv.FormatFloat(value, 'f', -1, 64)
}
