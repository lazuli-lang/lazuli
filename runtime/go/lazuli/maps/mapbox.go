package maps

import (
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"strings"
)

const (
	defaultMapboxBaseURL = "https://api.mapbox.com"
	mapboxGeocodePath    = "/search/geocode/v6/forward"
)

var (
	// ErrMapboxAccessTokenRequired is returned when a Mapbox request helper is
	// asked to build a request without the required access token.
	ErrMapboxAccessTokenRequired = errors.New("lazuli/maps/mapbox: access_token_required")

	// ErrInvalidBoundingBox is wrapped when a Mapbox bbox has reversed corners.
	ErrInvalidBoundingBox = errors.New("lazuli/maps: invalid_bounding_box")
)

// MapboxGeocodeOptions are Mapbox-specific forward geocoding request options.
type MapboxGeocodeOptions struct {
	// AccessToken is the Mapbox access token added as access_token.
	AccessToken string

	// BaseURL defaults to https://api.mapbox.com. Tests can set it to a local
	// base URL; the helper appends the standard geocoding path when the URL has
	// no path.
	BaseURL string

	// BBox optionally restricts results to a bounding box.
	BBox *MapboxBoundingBox
}

// MapboxBoundingBox is a Mapbox bbox in [min_lon,min_lat,max_lon,max_lat] order.
type MapboxBoundingBox struct {
	MinLongitude float64
	MinLatitude  float64
	MaxLongitude float64
	MaxLatitude  float64
}

// Validate validates that the bounding box uses WGS84 coordinates and ordered
// southwest/northeast corners.
func (b MapboxBoundingBox) Validate() error {
	if err := (Coordinates{Latitude: b.MinLatitude, Longitude: b.MinLongitude}).Validate(); err != nil {
		return fieldError("min", err)
	}
	if err := (Coordinates{Latitude: b.MaxLatitude, Longitude: b.MaxLongitude}).Validate(); err != nil {
		return fieldError("max", err)
	}
	if b.MinLongitude > b.MaxLongitude {
		return fmt.Errorf("%w: min_longitude must be less than or equal to max_longitude", ErrInvalidBoundingBox)
	}
	if b.MinLatitude > b.MaxLatitude {
		return fmt.Errorf("%w: min_latitude must be less than or equal to max_latitude", ErrInvalidBoundingBox)
	}
	return nil
}

// String renders b in the comma-separated order expected by Mapbox.
func (b MapboxBoundingBox) String() string {
	return strings.Join([]string{
		formatMapboxFloat(b.MinLongitude),
		formatMapboxFloat(b.MinLatitude),
		formatMapboxFloat(b.MaxLongitude),
		formatMapboxFloat(b.MaxLatitude),
	}, ",")
}

// MapboxGeocodeURL builds a Mapbox v6 forward geocoding URL from the neutral
// GeocodeRequest shape. It does not perform a network call.
func MapboxGeocodeURL(req GeocodeRequest, options MapboxGeocodeOptions) (string, error) {
	if err := req.Validate(); err != nil {
		return "", err
	}
	token := strings.TrimSpace(options.AccessToken)
	if token == "" {
		return "", fieldError("access_token", ErrMapboxAccessTokenRequired)
	}
	if options.BBox != nil {
		if err := options.BBox.Validate(); err != nil {
			return "", fieldError("bbox", err)
		}
	}

	u, err := mapboxBaseURL(options.BaseURL)
	if err != nil {
		return "", err
	}
	if u.Path == "" || u.Path == "/" {
		u.Path = mapboxGeocodePath
	}

	q := u.Query()
	q.Set("q", req.NormalizedAddress())
	q.Set("access_token", token)
	if countryCode := strings.TrimSpace(req.CountryCode); countryCode != "" {
		q.Set("country", strings.ToUpper(countryCode))
	}
	if language := strings.TrimSpace(req.Language); language != "" {
		q.Set("language", language)
	}
	if options.BBox != nil {
		q.Set("bbox", options.BBox.String())
	}
	u.RawQuery = q.Encode()
	return u.String(), nil
}

func mapboxBaseURL(raw string) (url.URL, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		raw = defaultMapboxBaseURL
	}
	u, err := url.Parse(raw)
	if err != nil {
		return url.URL{}, fmt.Errorf("lazuli/maps/mapbox: invalid base url: %w", err)
	}
	if u.Scheme == "" || u.Host == "" {
		return url.URL{}, fmt.Errorf("lazuli/maps/mapbox: invalid base url %q", raw)
	}
	return *u, nil
}

// MapboxGeocodeResponse is the subset of a Mapbox v6 geocoding response needed
// to map candidates into Lazuli's provider-neutral GeocodeResponse.
type MapboxGeocodeResponse struct {
	Features []MapboxGeocodeFeature `json:"features"`
}

// GeocodeResponse converts r to the provider-neutral response shape.
func (r MapboxGeocodeResponse) GeocodeResponse() (GeocodeResponse, error) {
	response := GeocodeResponse{Results: make([]GeocodeResult, 0, len(r.Features))}
	for i, feature := range r.Features {
		result, err := feature.GeocodeResult()
		if err != nil {
			return GeocodeResponse{}, fieldError(fmt.Sprintf("features[%d]", i), err)
		}
		response.Results = append(response.Results, result)
	}
	if err := response.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	return response, nil
}

// MapboxGeocodeFeature is one GeoJSON feature from a Mapbox geocoding response.
type MapboxGeocodeFeature struct {
	ID         string                  `json:"id"`
	Geometry   MapboxGeometry          `json:"geometry"`
	Properties MapboxGeocodeProperties `json:"properties"`
}

// GeocodeResult converts f to one provider-neutral geocoding candidate.
func (f MapboxGeocodeFeature) GeocodeResult() (GeocodeResult, error) {
	coordinates, err := f.Geometry.Point()
	if err != nil {
		return GeocodeResult{}, fieldError("geometry", err)
	}

	result := GeocodeResult{
		Address:     f.Properties.FormattedAddress(),
		Coordinates: coordinates,
		PlaceID:     f.Properties.MapboxID,
	}
	if result.PlaceID == "" {
		result.PlaceID = f.ID
	}
	if err := result.Validate(); err != nil {
		return GeocodeResult{}, err
	}
	return result, nil
}

// ConfidenceScore returns the normalized confidence score for f.
func (f MapboxGeocodeFeature) ConfidenceScore() float64 {
	return NormalizeMapboxConfidence(f.Properties.MatchCode.Confidence)
}

// MapboxGeometry is the GeoJSON point geometry returned by Mapbox.
type MapboxGeometry struct {
	Type        string    `json:"type"`
	Coordinates []float64 `json:"coordinates"`
}

// Point converts Mapbox [longitude, latitude] GeoJSON coordinates to a
// WGS84 latitude/longitude pair.
func (g MapboxGeometry) Point() (Coordinates, error) {
	if len(g.Coordinates) < 2 {
		return Coordinates{}, fieldError("coordinates", ErrInvalidCoordinates)
	}
	coordinates := Coordinates{
		Latitude:  g.Coordinates[1],
		Longitude: g.Coordinates[0],
	}
	if err := coordinates.Validate(); err != nil {
		return Coordinates{}, err
	}
	return coordinates, nil
}

// MapboxGeocodeProperties is the subset of Mapbox feature properties used by
// the provider-neutral helpers.
type MapboxGeocodeProperties struct {
	MapboxID       string           `json:"mapbox_id"`
	FullAddress    string           `json:"full_address"`
	Name           string           `json:"name"`
	PlaceFormatted string           `json:"place_formatted"`
	MatchCode      MapboxMatchCode  `json:"match_code"`
	Coordinates    MapboxCoordinate `json:"coordinates"`
}

// FormattedAddress returns the best display address available in p.
func (p MapboxGeocodeProperties) FormattedAddress() string {
	fullAddress := strings.TrimSpace(p.FullAddress)
	if fullAddress != "" {
		return fullAddress
	}

	name := strings.TrimSpace(p.Name)
	placeFormatted := strings.TrimSpace(p.PlaceFormatted)
	switch {
	case name != "" && placeFormatted != "":
		return name + ", " + placeFormatted
	case name != "":
		return name
	default:
		return placeFormatted
	}
}

// MapboxCoordinate is the optional coordinate object in Mapbox feature
// properties. Geometry is used for provider-neutral result conversion.
type MapboxCoordinate struct {
	Longitude float64 `json:"longitude"`
	Latitude  float64 `json:"latitude"`
	Accuracy  string  `json:"accuracy"`
}

// MapboxMatchCode carries Mapbox Smart Address Match metadata.
type MapboxMatchCode struct {
	Confidence string `json:"confidence"`
}

// NormalizeMapboxConfidence maps Mapbox Smart Address Match confidence labels to
// a bounded provider-neutral score where exact is 1 and unknown is 0.
func NormalizeMapboxConfidence(confidence string) float64 {
	switch strings.ToLower(strings.TrimSpace(confidence)) {
	case "exact":
		return 1
	case "high":
		return 0.8
	case "medium":
		return 0.5
	case "low":
		return 0.2
	default:
		return 0
	}
}

// NewMapboxAPIError maps a Mapbox HTTP status and response message into a typed
// error. Successful 2xx statuses return nil.
func NewMapboxAPIError(statusCode int, message string) error {
	if statusCode >= http.StatusOK && statusCode < http.StatusMultipleChoices {
		return nil
	}
	return &MapboxAPIError{
		StatusCode: statusCode,
		Message:    strings.TrimSpace(message),
		Err:        mapboxAPIErrorCause(statusCode, message),
	}
}

// MapboxAPIError reports a non-success Mapbox Geocoding API status.
type MapboxAPIError struct {
	StatusCode int
	Message    string
	Err        error
}

// Error implements error.
func (e *MapboxAPIError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Message == "" {
		return fmt.Sprintf("lazuli/maps/mapbox: api status %d", e.StatusCode)
	}
	return fmt.Sprintf("lazuli/maps/mapbox: api status %d: %s", e.StatusCode, e.Message)
}

// Unwrap exposes the provider-neutral cause for errors.Is.
func (e *MapboxAPIError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

func mapboxAPIErrorCause(statusCode int, message string) error {
	message = strings.ToLower(strings.TrimSpace(message))
	switch statusCode {
	case http.StatusUnauthorized, http.StatusForbidden:
		return ErrProviderUnavailable
	case http.StatusNotFound:
		if strings.Contains(message, "no search text") || strings.Contains(message, "structured input") {
			return ErrEmptyAddress
		}
	case http.StatusUnprocessableEntity:
		if strings.Contains(message, "bbox") {
			if strings.Contains(message, "not valid") ||
				strings.Contains(message, "format") ||
				strings.Contains(message, "cannot be greater") {
				return ErrInvalidBoundingBox
			}
			return ErrInvalidCoordinates
		}
		if strings.Contains(message, "lon") ||
			strings.Contains(message, "lat") ||
			strings.Contains(message, "proximity") {
			return ErrInvalidCoordinates
		}
	}
	return nil
}

func formatMapboxFloat(value float64) string {
	return strconv.FormatFloat(value, 'f', -1, 64)
}
