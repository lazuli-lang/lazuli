package jobs

import (
	"sync"
	"time"
)

// JobMetricsSnapshot is the point-in-time counters for one job kind.
type JobMetricsSnapshot struct {
	// Started is the cumulative number of executions that began.
	Started uint64 `json:"started"`
	// Finished is the cumulative number of executions that completed.
	Finished uint64 `json:"finished"`
	// Failures is the cumulative number of completed executions that failed.
	Failures uint64 `json:"failures"`
	// Retries is the cumulative number of retry attempts scheduled.
	Retries uint64 `json:"retries"`
	// Running is the current number of in-flight executions.
	Running uint64 `json:"running"`
	// DurationTotal is the cumulative runtime of completed executions.
	DurationTotal time.Duration `json:"duration_total"`
}

// MetricsCollector records in-memory job execution metrics grouped by job kind.
//
// The zero value is ready to use.
type MetricsCollector struct {
	mu    sync.Mutex
	kinds map[string]JobMetricsSnapshot
}

// RecordStarted records that an execution for kind has started.
func (c *MetricsCollector) RecordStarted(kind string) {
	c.mu.Lock()
	defer c.mu.Unlock()

	metrics := c.metricsFor(kind)
	metrics.Started++
	metrics.Running++
	c.kinds[kind] = metrics
}

// RecordFinished records that an execution for kind completed.
//
// Negative durations are ignored. A non-nil err records the execution as a
// failure.
func (c *MetricsCollector) RecordFinished(kind string, duration time.Duration, err error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	metrics := c.metricsFor(kind)
	metrics.Finished++
	if metrics.Running > 0 {
		metrics.Running--
	}
	if duration > 0 {
		metrics.DurationTotal += duration
	}
	if err != nil {
		metrics.Failures++
	}
	c.kinds[kind] = metrics
}

// RecordRetry records that a retry attempt was scheduled for kind.
func (c *MetricsCollector) RecordRetry(kind string) {
	c.mu.Lock()
	defer c.mu.Unlock()

	metrics := c.metricsFor(kind)
	metrics.Retries++
	c.kinds[kind] = metrics
}

// Snapshot returns a copy of the metrics grouped by job kind.
func (c *MetricsCollector) Snapshot() map[string]JobMetricsSnapshot {
	c.mu.Lock()
	defer c.mu.Unlock()

	snapshot := make(map[string]JobMetricsSnapshot, len(c.kinds))
	for kind, metrics := range c.kinds {
		snapshot[kind] = metrics
	}
	return snapshot
}

func (c *MetricsCollector) metricsFor(kind string) JobMetricsSnapshot {
	if c.kinds == nil {
		c.kinds = make(map[string]JobMetricsSnapshot)
	}
	return c.kinds[kind]
}
