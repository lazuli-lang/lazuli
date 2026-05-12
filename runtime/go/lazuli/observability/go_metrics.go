package observability

import (
	"runtime"
	"runtime/metrics"
)

const (
	goMetricCgoCalls             = "/cgo/go-to-c-calls:calls"
	goMetricMemoryTotalBytes     = "/memory/classes/total:bytes"
	goMetricGCHeapAllocatedBytes = "/gc/heap/allocs:bytes"
	goMetricGCHeapFreedBytes     = "/gc/heap/frees:bytes"
	goMetricGCHeapLiveBytes      = "/gc/heap/live:bytes"
	goMetricGCHeapObjects        = "/gc/heap/objects:objects"
	goMetricGCHeapGoalBytes      = "/gc/heap/goal:bytes"
	goMetricGCCyclesTotal        = "/gc/cycles/total:gc-cycles"
	goMetricGCCyclesAutomatic    = "/gc/cycles/automatic:gc-cycles"
	goMetricGCCyclesForced       = "/gc/cycles/forced:gc-cycles"
	goMetricGCCPUSeconds         = "/cpu/classes/gc/total:cpu-seconds"
	goMetricSchedulerGOMAXPROCS  = "/sched/gomaxprocs:threads"
	goMetricSchedulerGoroutines  = "/sched/goroutines:goroutines"
	goMetricGoroutinesCreated    = "/sched/goroutines-created:goroutines"
	goMetricGoroutinesNotInGo    = "/sched/goroutines/not-in-go:goroutines"
	goMetricGoroutinesRunnable   = "/sched/goroutines/runnable:goroutines"
	goMetricGoroutinesRunning    = "/sched/goroutines/running:goroutines"
	goMetricGoroutinesWaiting    = "/sched/goroutines/waiting:goroutines"
	goMetricSchedulerThreads     = "/sched/threads/total:threads"
)

var goRuntimeMetricTargets = []goRuntimeMetricTarget{
	{name: goMetricCgoCalls, kind: metrics.KindUint64},
	{name: goMetricMemoryTotalBytes, kind: metrics.KindUint64},
	{name: goMetricGCHeapAllocatedBytes, kind: metrics.KindUint64},
	{name: goMetricGCHeapFreedBytes, kind: metrics.KindUint64},
	{name: goMetricGCHeapLiveBytes, kind: metrics.KindUint64},
	{name: goMetricGCHeapObjects, kind: metrics.KindUint64},
	{name: goMetricGCHeapGoalBytes, kind: metrics.KindUint64},
	{name: goMetricGCCyclesTotal, kind: metrics.KindUint64},
	{name: goMetricGCCyclesAutomatic, kind: metrics.KindUint64},
	{name: goMetricGCCyclesForced, kind: metrics.KindUint64},
	{name: goMetricGCCPUSeconds, kind: metrics.KindFloat64},
	{name: goMetricSchedulerGOMAXPROCS, kind: metrics.KindUint64},
	{name: goMetricSchedulerGoroutines, kind: metrics.KindUint64},
	{name: goMetricGoroutinesCreated, kind: metrics.KindUint64},
	{name: goMetricGoroutinesNotInGo, kind: metrics.KindUint64},
	{name: goMetricGoroutinesRunnable, kind: metrics.KindUint64},
	{name: goMetricGoroutinesRunning, kind: metrics.KindUint64},
	{name: goMetricGoroutinesWaiting, kind: metrics.KindUint64},
	{name: goMetricSchedulerThreads, kind: metrics.KindUint64},
}

// GoMetricsSnapshot is a stable point-in-time view of Go runtime, GC, and
// scheduler metrics.
type GoMetricsSnapshot struct {
	Runtime   GoRuntimeMetricsSnapshot   `json:"runtime"`
	GC        GoGCMetricsSnapshot        `json:"gc"`
	Scheduler GoSchedulerMetricsSnapshot `json:"scheduler"`
}

// GoRuntimeMetricsSnapshot describes process-level Go runtime state.
type GoRuntimeMetricsSnapshot struct {
	GoVersion        string `json:"go_version"`
	GOOS             string `json:"goos"`
	GOARCH           string `json:"goarch"`
	NumCPU           int    `json:"num_cpu"`
	CgoCalls         uint64 `json:"cgo_calls"`
	MemoryTotalBytes uint64 `json:"memory_total_bytes"`
}

// GoGCMetricsSnapshot describes Go heap and garbage collector state.
type GoGCMetricsSnapshot struct {
	HeapAllocatedBytes    uint64  `json:"heap_allocated_bytes"`
	HeapFreedBytes        uint64  `json:"heap_freed_bytes"`
	HeapLiveBytes         uint64  `json:"heap_live_bytes"`
	HeapObjects           uint64  `json:"heap_objects"`
	HeapGoalBytes         uint64  `json:"heap_goal_bytes"`
	CyclesTotal           uint64  `json:"cycles_total"`
	CyclesAutomatic       uint64  `json:"cycles_automatic"`
	CyclesForced          uint64  `json:"cycles_forced"`
	PauseTotalNanoseconds uint64  `json:"pause_total_nanoseconds"`
	CPUSeconds            float64 `json:"cpu_seconds"`
}

// GoSchedulerMetricsSnapshot describes Go scheduler state.
type GoSchedulerMetricsSnapshot struct {
	GOMAXPROCS          int    `json:"gomaxprocs"`
	Goroutines          uint64 `json:"goroutines"`
	GoroutinesCreated   uint64 `json:"goroutines_created"`
	GoroutinesNotInGo   uint64 `json:"goroutines_not_in_go"`
	GoroutinesRunnable  uint64 `json:"goroutines_runnable"`
	GoroutinesRunning   uint64 `json:"goroutines_running"`
	GoroutinesWaiting   uint64 `json:"goroutines_waiting"`
	RuntimeOwnedThreads uint64 `json:"runtime_owned_threads"`
}

// CollectGoMetrics returns a Go runtime metrics snapshot. The snapshot is
// first populated from runtime.MemStats and runtime package helpers, then
// overwritten with matching runtime/metrics values when the current toolchain
// exposes them.
func CollectGoMetrics() GoMetricsSnapshot {
	var mem runtime.MemStats
	runtime.ReadMemStats(&mem)

	snapshot := goMetricsSnapshotFromMemStats(mem)
	applyGoRuntimeMetrics(&snapshot, readAvailableGoRuntimeMetrics())
	return snapshot
}

type goRuntimeMetricTarget struct {
	name string
	kind metrics.ValueKind
}

func goMetricsSnapshotFromMemStats(mem runtime.MemStats) GoMetricsSnapshot {
	heapFreedBytes := uint64(0)
	if mem.TotalAlloc >= mem.HeapAlloc {
		heapFreedBytes = mem.TotalAlloc - mem.HeapAlloc
	}

	cgoCalls := uint64(0)
	if calls := runtime.NumCgoCall(); calls > 0 {
		cgoCalls = uint64(calls)
	}

	gcCyclesForced := uint64(mem.NumForcedGC)
	gcCyclesAutomatic := uint64(0)
	if mem.NumGC >= mem.NumForcedGC {
		gcCyclesAutomatic = uint64(mem.NumGC - mem.NumForcedGC)
	}

	return GoMetricsSnapshot{
		Runtime: GoRuntimeMetricsSnapshot{
			GoVersion:        runtime.Version(),
			GOOS:             runtime.GOOS,
			GOARCH:           runtime.GOARCH,
			NumCPU:           runtime.NumCPU(),
			CgoCalls:         cgoCalls,
			MemoryTotalBytes: mem.Sys,
		},
		GC: GoGCMetricsSnapshot{
			HeapAllocatedBytes:    mem.TotalAlloc,
			HeapFreedBytes:        heapFreedBytes,
			HeapLiveBytes:         mem.HeapAlloc,
			HeapObjects:           mem.HeapObjects,
			HeapGoalBytes:         mem.NextGC,
			CyclesTotal:           uint64(mem.NumGC),
			CyclesAutomatic:       gcCyclesAutomatic,
			CyclesForced:          gcCyclesForced,
			PauseTotalNanoseconds: mem.PauseTotalNs,
		},
		Scheduler: GoSchedulerMetricsSnapshot{
			GOMAXPROCS: runtime.GOMAXPROCS(0),
			Goroutines: uint64(runtime.NumGoroutine()),
		},
	}
}

func readAvailableGoRuntimeMetrics() map[string]metrics.Value {
	available := make(map[string]metrics.ValueKind)
	for _, description := range metrics.All() {
		available[description.Name] = description.Kind
	}

	samples := make([]metrics.Sample, 0, len(goRuntimeMetricTargets))
	for _, target := range goRuntimeMetricTargets {
		if available[target.name] == target.kind {
			samples = append(samples, metrics.Sample{Name: target.name})
		}
	}
	if len(samples) == 0 {
		return nil
	}

	metrics.Read(samples)
	values := make(map[string]metrics.Value, len(samples))
	for _, sample := range samples {
		values[sample.Name] = sample.Value
	}
	return values
}

func applyGoRuntimeMetrics(snapshot *GoMetricsSnapshot, values map[string]metrics.Value) {
	if snapshot == nil || len(values) == 0 {
		return
	}

	setUint64 := func(name string, assign func(uint64)) {
		value, ok := values[name]
		if !ok || value.Kind() != metrics.KindUint64 {
			return
		}
		assign(value.Uint64())
	}
	setFloat64 := func(name string, assign func(float64)) {
		value, ok := values[name]
		if !ok || value.Kind() != metrics.KindFloat64 {
			return
		}
		assign(value.Float64())
	}

	setUint64(goMetricCgoCalls, func(value uint64) { snapshot.Runtime.CgoCalls = value })
	setUint64(goMetricMemoryTotalBytes, func(value uint64) { snapshot.Runtime.MemoryTotalBytes = value })
	setUint64(goMetricGCHeapAllocatedBytes, func(value uint64) { snapshot.GC.HeapAllocatedBytes = value })
	setUint64(goMetricGCHeapFreedBytes, func(value uint64) { snapshot.GC.HeapFreedBytes = value })
	setUint64(goMetricGCHeapLiveBytes, func(value uint64) { snapshot.GC.HeapLiveBytes = value })
	setUint64(goMetricGCHeapObjects, func(value uint64) { snapshot.GC.HeapObjects = value })
	setUint64(goMetricGCHeapGoalBytes, func(value uint64) { snapshot.GC.HeapGoalBytes = value })
	setUint64(goMetricGCCyclesTotal, func(value uint64) { snapshot.GC.CyclesTotal = value })
	setUint64(goMetricGCCyclesAutomatic, func(value uint64) { snapshot.GC.CyclesAutomatic = value })
	setUint64(goMetricGCCyclesForced, func(value uint64) { snapshot.GC.CyclesForced = value })
	setFloat64(goMetricGCCPUSeconds, func(value float64) { snapshot.GC.CPUSeconds = value })
	setUint64(goMetricSchedulerGOMAXPROCS, func(value uint64) { snapshot.Scheduler.GOMAXPROCS = int(value) })
	setUint64(goMetricSchedulerGoroutines, func(value uint64) { snapshot.Scheduler.Goroutines = value })
	setUint64(goMetricGoroutinesCreated, func(value uint64) { snapshot.Scheduler.GoroutinesCreated = value })
	setUint64(goMetricGoroutinesNotInGo, func(value uint64) { snapshot.Scheduler.GoroutinesNotInGo = value })
	setUint64(goMetricGoroutinesRunnable, func(value uint64) { snapshot.Scheduler.GoroutinesRunnable = value })
	setUint64(goMetricGoroutinesRunning, func(value uint64) { snapshot.Scheduler.GoroutinesRunning = value })
	setUint64(goMetricGoroutinesWaiting, func(value uint64) { snapshot.Scheduler.GoroutinesWaiting = value })
	setUint64(goMetricSchedulerThreads, func(value uint64) { snapshot.Scheduler.RuntimeOwnedThreads = value })
}
