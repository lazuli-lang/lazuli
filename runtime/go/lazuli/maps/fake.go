package maps

import (
	"context"
	"sync"
)

// NoopProvider validates requests and returns an empty successful response.
// Tests can use it when the code path requires a bound maps provider but does
// not inspect geocoding results.
type NoopProvider struct{}

var _ MapsProvider = NoopProvider{}

// Geocode implements Geocoder.
func (NoopProvider) Geocode(ctx context.Context, req GeocodeRequest) (GeocodeResponse, error) {
	if err := contextError(ctx); err != nil {
		return GeocodeResponse{}, err
	}
	if err := req.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	return GeocodeResponse{}, nil
}

// FakeProvider is a concurrency-safe recording geocoder for tests.
type FakeProvider struct {
	// GeocodeFunc handles requests when non-nil.
	GeocodeFunc func(context.Context, GeocodeRequest) (GeocodeResponse, error)
	// Err is returned after request validation and recording when GeocodeFunc is nil.
	Err error

	mu        sync.Mutex
	responses map[string]GeocodeResponse
	requests  []GeocodeRequest
}

var _ MapsProvider = (*FakeProvider)(nil)

// SetResponse seeds the response returned for address. The key is trimmed so
// tests can use the same value a real adapter would receive after validation.
func (p *FakeProvider) SetResponse(address string, response GeocodeResponse) {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.responses == nil {
		p.responses = make(map[string]GeocodeResponse)
	}
	p.responses[normalizeAddress(address)] = cloneResponse(response)
}

// Geocode implements Geocoder.
func (p *FakeProvider) Geocode(ctx context.Context, req GeocodeRequest) (GeocodeResponse, error) {
	if err := contextError(ctx); err != nil {
		return GeocodeResponse{}, err
	}
	if err := req.Validate(); err != nil {
		return GeocodeResponse{}, err
	}

	p.mu.Lock()
	p.requests = append(p.requests, req)
	fn := p.GeocodeFunc
	fakeErr := p.Err
	response, ok := p.responses[req.NormalizedAddress()]
	p.mu.Unlock()

	if fn != nil {
		response, err := fn(ctx, req)
		if err != nil {
			return GeocodeResponse{}, err
		}
		if err := response.Validate(); err != nil {
			return GeocodeResponse{}, err
		}
		return response, nil
	}
	if fakeErr != nil {
		return GeocodeResponse{}, fakeErr
	}
	if !ok {
		return GeocodeResponse{}, nil
	}
	response = cloneResponse(response)
	if err := response.Validate(); err != nil {
		return GeocodeResponse{}, err
	}
	return response, nil
}

// Requests returns a snapshot of validated requests passed to Geocode.
func (p *FakeProvider) Requests() []GeocodeRequest {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.requests == nil {
		return nil
	}
	return append([]GeocodeRequest(nil), p.requests...)
}

// Reset clears recorded requests and configured responses.
func (p *FakeProvider) Reset() {
	p.mu.Lock()
	defer p.mu.Unlock()

	p.requests = nil
	p.responses = nil
}

func normalizeAddress(address string) string {
	return GeocodeRequest{Address: address}.NormalizedAddress()
}

func cloneResponse(response GeocodeResponse) GeocodeResponse {
	if response.Results == nil {
		return response
	}
	response.Results = append([]GeocodeResult(nil), response.Results...)
	return response
}

func contextError(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	return ctx.Err()
}
