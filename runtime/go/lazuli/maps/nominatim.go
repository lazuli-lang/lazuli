package maps

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
	"unicode"
)

const (
	// DefaultNominatimBaseURL is the public OpenStreetMap Nominatim endpoint.
	DefaultNominatimBaseURL = "https://nominatim.openstreetmap.org"

	// NominatimMaxRequestsPerSecond is the public endpoint's polite upper bound.
	NominatimMaxRequestsPerSecond = 1

	// NominatimMinRequestInterval is the minimum interval implied by the polite
	// public endpoint rate. Helpers only expose this metadata; they do not sleep.
	NominatimMinRequestInterval = time.Second

	nominatimSearchPath = "/search"
)

var (
	// ErrNominatimUserAgentRequired is returned when a Nominatim request does
	// not identify the calling application with a usable User-Agent value.
	ErrNominatimUserAgentRequired = errors.New("lazuli/maps/nominatim: user_agent_required")

	// ErrInvalidNominatimUserAgent is returned when the User-Agent cannot be
	// safely placed on an HTTP request.
	ErrInvalidNominatimUserAgent = errors.New("lazuli/maps/nominatim: invalid_user_agent")

	// ErrInvalidNominatimRequest is returned when request helper options are
	// structurally invalid.
	ErrInvalidNominatimRequest = errors.New("lazuli/maps/nominatim: invalid_request")
)

// NominatimGeocodeRequest carries the provider-neutral geocode request plus
// Nominatim-specific request options. It is a helper shape only and does not
// perform network calls.
type NominatimGeocodeRequest struct {
	GeocodeRequest

	// UserAgent must identify the calling application. Stock library defaults
	// are rejected because the public Nominatim endpoint requires attribution.
	UserAgent string

	// BaseURL defaults to DefaultNominatimBaseURL. Tests or self-hosted
	// deployments can set a custom base URL; if the path is empty, /search is
	// appended.
	BaseURL string

	// Limit optionally sets Nominatim's result limit. Zero omits the parameter.
	Limit int
}

// Validate validates provider-neutral fields and Nominatim-specific request
// requirements.
func (r NominatimGeocodeRequest) Validate() error {
	if err := r.GeocodeRequest.Validate(); err != nil {
		return err
	}
	if err := ValidateNominatimUserAgent(r.UserAgent); err != nil {
		return err
	}
	if r.Limit < 0 {
		return fieldError("limit", fmt.Errorf("%w: must be non-negative", ErrInvalidNominatimRequest))
	}
	if _, err := r.baseURL(); err != nil {
		return err
	}
	return nil
}

// QueryURL returns the Nominatim forward-search URL for the request.
func (r NominatimGeocodeRequest) QueryURL() (string, error) {
	if err := r.Validate(); err != nil {
		return "", err
	}

	u, err := r.baseURL()
	if err != nil {
		return "", err
	}
	if u.Path == "" || u.Path == "/" {
		u.Path = nominatimSearchPath
	}

	q := u.Query()
	q.Set("q", r.NormalizedAddress())
	q.Set("format", "jsonv2")
	q.Set("addressdetails", "1")
	if r.Limit > 0 {
		q.Set("limit", strconv.Itoa(r.Limit))
	}
	if countryCode := strings.TrimSpace(r.CountryCode); countryCode != "" {
		q.Set("countrycodes", strings.ToLower(countryCode))
	}
	if language := strings.TrimSpace(r.Language); language != "" {
		q.Set("accept-language", language)
	}
	u.RawQuery = q.Encode()
	return u.String(), nil
}

// Headers returns request headers required by Nominatim's public endpoint.
func (r NominatimGeocodeRequest) Headers() (http.Header, error) {
	if err := ValidateNominatimUserAgent(r.UserAgent); err != nil {
		return nil, err
	}
	headers := make(http.Header)
	headers.Set("User-Agent", strings.TrimSpace(r.UserAgent))
	return headers, nil
}

// RateMetadata returns polite public-endpoint throttle metadata for the request.
func (r NominatimGeocodeRequest) RateMetadata() NominatimRateMetadata {
	return DefaultNominatimRateMetadata()
}

func (r NominatimGeocodeRequest) baseURL() (url.URL, error) {
	raw := strings.TrimSpace(r.BaseURL)
	if raw == "" {
		raw = DefaultNominatimBaseURL
	}
	u, err := url.Parse(raw)
	if err != nil {
		return url.URL{}, fmt.Errorf("lazuli/maps/nominatim: invalid base url: %w", err)
	}
	if u.Scheme == "" || u.Host == "" {
		return url.URL{}, fmt.Errorf("lazuli/maps/nominatim: invalid base url %q", raw)
	}
	return *u, nil
}

// ValidateNominatimUserAgent validates the application identifier required by
// the public Nominatim endpoint.
func ValidateNominatimUserAgent(userAgent string) error {
	clean := strings.TrimSpace(userAgent)
	if clean == "" {
		return fieldError("user_agent", ErrNominatimUserAgentRequired)
	}
	for _, r := range clean {
		if unicode.IsControl(r) {
			return fieldError("user_agent", fmt.Errorf("%w: contains control characters", ErrInvalidNominatimUserAgent))
		}
	}
	if isStockNominatimUserAgent(clean) {
		return fieldError("user_agent", fmt.Errorf("%w: stock HTTP client User-Agent is not allowed", ErrNominatimUserAgentRequired))
	}
	return nil
}

func isStockNominatimUserAgent(userAgent string) bool {
	clean := strings.ToLower(strings.TrimSpace(userAgent))
	return strings.HasPrefix(clean, "go-http-client/") ||
		strings.HasPrefix(clean, "curl/") ||
		strings.HasPrefix(clean, "wget/") ||
		strings.HasPrefix(clean, "python-requests/") ||
		strings.HasPrefix(clean, "java/")
}

// NominatimRateMetadata describes public Nominatim throttle expectations. It is
// metadata for adapters and schedulers; this package does not enforce sleeping,
// queueing, or locking.
type NominatimRateMetadata struct {
	MaxRequestsPerSecond int
	MinInterval          time.Duration
	SingleThreaded       bool
	RequiresUserAgent    bool
}

// DefaultNominatimRateMetadata returns polite public-endpoint throttle metadata.
func DefaultNominatimRateMetadata() NominatimRateMetadata {
	return NominatimRateMetadata{
		MaxRequestsPerSecond: NominatimMaxRequestsPerSecond,
		MinInterval:          NominatimMinRequestInterval,
		SingleThreaded:       true,
		RequiresUserAgent:    true,
	}
}

// NominatimSearchResult is one JSONv2 result returned by Nominatim's search
// endpoint.
type NominatimSearchResult struct {
	PlaceID     int64             `json:"place_id"`
	OSMType     string            `json:"osm_type"`
	OSMID       int64             `json:"osm_id"`
	Lat         string            `json:"lat"`
	Lon         string            `json:"lon"`
	DisplayName string            `json:"display_name"`
	Class       string            `json:"class"`
	Category    string            `json:"category"`
	Type        string            `json:"type"`
	Importance  float64           `json:"importance"`
	Address     map[string]string `json:"address"`
}

// NominatimSearchResponse is the JSONv2 search response payload.
type NominatimSearchResponse []NominatimSearchResult

// Normalize converts a Nominatim search response into the provider-neutral
// geocoding response shape.
func (r NominatimSearchResponse) Normalize() (GeocodeResponse, error) {
	return NormalizeNominatimResults(r)
}

// DecodeNominatimSearchResponse decodes and normalizes a JSONv2 search response.
func DecodeNominatimSearchResponse(r io.Reader) (GeocodeResponse, error) {
	if r == nil {
		return GeocodeResponse{}, fmt.Errorf("lazuli/maps/nominatim: response body is required")
	}

	var payload NominatimSearchResponse
	if err := json.NewDecoder(io.LimitReader(r, 1<<20)).Decode(&payload); err != nil {
		return GeocodeResponse{}, fmt.Errorf("lazuli/maps/nominatim: decode response: %w", err)
	}
	return payload.Normalize()
}

// NormalizeNominatimResults converts Nominatim JSONv2 result entries into the
// provider-neutral geocoding response shape.
func NormalizeNominatimResults(results []NominatimSearchResult) (GeocodeResponse, error) {
	response := GeocodeResponse{Results: make([]GeocodeResult, 0, len(results))}
	for i, result := range results {
		normalized, err := result.Normalize()
		if err != nil {
			return GeocodeResponse{}, fieldError(fmt.Sprintf("results[%d]", i), err)
		}
		response.Results = append(response.Results, normalized)
	}
	if err := response.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	return response, nil
}

// Normalize converts one Nominatim JSONv2 result entry into a provider-neutral
// geocoding result.
func (r NominatimSearchResult) Normalize() (GeocodeResult, error) {
	latitude, err := parseNominatimCoordinate("lat", r.Lat)
	if err != nil {
		return GeocodeResult{}, err
	}
	longitude, err := parseNominatimCoordinate("lon", r.Lon)
	if err != nil {
		return GeocodeResult{}, err
	}

	result := GeocodeResult{
		Address: strings.TrimSpace(r.DisplayName),
		Coordinates: Coordinates{
			Latitude:  latitude,
			Longitude: longitude,
		},
		PlaceID: r.normalizedPlaceID(),
	}
	if result.Address == "" {
		result.Address = formatNominatimAddress(r.Address)
	}
	if err := result.Validate(); err != nil {
		return GeocodeResult{}, err
	}
	return result, nil
}

func (r NominatimSearchResult) normalizedPlaceID() string {
	if r.PlaceID != 0 {
		return strconv.FormatInt(r.PlaceID, 10)
	}
	osmType := strings.TrimSpace(r.OSMType)
	if osmType != "" && r.OSMID != 0 {
		return osmType + ":" + strconv.FormatInt(r.OSMID, 10)
	}
	return ""
}

func parseNominatimCoordinate(field, raw string) (float64, error) {
	clean := strings.TrimSpace(raw)
	if clean == "" {
		return 0, fieldError(field, fmt.Errorf("%w: coordinate is required", ErrInvalidCoordinates))
	}
	value, err := strconv.ParseFloat(clean, 64)
	if err != nil {
		return 0, fieldError(field, fmt.Errorf("%w: parse %q", ErrInvalidCoordinates, clean))
	}
	return value, nil
}

func formatNominatimAddress(address map[string]string) string {
	if len(address) == 0 {
		return ""
	}

	keys := []string{
		"house_number",
		"road",
		"neighbourhood",
		"suburb",
		"city",
		"town",
		"village",
		"municipality",
		"county",
		"state",
		"postcode",
		"country",
	}
	parts := make([]string, 0, len(keys))
	seen := make(map[string]struct{}, len(keys))
	for _, key := range keys {
		part := strings.TrimSpace(address[key])
		if part == "" {
			continue
		}
		if _, ok := seen[part]; ok {
			continue
		}
		seen[part] = struct{}{}
		parts = append(parts, part)
	}
	return strings.Join(parts, ", ")
}
