// Package testkit contains stdlib-only fakes for Lazuli runtime tests.
package testkit

import (
	"bytes"
	"context"
	"errors"
	"io"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/cache"
)

var errNilRequest = errors.New("lazuli/testkit: nil request")

// RoundTripper is a recording http.RoundTripper fake.
//
// When RoundTripFunc is set it receives the request after the fake has recorded
// it. Otherwise RoundTripper returns Err, or a synthetic response built from
// StatusCode, Header, and Body.
type RoundTripper struct {
	// RoundTripFunc handles requests when non-nil.
	RoundTripFunc func(*http.Request) (*http.Response, error)
	// Err is returned when RoundTripFunc is nil.
	Err error
	// StatusCode is the synthetic response status. It defaults to 200.
	StatusCode int
	// Header is copied onto synthetic responses.
	Header http.Header
	// Body is copied into each synthetic response body.
	Body []byte

	mu       sync.Mutex
	requests []recordedRequest
}

type recordedRequest struct {
	request *http.Request
	body    []byte
	hasBody bool
}

var _ http.RoundTripper = (*RoundTripper)(nil)

// Client returns an http.Client that uses rt as its Transport.
func (rt *RoundTripper) Client() *http.Client {
	return &http.Client{Transport: rt}
}

// RoundTrip records req and returns the configured fake response.
func (rt *RoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
	record, err := recordRequest(req)
	if err != nil {
		return nil, err
	}
	if req.Body != nil {
		defer req.Body.Close()
	}

	rt.mu.Lock()
	rt.requests = append(rt.requests, record)
	fn := rt.RoundTripFunc
	fakeErr := rt.Err
	statusCode := rt.StatusCode
	header := cloneHeader(rt.Header)
	body := cloneBytes(rt.Body)
	rt.mu.Unlock()

	if fn != nil {
		return fn(req)
	}
	if fakeErr != nil {
		return nil, fakeErr
	}
	if statusCode == 0 {
		statusCode = http.StatusOK
	}
	return &http.Response{
		StatusCode:    statusCode,
		Status:        responseStatus(statusCode),
		Header:        header,
		Body:          io.NopCloser(bytes.NewReader(body)),
		ContentLength: int64(len(body)),
		Request:       req,
	}, nil
}

// Requests returns snapshots of requests passed to RoundTrip.
func (rt *RoundTripper) Requests() []*http.Request {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	requests := make([]*http.Request, len(rt.requests))
	for i, record := range rt.requests {
		requests[i] = cloneRecordedRequest(record)
	}
	return requests
}

// Reset clears recorded requests.
func (rt *RoundTripper) Reset() {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.requests = nil
}

// EventRecord is one event delivered to an EventRecorder.
type EventRecord struct {
	// Context is the context passed to EventRecorder.Subscriber.
	Context context.Context
	// Event is a defensive copy of the delivered event envelope.
	Event lazuli.Event
}

// EventRecorder records events delivered through its Subscriber method.
type EventRecorder struct {
	// Err is returned by Subscriber after the event is recorded.
	Err error

	mu      sync.Mutex
	records []EventRecord
}

// Subscriber records e and returns the recorder's configured Err.
func (r *EventRecorder) Subscriber(ctx context.Context, e lazuli.Event) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	r.records = append(r.records, EventRecord{
		Context: ctx,
		Event:   cloneEvent(e),
	})
	return r.Err
}

// Records returns the events delivered to Subscriber in order.
func (r *EventRecorder) Records() []EventRecord {
	r.mu.Lock()
	defer r.mu.Unlock()

	records := make([]EventRecord, len(r.records))
	for i, record := range r.records {
		records[i] = EventRecord{
			Context: record.Context,
			Event:   cloneEvent(record.Event),
		}
	}
	return records
}

// Events returns just the event envelopes delivered to Subscriber in order.
func (r *EventRecorder) Events() []lazuli.Event {
	records := r.Records()
	events := make([]lazuli.Event, len(records))
	for i, record := range records {
		events[i] = record.Event
	}
	return events
}

// Last returns the most recent event delivered to Subscriber.
func (r *EventRecorder) Last() (lazuli.Event, bool) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if len(r.records) == 0 {
		return lazuli.Event{}, false
	}
	return cloneEvent(r.records[len(r.records)-1].Event), true
}

// Reset clears recorded events.
func (r *EventRecorder) Reset() {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.records = nil
}

// CachePutCall records one ByteCacheBackend Put call.
type CachePutCall struct {
	// Key is the cache key passed to Put.
	Key string
	// Value is a defensive copy of the bytes passed to Put.
	Value []byte
	// TTL is the entry lifetime passed to Put.
	TTL time.Duration
	// Tags is a defensive copy of the invalidation tags passed to Put.
	Tags []string
}

// ByteCacheBackend is a recording cache.Backend fake for byte payload tests.
//
// It stores defensive copies of values. TTLs are recorded but not enforced.
type ByteCacheBackend struct {
	// GetErr is returned by Get after recording the call.
	GetErr error
	// PutErr is returned by Put after recording the call. Failed puts are not stored.
	PutErr error
	// InvalidateQueriesErr is returned by InvalidateQueries after recording the call.
	InvalidateQueriesErr error
	// InvalidateTagsErr is returned by InvalidateTags after recording the call.
	InvalidateTagsErr error
	// StatsErr is returned by Stats together with the current or overridden stats.
	StatsErr error
	// StatsOverride, when non-nil, is returned by Stats instead of derived stats.
	StatsOverride *cache.QueryStats

	mu sync.Mutex

	values map[string][]byte
	tags   map[string][]string

	getCalls               []string
	putCalls               []CachePutCall
	invalidateQueriesCalls [][]string
	invalidateTagsCalls    [][]string

	hits   uint64
	misses uint64
	evicts uint64
}

var _ cache.Backend = (*ByteCacheBackend)(nil)

// Seed stores value under key without recording a Put call.
func (b *ByteCacheBackend) Seed(key string, value []byte, tags ...string) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.ensureMapsLocked()
	b.putLocked(key, value, tags)
}

// SeedString stores value under key without recording a Put call.
func (b *ByteCacheBackend) SeedString(key, value string, tags ...string) {
	b.Seed(key, []byte(value), tags...)
}

// Value returns a defensive copy of the stored value for key.
func (b *ByteCacheBackend) Value(key string) ([]byte, bool) {
	b.mu.Lock()
	defer b.mu.Unlock()

	value, ok := b.values[key]
	if !ok {
		return nil, false
	}
	return cloneBytes(value), true
}

// Get implements cache.Backend.
func (b *ByteCacheBackend) Get(ctx context.Context, key string) ([]byte, bool, error) {
	if err := contextError(ctx); err != nil {
		return nil, false, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()
	b.ensureMapsLocked()
	b.getCalls = append(b.getCalls, key)

	if b.GetErr != nil {
		return nil, false, b.GetErr
	}
	value, ok := b.values[key]
	if !ok {
		b.misses++
		return nil, false, nil
	}
	b.hits++
	return cloneBytes(value), true, nil
}

// Put implements cache.Backend.
func (b *ByteCacheBackend) Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error {
	if err := contextError(ctx); err != nil {
		return err
	}

	b.mu.Lock()
	defer b.mu.Unlock()
	b.ensureMapsLocked()
	b.putCalls = append(b.putCalls, CachePutCall{
		Key:   key,
		Value: cloneBytes(value),
		TTL:   ttl,
		Tags:  cloneStrings(tags),
	})

	if b.PutErr != nil {
		return b.PutErr
	}
	b.putLocked(key, value, tags)
	return nil
}

// InvalidateQueries implements cache.Backend.
func (b *ByteCacheBackend) InvalidateQueries(ctx context.Context, names []string) (int, error) {
	if err := contextError(ctx); err != nil {
		return 0, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()
	b.ensureMapsLocked()
	b.invalidateQueriesCalls = append(b.invalidateQueriesCalls, cloneStrings(names))

	if b.InvalidateQueriesErr != nil {
		return 0, b.InvalidateQueriesErr
	}

	var deleted int
	for _, name := range names {
		if name == "" {
			continue
		}
		prefix := name + "|"
		for key := range b.values {
			if strings.HasPrefix(key, prefix) {
				b.deleteLocked(key)
				deleted++
			}
		}
	}
	b.evicts += uint64(deleted)
	return deleted, nil
}

// InvalidateTags implements cache.Backend.
func (b *ByteCacheBackend) InvalidateTags(ctx context.Context, labels []string) (int, error) {
	if err := contextError(ctx); err != nil {
		return 0, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()
	b.ensureMapsLocked()
	b.invalidateTagsCalls = append(b.invalidateTagsCalls, cloneStrings(labels))

	if b.InvalidateTagsErr != nil {
		return 0, b.InvalidateTagsErr
	}

	labelSet := stringSet(labels)
	var deleted int
	for key, tags := range b.tags {
		if intersects(tags, labelSet) {
			b.deleteLocked(key)
			deleted++
		}
	}
	b.evicts += uint64(deleted)
	return deleted, nil
}

// Stats implements cache.Backend.
func (b *ByteCacheBackend) Stats(ctx context.Context) (cache.QueryStats, error) {
	if err := contextError(ctx); err != nil {
		return cache.QueryStats{}, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()
	b.ensureMapsLocked()

	stats := cache.QueryStats{
		Entries: uint64(len(b.values)),
		Hits:    b.hits,
		Misses:  b.misses,
		Evicts:  b.evicts,
	}
	if b.StatsOverride != nil {
		stats = *b.StatsOverride
	}
	return stats, b.StatsErr
}

// GetCalls returns keys passed to Get in order.
func (b *ByteCacheBackend) GetCalls() []string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return cloneStrings(b.getCalls)
}

// PutCalls returns Put calls in order.
func (b *ByteCacheBackend) PutCalls() []CachePutCall {
	b.mu.Lock()
	defer b.mu.Unlock()

	calls := make([]CachePutCall, len(b.putCalls))
	for i, call := range b.putCalls {
		calls[i] = CachePutCall{
			Key:   call.Key,
			Value: cloneBytes(call.Value),
			TTL:   call.TTL,
			Tags:  cloneStrings(call.Tags),
		}
	}
	return calls
}

// InvalidateQueriesCalls returns query invalidation calls in order.
func (b *ByteCacheBackend) InvalidateQueriesCalls() [][]string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return cloneStringSlices(b.invalidateQueriesCalls)
}

// InvalidateTagsCalls returns tag invalidation calls in order.
func (b *ByteCacheBackend) InvalidateTagsCalls() [][]string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return cloneStringSlices(b.invalidateTagsCalls)
}

// ResetCalls clears recorded calls while preserving cached values and stats.
func (b *ByteCacheBackend) ResetCalls() {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.getCalls = nil
	b.putCalls = nil
	b.invalidateQueriesCalls = nil
	b.invalidateTagsCalls = nil
}

func (b *ByteCacheBackend) ensureMapsLocked() {
	if b.values == nil {
		b.values = make(map[string][]byte)
	}
	if b.tags == nil {
		b.tags = make(map[string][]string)
	}
}

func (b *ByteCacheBackend) putLocked(key string, value []byte, tags []string) {
	if _, ok := b.values[key]; ok {
		b.deleteLocked(key)
	}
	b.values[key] = cloneBytes(value)
	b.tags[key] = cleanStrings(tags)
}

func (b *ByteCacheBackend) deleteLocked(key string) {
	delete(b.values, key)
	delete(b.tags, key)
}

func recordRequest(req *http.Request) (recordedRequest, error) {
	if req == nil {
		return recordedRequest{}, errNilRequest
	}

	record := recordedRequest{request: req.Clone(req.Context())}
	if req.Body == nil {
		return record, nil
	}

	body, err := io.ReadAll(req.Body)
	if err != nil {
		return recordedRequest{}, err
	}
	_ = req.Body.Close()
	req.Body = io.NopCloser(bytes.NewReader(body))
	req.GetBody = func() (io.ReadCloser, error) {
		return io.NopCloser(bytes.NewReader(body)), nil
	}

	record.body = cloneBytes(body)
	record.hasBody = true
	record.request = cloneRecordedRequest(record)
	return record, nil
}

func cloneRecordedRequest(record recordedRequest) *http.Request {
	req := record.request.Clone(record.request.Context())
	if record.hasBody {
		body := cloneBytes(record.body)
		req.Body = io.NopCloser(bytes.NewReader(body))
		req.GetBody = func() (io.ReadCloser, error) {
			return io.NopCloser(bytes.NewReader(body)), nil
		}
	} else {
		req.Body = nil
		req.GetBody = nil
	}
	return req
}

func responseStatus(code int) string {
	text := http.StatusText(code)
	if text == "" {
		return strconv.Itoa(code)
	}
	return strconv.Itoa(code) + " " + text
}

func cloneEvent(e lazuli.Event) lazuli.Event {
	if e.Tenant != nil {
		tenant := *e.Tenant
		e.Tenant = &tenant
	}
	if e.UserID != nil {
		userID := *e.UserID
		e.UserID = &userID
	}
	if e.Payload != nil {
		payload := make(map[string]any, len(e.Payload))
		for key, value := range e.Payload {
			payload[key] = value
		}
		e.Payload = payload
	}
	return e
}

func cloneBytes(value []byte) []byte {
	if value == nil {
		return nil
	}
	return append([]byte(nil), value...)
}

func cloneHeader(header http.Header) http.Header {
	if header == nil {
		return nil
	}
	return header.Clone()
}

func cloneStrings(values []string) []string {
	if values == nil {
		return nil
	}
	return append([]string(nil), values...)
}

func cloneStringSlices(values [][]string) [][]string {
	if values == nil {
		return nil
	}
	clone := make([][]string, len(values))
	for i, value := range values {
		clone[i] = cloneStrings(value)
	}
	return clone
}

func cleanStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(values))
	cleaned := make([]string, 0, len(values))
	for _, value := range values {
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		cleaned = append(cleaned, value)
	}
	return cleaned
}

func stringSet(values []string) map[string]struct{} {
	set := make(map[string]struct{}, len(values))
	for _, value := range values {
		if value != "" {
			set[value] = struct{}{}
		}
	}
	return set
}

func intersects(values []string, set map[string]struct{}) bool {
	for _, value := range values {
		if _, ok := set[value]; ok {
			return true
		}
	}
	return false
}

func contextError(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	return ctx.Err()
}
