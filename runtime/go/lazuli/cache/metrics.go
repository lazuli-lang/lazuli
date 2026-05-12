package cache

import (
	"encoding/json"
	"net/http"
	"reflect"
)

// StatsSnapshot is the JSON-ready point-in-time view of cache counters.
type StatsSnapshot struct {
	// Entries is the current number of cached entries known to the backend.
	Entries uint64 `json:"entries"`
	// Hits is the cumulative number of successful cache lookups.
	Hits uint64 `json:"hits"`
	// Misses is the cumulative number of cache lookups that missed.
	Misses uint64 `json:"misses"`
	// Evicts is the cumulative number of entries evicted by the backend.
	Evicts uint64 `json:"evicts"`
	// HitRatio is Hits divided by total cache lookups. It is zero before
	// any hits or misses have been recorded.
	HitRatio float64 `json:"hit_ratio"`
}

// StatsHandler returns an HTTP handler that writes cache stats as JSON.
func StatsHandler(backend Backend) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		snapshot := StatsSnapshot{}
		if !isNilBackend(backend) {
			stats, err := backend.Stats(r.Context())
			if err != nil {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusServiceUnavailable)
				_ = json.NewEncoder(w).Encode(map[string]string{"error": err.Error()})
				return
			}
			snapshot = statsSnapshot(stats)
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_ = json.NewEncoder(w).Encode(snapshot)
	})
}

func statsSnapshot(stats QueryStats) StatsSnapshot {
	total := float64(stats.Hits) + float64(stats.Misses)
	var hitRatio float64
	if total > 0 {
		hitRatio = float64(stats.Hits) / total
	}

	return StatsSnapshot{
		Entries:  stats.Entries,
		Hits:     stats.Hits,
		Misses:   stats.Misses,
		Evicts:   stats.Evicts,
		HitRatio: hitRatio,
	}
}

func isNilBackend(backend Backend) bool {
	if backend == nil {
		return true
	}

	value := reflect.ValueOf(backend)
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}
