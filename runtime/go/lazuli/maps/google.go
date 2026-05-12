package maps

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

const (
	defaultGoogleMapsBaseURL = "https://maps.googleapis.com"
	googleGeocodePath        = "/maps/api/geocode/json"
)

// GoogleProvider geocodes addresses with the Google Maps Geocoding API.
type GoogleProvider struct {
	APIKey string

	// BaseURL defaults to https://maps.googleapis.com. Tests can set it to a
	// local server URL; the provider appends the standard geocoding path when
	// the URL has no path.
	BaseURL string

	// HTTPClient defaults to http.DefaultClient.
	HTTPClient *http.Client
}

var _ MapsProvider = (*GoogleProvider)(nil)

// Geocode implements Geocoder.
func (p *GoogleProvider) Geocode(ctx context.Context, req GeocodeRequest) (GeocodeResponse, error) {
	if err := contextError(ctx); err != nil {
		return GeocodeResponse{}, err
	}
	if err := req.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	if p == nil {
		return GeocodeResponse{}, ErrProviderUnavailable
	}
	if ctx == nil {
		ctx = context.Background()
	}

	target, err := p.geocodeURL(req)
	if err != nil {
		return GeocodeResponse{}, err
	}
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodGet, target, nil)
	if err != nil {
		return GeocodeResponse{}, err
	}
	resp, err := p.httpClient().Do(httpReq)
	if err != nil {
		return GeocodeResponse{}, err
	}
	defer resp.Body.Close()

	if resp.StatusCode < http.StatusOK || resp.StatusCode >= http.StatusMultipleChoices {
		return GeocodeResponse{}, fmt.Errorf("lazuli/maps/google: http status %d", resp.StatusCode)
	}

	var payload googleGeocodeResponse
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&payload); err != nil {
		return GeocodeResponse{}, fmt.Errorf("lazuli/maps/google: decode response: %w", err)
	}
	if payload.Status == "ZERO_RESULTS" {
		return GeocodeResponse{}, nil
	}
	if payload.Status != "OK" {
		return GeocodeResponse{}, &GoogleAPIError{Status: payload.Status, Message: payload.ErrorMessage}
	}

	response := GeocodeResponse{Results: make([]GeocodeResult, 0, len(payload.Results))}
	for _, result := range payload.Results {
		response.Results = append(response.Results, GeocodeResult{
			Address: result.FormattedAddress,
			Coordinates: Coordinates{
				Latitude:  result.Geometry.Location.Latitude,
				Longitude: result.Geometry.Location.Longitude,
			},
			PlaceID: result.PlaceID,
		})
	}
	if err := response.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	return response, nil
}

func (p *GoogleProvider) geocodeURL(req GeocodeRequest) (string, error) {
	u, err := p.baseURL()
	if err != nil {
		return "", err
	}
	if u.Path == "" || u.Path == "/" {
		u.Path = googleGeocodePath
	}
	q := u.Query()
	q.Set("address", req.NormalizedAddress())
	if p.APIKey != "" {
		q.Set("key", p.APIKey)
	}
	if countryCode := strings.TrimSpace(req.CountryCode); countryCode != "" {
		q.Set("components", "country:"+strings.ToUpper(countryCode))
	}
	if language := strings.TrimSpace(req.Language); language != "" {
		q.Set("language", language)
	}
	u.RawQuery = q.Encode()
	return u.String(), nil
}

func (p *GoogleProvider) baseURL() (url.URL, error) {
	raw := strings.TrimSpace(p.BaseURL)
	if raw == "" {
		raw = defaultGoogleMapsBaseURL
	}
	u, err := url.Parse(raw)
	if err != nil {
		return url.URL{}, fmt.Errorf("lazuli/maps/google: invalid base url: %w", err)
	}
	if u.Scheme == "" || u.Host == "" {
		return url.URL{}, fmt.Errorf("lazuli/maps/google: invalid base url %q", raw)
	}
	return *u, nil
}

func (p *GoogleProvider) httpClient() *http.Client {
	if p != nil && p.HTTPClient != nil {
		return p.HTTPClient
	}
	return http.DefaultClient
}

// GoogleAPIError reports a non-success Google Geocoding API status.
type GoogleAPIError struct {
	Status  string
	Message string
}

// Error implements error.
func (e *GoogleAPIError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Message == "" {
		return "lazuli/maps/google: api status " + e.Status
	}
	return fmt.Sprintf("lazuli/maps/google: api status %s: %s", e.Status, e.Message)
}

type googleGeocodeResponse struct {
	Status       string                `json:"status"`
	ErrorMessage string                `json:"error_message"`
	Results      []googleGeocodeResult `json:"results"`
}

type googleGeocodeResult struct {
	FormattedAddress string `json:"formatted_address"`
	PlaceID          string `json:"place_id"`
	Geometry         struct {
		Location struct {
			Latitude  float64 `json:"lat"`
			Longitude float64 `json:"lng"`
		} `json:"location"`
	} `json:"geometry"`
}
