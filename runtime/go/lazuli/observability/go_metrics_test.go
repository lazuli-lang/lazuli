package observability

import (
	"encoding/json"
	"runtime"
	"testing"
)

var goMetricsTestSink []byte

func TestCollectGoMetricsReturnsSaneSnapshot(t *testing.T) {
	goMetricsTestSink = make([]byte, 1024)
	goMetricsTestSink[0] = 1
	t.Cleanup(func() { goMetricsTestSink = nil })

	snapshot := CollectGoMetrics()
	runtime.KeepAlive(goMetricsTestSink)

	if snapshot.Runtime.GoVersion != runtime.Version() {
		t.Fatalf("go_version = %q, want %q", snapshot.Runtime.GoVersion, runtime.Version())
	}
	if snapshot.Runtime.GOOS != runtime.GOOS {
		t.Fatalf("goos = %q, want %q", snapshot.Runtime.GOOS, runtime.GOOS)
	}
	if snapshot.Runtime.GOARCH != runtime.GOARCH {
		t.Fatalf("goarch = %q, want %q", snapshot.Runtime.GOARCH, runtime.GOARCH)
	}
	if snapshot.Runtime.NumCPU == 0 {
		t.Fatal("num_cpu = 0, want a positive runtime CPU count")
	}
	if snapshot.Runtime.MemoryTotalBytes == 0 {
		t.Fatal("memory_total_bytes = 0, want mapped runtime memory")
	}
	if snapshot.GC.HeapAllocatedBytes == 0 {
		t.Fatal("heap_allocated_bytes = 0, want cumulative heap allocation count")
	}
	if snapshot.GC.HeapFreedBytes > snapshot.GC.HeapAllocatedBytes {
		t.Fatalf("heap_freed_bytes = %d, want <= heap_allocated_bytes %d",
			snapshot.GC.HeapFreedBytes, snapshot.GC.HeapAllocatedBytes)
	}
	if snapshot.GC.HeapGoalBytes == 0 {
		t.Fatal("heap_goal_bytes = 0, want a current GC heap goal")
	}
	if snapshot.Scheduler.GOMAXPROCS <= 0 {
		t.Fatalf("gomaxprocs = %d, want a positive scheduler width", snapshot.Scheduler.GOMAXPROCS)
	}
	if snapshot.Scheduler.Goroutines == 0 {
		t.Fatal("goroutines = 0, want a live goroutine count")
	}
}

func TestGoMetricsSnapshotFromMemStatsFallback(t *testing.T) {
	mem := runtime.MemStats{
		Sys:          8192,
		TotalAlloc:   4096,
		HeapAlloc:    1024,
		HeapObjects:  32,
		NextGC:       2048,
		NumGC:        5,
		NumForcedGC:  2,
		PauseTotalNs: 123,
	}

	snapshot := goMetricsSnapshotFromMemStats(mem)

	if snapshot.Runtime.MemoryTotalBytes != mem.Sys {
		t.Fatalf("memory_total_bytes = %d, want %d", snapshot.Runtime.MemoryTotalBytes, mem.Sys)
	}
	if snapshot.GC.HeapAllocatedBytes != mem.TotalAlloc {
		t.Fatalf("heap_allocated_bytes = %d, want %d", snapshot.GC.HeapAllocatedBytes, mem.TotalAlloc)
	}
	if snapshot.GC.HeapFreedBytes != mem.TotalAlloc-mem.HeapAlloc {
		t.Fatalf("heap_freed_bytes = %d, want %d", snapshot.GC.HeapFreedBytes, mem.TotalAlloc-mem.HeapAlloc)
	}
	if snapshot.GC.HeapLiveBytes != mem.HeapAlloc {
		t.Fatalf("heap_live_bytes = %d, want %d", snapshot.GC.HeapLiveBytes, mem.HeapAlloc)
	}
	if snapshot.GC.HeapObjects != mem.HeapObjects {
		t.Fatalf("heap_objects = %d, want %d", snapshot.GC.HeapObjects, mem.HeapObjects)
	}
	if snapshot.GC.HeapGoalBytes != mem.NextGC {
		t.Fatalf("heap_goal_bytes = %d, want %d", snapshot.GC.HeapGoalBytes, mem.NextGC)
	}
	if snapshot.GC.CyclesTotal != uint64(mem.NumGC) {
		t.Fatalf("cycles_total = %d, want %d", snapshot.GC.CyclesTotal, mem.NumGC)
	}
	if snapshot.GC.CyclesAutomatic != uint64(mem.NumGC-mem.NumForcedGC) {
		t.Fatalf("cycles_automatic = %d, want %d", snapshot.GC.CyclesAutomatic, mem.NumGC-mem.NumForcedGC)
	}
	if snapshot.GC.CyclesForced != uint64(mem.NumForcedGC) {
		t.Fatalf("cycles_forced = %d, want %d", snapshot.GC.CyclesForced, mem.NumForcedGC)
	}
	if snapshot.GC.PauseTotalNanoseconds != mem.PauseTotalNs {
		t.Fatalf("pause_total_nanoseconds = %d, want %d", snapshot.GC.PauseTotalNanoseconds, mem.PauseTotalNs)
	}
	if snapshot.Scheduler.GOMAXPROCS <= 0 {
		t.Fatalf("gomaxprocs = %d, want a positive scheduler width", snapshot.Scheduler.GOMAXPROCS)
	}
	if snapshot.Scheduler.Goroutines == 0 {
		t.Fatal("goroutines = 0, want a live goroutine count")
	}
}

func TestGoMetricsSnapshotJSONShape(t *testing.T) {
	raw, err := json.Marshal(CollectGoMetrics())
	if err != nil {
		t.Fatalf("marshal snapshot: %v", err)
	}

	var body struct {
		Runtime   map[string]json.RawMessage `json:"runtime"`
		GC        map[string]json.RawMessage `json:"gc"`
		Scheduler map[string]json.RawMessage `json:"scheduler"`
	}
	if err := json.Unmarshal(raw, &body); err != nil {
		t.Fatalf("decode snapshot shape: %v", err)
	}
	if body.Runtime == nil {
		t.Fatal("runtime object is nil")
	}
	if body.GC == nil {
		t.Fatal("gc object is nil")
	}
	if body.Scheduler == nil {
		t.Fatal("scheduler object is nil")
	}

	assertJSONKeys(t, body.Runtime, []string{
		"go_version",
		"goos",
		"goarch",
		"num_cpu",
		"cgo_calls",
		"memory_total_bytes",
	})
	assertJSONKeys(t, body.GC, []string{
		"heap_allocated_bytes",
		"heap_freed_bytes",
		"heap_live_bytes",
		"heap_objects",
		"heap_goal_bytes",
		"cycles_total",
		"cycles_automatic",
		"cycles_forced",
		"pause_total_nanoseconds",
		"cpu_seconds",
	})
	assertJSONKeys(t, body.Scheduler, []string{
		"gomaxprocs",
		"goroutines",
		"goroutines_created",
		"goroutines_not_in_go",
		"goroutines_runnable",
		"goroutines_running",
		"goroutines_waiting",
		"runtime_owned_threads",
	})
}

func assertJSONKeys(t *testing.T, got map[string]json.RawMessage, expected []string) {
	t.Helper()

	if len(got) != len(expected) {
		t.Fatalf("keys = %v, want exactly %v", got, expected)
	}
	for _, key := range expected {
		if _, ok := got[key]; !ok {
			t.Fatalf("missing key %q in %v", key, got)
		}
	}
}
