package testkit_test

import (
	"context"
	"errors"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/cache"
	"lazuli.dev/runtime/lazuli/testkit"
)

func TestClockCanBeAdvancedAndUsedAsNowFunc(t *testing.T) {
	start := time.Date(2026, time.May, 12, 10, 30, 0, 0, time.UTC)
	clock := testkit.NewClock(start)

	var now func() time.Time = clock.Now
	if got := now(); !got.Equal(start) {
		t.Fatalf("Now() = %s, want %s", got, start)
	}

	advanced := clock.Advance(5 * time.Minute)
	if want := start.Add(5 * time.Minute); !advanced.Equal(want) {
		t.Fatalf("Advance() = %s, want %s", advanced, want)
	}

	reset := start.Add(time.Hour)
	clock.Set(reset)
	if got := clock.Now(); !got.Equal(reset) {
		t.Fatalf("Now() after Set = %s, want %s", got, reset)
	}
}

func TestRoundTripperRecordsRequestsAndReturnsSyntheticResponse(t *testing.T) {
	transport := &testkit.RoundTripper{
		StatusCode: http.StatusCreated,
		Header:     http.Header{"X-Test": []string{"yes"}},
		Body:       []byte("created"),
	}
	client := transport.Client()

	req, err := http.NewRequest(http.MethodPost, "https://example.test/widgets", strings.NewReader("payload"))
	if err != nil {
		t.Fatalf("NewRequest() error = %v", err)
	}
	req.Header.Set("X-Input", "original")

	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("Do() error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusCreated {
		t.Fatalf("StatusCode = %d, want %d", resp.StatusCode, http.StatusCreated)
	}
	if got := resp.Header.Get("X-Test"); got != "yes" {
		t.Fatalf("X-Test = %q, want yes", got)
	}
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("ReadAll(response) error = %v", err)
	}
	if string(body) != "created" {
		t.Fatalf("response body = %q, want created", body)
	}

	requests := transport.Requests()
	if len(requests) != 1 {
		t.Fatalf("Requests() length = %d, want 1", len(requests))
	}
	if requests[0].Method != http.MethodPost {
		t.Fatalf("recorded method = %s, want POST", requests[0].Method)
	}
	recordedBody, err := io.ReadAll(requests[0].Body)
	if err != nil {
		t.Fatalf("ReadAll(recorded request) error = %v", err)
	}
	if string(recordedBody) != "payload" {
		t.Fatalf("recorded request body = %q, want payload", recordedBody)
	}

	requests[0].Header.Set("X-Input", "mutated")
	again := transport.Requests()
	if got := again[0].Header.Get("X-Input"); got != "original" {
		t.Fatalf("recorded request header mutated to %q, want original", got)
	}
}

func TestRoundTripperHandlerReceivesReplayableBody(t *testing.T) {
	var sawBody string
	transport := &testkit.RoundTripper{
		RoundTripFunc: func(req *http.Request) (*http.Response, error) {
			body, err := io.ReadAll(req.Body)
			if err != nil {
				return nil, err
			}
			sawBody = string(body)
			return &http.Response{
				StatusCode: http.StatusAccepted,
				Body:       io.NopCloser(strings.NewReader("handled")),
				Header:     make(http.Header),
				Request:    req,
			}, nil
		},
	}

	req, err := http.NewRequest(http.MethodPut, "https://example.test/widgets/1", strings.NewReader("payload"))
	if err != nil {
		t.Fatalf("NewRequest() error = %v", err)
	}
	resp, err := transport.Client().Do(req)
	if err != nil {
		t.Fatalf("Do() error = %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("StatusCode = %d, want %d", resp.StatusCode, http.StatusAccepted)
	}
	if sawBody != "payload" {
		t.Fatalf("handler body = %q, want payload", sawBody)
	}
}

func TestEventRecorderRecordsCopiesAndReturnsError(t *testing.T) {
	wantErr := errors.New("subscriber failed")
	recorder := &testkit.EventRecorder{Err: wantErr}
	ctx := context.Background()
	tenant := &lazuli.Tenant{OrgID: 7}
	userID := lazuli.ID(42)
	event := lazuli.Event{
		Name:    "customer.created",
		Tenant:  tenant,
		UserID:  &userID,
		Payload: map[string]any{"id": "customer-1"},
	}

	if err := recorder.Subscriber(ctx, event); !errors.Is(err, wantErr) {
		t.Fatalf("Subscriber() error = %v, want %v", err, wantErr)
	}

	tenant.OrgID = 9
	userID = 100
	event.Payload["id"] = "mutated"

	records := recorder.Records()
	if len(records) != 1 {
		t.Fatalf("Records() length = %d, want 1", len(records))
	}
	if records[0].Context != ctx {
		t.Fatal("recorded context does not match")
	}
	if records[0].Event.Tenant.OrgID != 7 {
		t.Fatalf("recorded tenant = %d, want 7", records[0].Event.Tenant.OrgID)
	}
	if *records[0].Event.UserID != 42 {
		t.Fatalf("recorded user id = %d, want 42", *records[0].Event.UserID)
	}
	if got := records[0].Event.Payload["id"]; got != "customer-1" {
		t.Fatalf("recorded payload id = %v, want customer-1", got)
	}

	last, ok := recorder.Last()
	if !ok || last.Name != "customer.created" {
		t.Fatalf("Last() = %q, %v; want customer.created, true", last.Name, ok)
	}

	recorder.Reset()
	if events := recorder.Events(); len(events) != 0 {
		t.Fatalf("Events() after Reset length = %d, want 0", len(events))
	}
}

func TestByteCacheBackendStoresCopiesRecordsCallsAndInvalidates(t *testing.T) {
	ctx := context.Background()
	backend := &testkit.ByteCacheBackend{}
	backend.SeedString("customer.query.list|tenant|a", "cached", "customers")

	value, hit, err := backend.Get(ctx, "customer.query.list|tenant|a")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	if !hit || string(value) != "cached" {
		t.Fatalf("Get() = %q, %v; want cached, true", value, hit)
	}
	value[0] = 'X'
	again, hit, err := backend.Get(ctx, "customer.query.list|tenant|a")
	if err != nil {
		t.Fatalf("Get() second error = %v", err)
	}
	if !hit || string(again) != "cached" {
		t.Fatalf("Get() second = %q, %v; want cached, true", again, hit)
	}

	putValue := []byte("fresh")
	if err := backend.Put(ctx, "customer.query.detail|tenant|b", putValue, 5*time.Minute, []string{"customers", "detail"}); err != nil {
		t.Fatalf("Put() error = %v", err)
	}
	putValue[0] = 'X'

	puts := backend.PutCalls()
	if len(puts) != 1 {
		t.Fatalf("PutCalls() length = %d, want 1", len(puts))
	}
	if puts[0].Key != "customer.query.detail|tenant|b" {
		t.Fatalf("PutCalls()[0].Key = %q", puts[0].Key)
	}
	if string(puts[0].Value) != "fresh" {
		t.Fatalf("PutCalls()[0].Value = %q, want fresh", puts[0].Value)
	}
	puts[0].Value[0] = 'Y'

	stored, ok := backend.Value("customer.query.detail|tenant|b")
	if !ok || string(stored) != "fresh" {
		t.Fatalf("Value() = %q, %v; want fresh, true", stored, ok)
	}

	deleted, err := backend.InvalidateTags(ctx, []string{"detail"})
	if err != nil {
		t.Fatalf("InvalidateTags() error = %v", err)
	}
	if deleted != 1 {
		t.Fatalf("InvalidateTags() deleted = %d, want 1", deleted)
	}
	if _, ok := backend.Value("customer.query.detail|tenant|b"); ok {
		t.Fatal("detail entry was not invalidated")
	}

	deleted, err = backend.InvalidateQueries(ctx, []string{"customer.query.list"})
	if err != nil {
		t.Fatalf("InvalidateQueries() error = %v", err)
	}
	if deleted != 1 {
		t.Fatalf("InvalidateQueries() deleted = %d, want 1", deleted)
	}

	if got := backend.GetCalls(); strings.Join(got, ",") != "customer.query.list|tenant|a,customer.query.list|tenant|a" {
		t.Fatalf("GetCalls() = %#v", got)
	}
	if got := backend.InvalidateTagsCalls(); len(got) != 1 || strings.Join(got[0], ",") != "detail" {
		t.Fatalf("InvalidateTagsCalls() = %#v", got)
	}
	if got := backend.InvalidateQueriesCalls(); len(got) != 1 || strings.Join(got[0], ",") != "customer.query.list" {
		t.Fatalf("InvalidateQueriesCalls() = %#v", got)
	}

	stats, err := backend.Stats(ctx)
	if err != nil {
		t.Fatalf("Stats() error = %v", err)
	}
	wantStats := cache.QueryStats{Entries: 0, Hits: 2, Evicts: 2}
	if stats != wantStats {
		t.Fatalf("Stats() = %+v, want %+v", stats, wantStats)
	}
}

func TestByteCacheBackendErrorsAndStatsOverride(t *testing.T) {
	wantStats := cache.QueryStats{Entries: 3, Hits: 2}
	wantStatsErr := errors.New("stats unavailable")
	backend := &testkit.ByteCacheBackend{
		StatsOverride: &wantStats,
		StatsErr:      wantStatsErr,
	}

	stats, err := backend.Stats(context.Background())
	if !errors.Is(err, wantStatsErr) {
		t.Fatalf("Stats() error = %v, want %v", err, wantStatsErr)
	}
	if stats != wantStats {
		t.Fatalf("Stats() = %+v, want %+v", stats, wantStats)
	}

	wantPutErr := errors.New("put failed")
	backend.PutErr = wantPutErr
	err = backend.Put(context.Background(), "customer.query.list|tenant|a", []byte("payload"), time.Minute, []string{"customers"})
	if !errors.Is(err, wantPutErr) {
		t.Fatalf("Put() error = %v, want %v", err, wantPutErr)
	}
	if puts := backend.PutCalls(); len(puts) != 1 {
		t.Fatalf("PutCalls() length = %d, want 1", len(puts))
	}
	if _, ok := backend.Value("customer.query.list|tenant|a"); ok {
		t.Fatal("failed Put stored a value")
	}
}
