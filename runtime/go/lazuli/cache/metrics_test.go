package cache

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sort"
	"testing"
	"time"
)

func TestStatsHandlerWritesJSONSnapshot(t *testing.T) {
	backend := &fakeMetricsBackend{
		stats: QueryStats{
			Entries: 12,
			Hits:    3,
			Misses:  1,
			Evicts:  2,
		},
	}

	rec := httptest.NewRecorder()
	StatsHandler(backend).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/metrics/cache", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}
	if backend.calls != 1 {
		t.Fatalf("Stats calls = %d, want 1", backend.calls)
	}

	var raw map[string]json.RawMessage
	if err := json.Unmarshal(rec.Body.Bytes(), &raw); err != nil {
		t.Fatalf("decode raw response: %v", err)
	}
	expectedKeys := []string{"entries", "hits", "misses", "evicts", "hit_ratio"}
	if len(raw) != len(expectedKeys) {
		t.Fatalf("keys = %v, want exactly %v", metricRawKeys(raw), expectedKeys)
	}
	for _, key := range expectedKeys {
		if _, ok := raw[key]; !ok {
			t.Fatalf("missing response key %q in %v", key, metricRawKeys(raw))
		}
	}

	var snapshot StatsSnapshot
	if err := json.Unmarshal(rec.Body.Bytes(), &snapshot); err != nil {
		t.Fatalf("decode snapshot: %v", err)
	}
	if snapshot.Entries != 12 {
		t.Fatalf("Entries = %d, want 12", snapshot.Entries)
	}
	if snapshot.Hits != 3 {
		t.Fatalf("Hits = %d, want 3", snapshot.Hits)
	}
	if snapshot.Misses != 1 {
		t.Fatalf("Misses = %d, want 1", snapshot.Misses)
	}
	if snapshot.Evicts != 2 {
		t.Fatalf("Evicts = %d, want 2", snapshot.Evicts)
	}
	if snapshot.HitRatio != 0.75 {
		t.Fatalf("HitRatio = %v, want 0.75", snapshot.HitRatio)
	}
}

func TestStatsHandlerUsesZeroHitRatioBeforeLookups(t *testing.T) {
	backend := &fakeMetricsBackend{stats: QueryStats{Entries: 4}}

	rec := httptest.NewRecorder()
	StatsHandler(backend).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/metrics/cache", nil))

	var snapshot StatsSnapshot
	if err := json.Unmarshal(rec.Body.Bytes(), &snapshot); err != nil {
		t.Fatalf("decode snapshot: %v", err)
	}
	if snapshot.HitRatio != 0 {
		t.Fatalf("HitRatio = %v, want 0", snapshot.HitRatio)
	}
}

func TestStatsHandlerHandlesNilBackend(t *testing.T) {
	for _, tt := range []struct {
		name    string
		backend Backend
	}{
		{name: "nil interface"},
		{name: "typed nil", backend: (*fakeMetricsBackend)(nil)},
	} {
		t.Run(tt.name, func(t *testing.T) {
			rec := httptest.NewRecorder()
			StatsHandler(tt.backend).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/metrics/cache", nil))

			if rec.Code != http.StatusOK {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
			}
			var snapshot StatsSnapshot
			if err := json.Unmarshal(rec.Body.Bytes(), &snapshot); err != nil {
				t.Fatalf("decode snapshot: %v", err)
			}
			if snapshot != (StatsSnapshot{}) {
				t.Fatalf("snapshot = %+v, want zero value", snapshot)
			}
		})
	}
}

func TestStatsHandlerReportsBackendErrors(t *testing.T) {
	wantErr := errors.New("cache unavailable")
	backend := &fakeMetricsBackend{err: wantErr}

	rec := httptest.NewRecorder()
	StatsHandler(backend).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/metrics/cache", nil))

	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusServiceUnavailable)
	}
	var body map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body["error"] != wantErr.Error() {
		t.Fatalf("error = %q, want %q", body["error"], wantErr.Error())
	}
}

type fakeMetricsBackend struct {
	stats QueryStats
	err   error
	calls int
}

func (b *fakeMetricsBackend) Get(context.Context, string) ([]byte, bool, error) {
	return nil, false, nil
}

func (b *fakeMetricsBackend) Put(context.Context, string, []byte, time.Duration, []string) error {
	return nil
}

func (b *fakeMetricsBackend) InvalidateQueries(context.Context, []string) (int, error) {
	return 0, nil
}

func (b *fakeMetricsBackend) InvalidateTags(context.Context, []string) (int, error) {
	return 0, nil
}

func (b *fakeMetricsBackend) Stats(context.Context) (QueryStats, error) {
	b.calls++
	return b.stats, b.err
}

func metricRawKeys(raw map[string]json.RawMessage) []string {
	keys := make([]string, 0, len(raw))
	for key := range raw {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
