package observability

import (
	"encoding/json"
	"net/http"
	"runtime/metrics"
)

const (
	metricGoroutines         = "/sched/goroutines:goroutines"
	metricHeapAllocatedBytes = "/gc/heap/allocs:bytes"
	metricHeapFreedBytes     = "/gc/heap/frees:bytes"
	metricHeapLiveBytes      = "/gc/heap/live:bytes"
	metricHeapObjects        = "/gc/heap/objects:objects"
	metricMemoryTotalBytes   = "/memory/classes/total:bytes"
	metricGCAutomaticCycles  = "/gc/cycles/automatic:gc-cycles"
	metricGCForcedCycles     = "/gc/cycles/forced:gc-cycles"
	metricGCTotalCPUSeconds  = "/cpu/classes/gc/total:cpu-seconds"
)

// MetricsSnapshot is a compact point-in-time view of Go runtime
// counters and gauges useful for health dashboards and lightweight
// diagnostics.
type MetricsSnapshot struct {
	// Goroutines is the current number of live goroutines.
	Goroutines uint64 `json:"goroutines"`
	// HeapAllocatedBytes is the cumulative number of bytes allocated
	// on the Go heap.
	HeapAllocatedBytes uint64 `json:"heap_allocated_bytes"`
	// HeapFreedBytes is the cumulative number of heap bytes freed.
	HeapFreedBytes uint64 `json:"heap_freed_bytes"`
	// HeapLiveBytes is the bytes marked live by the most recent GC.
	HeapLiveBytes uint64 `json:"heap_live_bytes"`
	// HeapObjects is the current number of live heap objects.
	HeapObjects uint64 `json:"heap_objects"`
	// MemoryTotalBytes is all memory mapped by the Go runtime.
	MemoryTotalBytes uint64 `json:"memory_total_bytes"`
	// GCAutomaticCycles is the cumulative count of automatically
	// triggered completed GC cycles.
	GCAutomaticCycles uint64 `json:"gc_automatic_cycles"`
	// GCForcedCycles is the cumulative count of forced completed GC
	// cycles.
	GCForcedCycles uint64 `json:"gc_forced_cycles"`
	// GCTotalCPUSeconds is the cumulative CPU seconds spent in GC.
	GCTotalCPUSeconds float64 `json:"gc_total_cpu_seconds"`
}

// CollectRuntimeMetrics reads a compact snapshot from runtime/metrics.
func CollectRuntimeMetrics() MetricsSnapshot {
	samples := []metrics.Sample{
		{Name: metricGoroutines},
		{Name: metricHeapAllocatedBytes},
		{Name: metricHeapFreedBytes},
		{Name: metricHeapLiveBytes},
		{Name: metricHeapObjects},
		{Name: metricMemoryTotalBytes},
		{Name: metricGCAutomaticCycles},
		{Name: metricGCForcedCycles},
		{Name: metricGCTotalCPUSeconds},
	}
	metrics.Read(samples)

	return MetricsSnapshot{
		Goroutines:         uint64Metric(samples[0].Value),
		HeapAllocatedBytes: uint64Metric(samples[1].Value),
		HeapFreedBytes:     uint64Metric(samples[2].Value),
		HeapLiveBytes:      uint64Metric(samples[3].Value),
		HeapObjects:        uint64Metric(samples[4].Value),
		MemoryTotalBytes:   uint64Metric(samples[5].Value),
		GCAutomaticCycles:  uint64Metric(samples[6].Value),
		GCForcedCycles:     uint64Metric(samples[7].Value),
		GCTotalCPUSeconds:  float64Metric(samples[8].Value),
	}
}

// RuntimeMetricsHandler returns an http.Handler that writes a JSON
// MetricsSnapshot with HTTP 200.
func RuntimeMetricsHandler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_ = json.NewEncoder(w).Encode(CollectRuntimeMetrics())
	})
}

func uint64Metric(value metrics.Value) uint64 {
	if value.Kind() != metrics.KindUint64 {
		return 0
	}
	return value.Uint64()
}

func float64Metric(value metrics.Value) float64 {
	if value.Kind() != metrics.KindFloat64 {
		return 0
	}
	return value.Float64()
}
