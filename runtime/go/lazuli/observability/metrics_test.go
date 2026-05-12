package observability

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"runtime"
	"sort"
	"testing"
)

var metricsTestSink []byte

func TestCollectRuntimeMetricsReturnsSnapshot(t *testing.T) {
	metricsTestSink = make([]byte, 1024)
	metricsTestSink[0] = 1
	t.Cleanup(func() { metricsTestSink = nil })

	snapshot := CollectRuntimeMetrics()
	runtime.KeepAlive(metricsTestSink)

	if snapshot.Goroutines == 0 {
		t.Fatal("goroutines = 0, want a live runtime count")
	}
	if snapshot.HeapAllocatedBytes == 0 {
		t.Fatal("heap_allocated_bytes = 0, want cumulative heap allocation count")
	}
	if snapshot.MemoryTotalBytes == 0 {
		t.Fatal("memory_total_bytes = 0, want mapped runtime memory count")
	}
	if snapshot.HeapFreedBytes > snapshot.HeapAllocatedBytes {
		t.Fatalf("heap_freed_bytes = %d, want <= heap_allocated_bytes %d",
			snapshot.HeapFreedBytes, snapshot.HeapAllocatedBytes)
	}
}

func TestRuntimeMetricsHandlerWritesJSONSnapshot(t *testing.T) {
	rec := httptest.NewRecorder()
	RuntimeMetricsHandler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/metrics/runtime", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var raw map[string]json.RawMessage
	if err := json.Unmarshal(rec.Body.Bytes(), &raw); err != nil {
		t.Fatalf("decode raw response: %v", err)
	}

	expectedKeys := []string{
		"goroutines",
		"heap_allocated_bytes",
		"heap_freed_bytes",
		"heap_live_bytes",
		"heap_objects",
		"memory_total_bytes",
		"gc_automatic_cycles",
		"gc_forced_cycles",
		"gc_total_cpu_seconds",
	}
	if len(raw) != len(expectedKeys) {
		t.Fatalf("keys = %v, want exactly %v", rawKeys(raw), expectedKeys)
	}
	for _, key := range expectedKeys {
		if _, ok := raw[key]; !ok {
			t.Fatalf("missing response key %q in %v", key, rawKeys(raw))
		}
	}

	var snapshot MetricsSnapshot
	if err := json.Unmarshal(rec.Body.Bytes(), &snapshot); err != nil {
		t.Fatalf("decode snapshot: %v", err)
	}
	if snapshot.Goroutines == 0 {
		t.Fatal("goroutines = 0, want a live runtime count")
	}
}

func rawKeys(raw map[string]json.RawMessage) []string {
	keys := make([]string, 0, len(raw))
	for key := range raw {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
