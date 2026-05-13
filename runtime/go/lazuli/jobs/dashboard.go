package jobs

import (
	"sort"
	"time"
)

const (
	// InlineDashboardQueue is the dashboard queue label used for jobs that run
	// inline instead of through an adapter queue.
	InlineDashboardQueue = "inline"

	defaultWorkerStaleAfter = time.Minute
)

// JobDashboardStatus is the provider-neutral lifecycle status used by job
// dashboard helpers.
type JobDashboardStatus string

const (
	JobDashboardStatusUnknown      JobDashboardStatus = "unknown"
	JobDashboardStatusQueued       JobDashboardStatus = "queued"
	JobDashboardStatusRunning      JobDashboardStatus = "running"
	JobDashboardStatusSucceeded    JobDashboardStatus = "succeeded"
	JobDashboardStatusFailed       JobDashboardStatus = "failed"
	JobDashboardStatusRetrying     JobDashboardStatus = "retrying"
	JobDashboardStatusDeadLettered JobDashboardStatus = "dead_lettered"
	JobDashboardStatusCanceled     JobDashboardStatus = "canceled"
)

// WorkerHealthStatus is the derived dashboard health for a queue worker.
type WorkerHealthStatus string

const (
	WorkerHealthUnknown  WorkerHealthStatus = "unknown"
	WorkerHealthHealthy  WorkerHealthStatus = "healthy"
	WorkerHealthStale    WorkerHealthStatus = "stale"
	WorkerHealthDraining WorkerHealthStatus = "draining"
	WorkerHealthStopped  WorkerHealthStatus = "stopped"
)

// JobDashboardRecord is one queue-adapter or inline job observation consumed by
// the dashboard summary helpers.
type JobDashboardRecord struct {
	ID          string             `json:"id,omitempty"`
	Feature     string             `json:"feature,omitempty"`
	Name        string             `json:"name,omitempty"`
	Queue       string             `json:"queue,omitempty"`
	Status      JobDashboardStatus `json:"status,omitempty"`
	Attempts    uint32             `json:"attempts,omitempty"`
	MaxAttempts uint32             `json:"max_attempts,omitempty"`
	Latency     time.Duration      `json:"latency,omitempty"`
	EnqueuedAt  time.Time          `json:"enqueued_at,omitempty"`
	StartedAt   time.Time          `json:"started_at,omitempty"`
	FinishedAt  time.Time          `json:"finished_at,omitempty"`
	NextRetryAt time.Time          `json:"next_retry_at,omitempty"`
	Error       string             `json:"error,omitempty"`
	WorkerID    string             `json:"worker_id,omitempty"`
}

// WorkerSnapshot is one provider-neutral worker heartbeat consumed by
// SummarizeWorkerHealth.
type WorkerSnapshot struct {
	ID            string    `json:"id,omitempty"`
	Queue         string    `json:"queue,omitempty"`
	Active        bool      `json:"active"`
	Draining      bool      `json:"draining,omitempty"`
	ActiveJobs    uint64    `json:"active_jobs,omitempty"`
	Capacity      uint64    `json:"capacity,omitempty"`
	StartedAt     time.Time `json:"started_at,omitempty"`
	LastHeartbeat time.Time `json:"last_heartbeat,omitempty"`
}

// LatencyBucket describes an inclusive latency bucket. An UpperBound less than
// or equal to zero is treated as the final catch-all bucket.
type LatencyBucket struct {
	Name       string        `json:"name"`
	UpperBound time.Duration `json:"upper_bound"`
}

// QueueSummary is the aggregate dashboard view for one queue.
type QueueSummary struct {
	Queue            string           `json:"queue"`
	Jobs             uint64           `json:"jobs"`
	Statuses         []JobStatusCount `json:"statuses"`
	Retries          uint64           `json:"retries"`
	DeadLettered     uint64           `json:"dead_lettered"`
	AverageLatency   time.Duration    `json:"average_latency"`
	MaxLatency       time.Duration    `json:"max_latency"`
	OldestEnqueuedAt time.Time        `json:"oldest_enqueued_at,omitempty"`
	NewestEnqueuedAt time.Time        `json:"newest_enqueued_at,omitempty"`
}

// JobStatusCount is the number of jobs observed in one lifecycle status.
type JobStatusCount struct {
	Status JobDashboardStatus `json:"status"`
	Jobs   uint64             `json:"jobs"`
}

// LatencyBucketCount is the number of jobs whose queue latency landed in one
// bucket.
type LatencyBucketCount struct {
	Name       string        `json:"name"`
	UpperBound time.Duration `json:"upper_bound"`
	Jobs       uint64        `json:"jobs"`
}

// WorkerHealth is the derived health row for one worker.
type WorkerHealth struct {
	ID            string             `json:"id"`
	Queue         string             `json:"queue"`
	Status        WorkerHealthStatus `json:"status"`
	ActiveJobs    uint64             `json:"active_jobs,omitempty"`
	Capacity      uint64             `json:"capacity,omitempty"`
	StartedAt     time.Time          `json:"started_at,omitempty"`
	LastHeartbeat time.Time          `json:"last_heartbeat,omitempty"`
	StaleFor      time.Duration      `json:"stale_for,omitempty"`
}

// RetrySummary is the aggregate dashboard view for retry activity.
type RetrySummary struct {
	Attempts    uint64    `json:"attempts"`
	Retries     uint64    `json:"retries"`
	Retrying    uint64    `json:"retrying"`
	Retryable   uint64    `json:"retryable"`
	Exhausted   uint64    `json:"exhausted"`
	NextRetryAt time.Time `json:"next_retry_at,omitempty"`
}

// DeadLetterSummary is the aggregate dashboard view for active dead-letter
// entries.
type DeadLetterSummary struct {
	Total          uint64                 `json:"total"`
	Jobs           []DeadLetterJobSummary `json:"jobs"`
	OldestFailedAt time.Time              `json:"oldest_failed_at,omitempty"`
	NewestFailedAt time.Time              `json:"newest_failed_at,omitempty"`
}

// DeadLetterJobSummary is the dead-letter count for one job contract.
type DeadLetterJobSummary struct {
	Feature        string    `json:"feature,omitempty"`
	Name           string    `json:"name,omitempty"`
	DeadLetters    uint64    `json:"dead_letters"`
	Attempts       uint64    `json:"attempts"`
	OldestFailedAt time.Time `json:"oldest_failed_at,omitempty"`
	NewestFailedAt time.Time `json:"newest_failed_at,omitempty"`
}

// DashboardInput contains the provider-neutral observations used to build a
// DashboardSummary.
type DashboardInput struct {
	Jobs             []JobDashboardRecord `json:"jobs,omitempty"`
	Workers          []WorkerSnapshot     `json:"workers,omitempty"`
	DeadLetters      []DeadLetterEntry    `json:"dead_letters,omitempty"`
	LatencyBuckets   []LatencyBucket      `json:"latency_buckets,omitempty"`
	Now              time.Time            `json:"now,omitempty"`
	WorkerStaleAfter time.Duration        `json:"worker_stale_after,omitempty"`
}

// DashboardSummary is the complete provider-neutral job dashboard data model.
type DashboardSummary struct {
	Queues      []QueueSummary       `json:"queues"`
	Statuses    []JobStatusCount     `json:"statuses"`
	Latencies   []LatencyBucketCount `json:"latencies"`
	Workers     []WorkerHealth       `json:"workers"`
	Retries     RetrySummary         `json:"retries"`
	DeadLetters DeadLetterSummary    `json:"dead_letters"`
}

// DefaultJobLatencyBuckets returns the default latency buckets used when a
// caller does not supply explicit buckets.
func DefaultJobLatencyBuckets() []LatencyBucket {
	return []LatencyBucket{
		{Name: "lte_100ms", UpperBound: 100 * time.Millisecond},
		{Name: "lte_1s", UpperBound: time.Second},
		{Name: "lte_5s", UpperBound: 5 * time.Second},
		{Name: "lte_30s", UpperBound: 30 * time.Second},
		{Name: "gt_30s"},
	}
}

// BuildDashboardSummary returns a deterministic job dashboard summary.
func BuildDashboardSummary(input DashboardInput) DashboardSummary {
	return DashboardSummary{
		Queues:      SummarizeQueues(input.Jobs),
		Statuses:    CountJobStatuses(input.Jobs),
		Latencies:   BucketJobLatencies(input.Jobs, input.LatencyBuckets),
		Workers:     SummarizeWorkerHealth(input.Workers, input.Now, input.WorkerStaleAfter),
		Retries:     SummarizeRetries(input.Jobs),
		DeadLetters: SummarizeDeadLetters(input.DeadLetters),
	}
}

// SortedJobDashboardRecords returns a normalized, deterministically sorted copy
// of records.
func SortedJobDashboardRecords(records []JobDashboardRecord) []JobDashboardRecord {
	out := make([]JobDashboardRecord, len(records))
	for i, record := range records {
		out[i] = normalizeDashboardRecord(record)
	}
	sort.SliceStable(out, func(i, j int) bool {
		return compareDashboardRecords(out[i], out[j]) < 0
	})
	return out
}

// SummarizeQueues returns deterministic queue summaries for records.
func SummarizeQueues(records []JobDashboardRecord) []QueueSummary {
	type queueState struct {
		jobs             uint64
		statuses         map[JobDashboardStatus]uint64
		retries          uint64
		deadLettered     uint64
		totalLatency     time.Duration
		maxLatency       time.Duration
		oldestEnqueuedAt time.Time
		newestEnqueuedAt time.Time
	}

	queues := make(map[string]*queueState)
	for _, record := range records {
		record = normalizeDashboardRecord(record)
		state := queues[record.Queue]
		if state == nil {
			state = &queueState{statuses: make(map[JobDashboardStatus]uint64)}
			queues[record.Queue] = state
		}

		latency := dashboardRecordLatency(record)
		state.jobs++
		state.statuses[record.Status]++
		state.retries += dashboardRecordRetries(record)
		state.totalLatency += latency
		if latency > state.maxLatency {
			state.maxLatency = latency
		}
		if record.Status == JobDashboardStatusDeadLettered {
			state.deadLettered++
		}
		if !record.EnqueuedAt.IsZero() {
			if state.oldestEnqueuedAt.IsZero() || record.EnqueuedAt.Before(state.oldestEnqueuedAt) {
				state.oldestEnqueuedAt = record.EnqueuedAt
			}
			if record.EnqueuedAt.After(state.newestEnqueuedAt) {
				state.newestEnqueuedAt = record.EnqueuedAt
			}
		}
	}

	names := make([]string, 0, len(queues))
	for name := range queues {
		names = append(names, name)
	}
	sort.Strings(names)

	summaries := make([]QueueSummary, 0, len(names))
	for _, name := range names {
		state := queues[name]
		summary := QueueSummary{
			Queue:            name,
			Jobs:             state.jobs,
			Statuses:         dashboardStatusCounts(state.statuses),
			Retries:          state.retries,
			DeadLettered:     state.deadLettered,
			MaxLatency:       state.maxLatency,
			OldestEnqueuedAt: state.oldestEnqueuedAt,
			NewestEnqueuedAt: state.newestEnqueuedAt,
		}
		if state.jobs > 0 {
			summary.AverageLatency = time.Duration(int64(state.totalLatency) / int64(state.jobs))
		}
		summaries = append(summaries, summary)
	}
	return summaries
}

// CountJobStatuses returns deterministic job counts by dashboard status.
func CountJobStatuses(records []JobDashboardRecord) []JobStatusCount {
	counts := make(map[JobDashboardStatus]uint64)
	for _, record := range records {
		counts[normalizeDashboardRecord(record).Status]++
	}
	return dashboardStatusCounts(counts)
}

// BucketJobLatencies returns deterministic counts for queue latency buckets.
func BucketJobLatencies(records []JobDashboardRecord, buckets []LatencyBucket) []LatencyBucketCount {
	normalized := normalizeLatencyBuckets(buckets)
	counts := make([]LatencyBucketCount, len(normalized))
	for i, bucket := range normalized {
		counts[i] = LatencyBucketCount{
			Name:       bucket.Name,
			UpperBound: bucket.UpperBound,
		}
	}

	for _, record := range records {
		latency := dashboardRecordLatency(normalizeDashboardRecord(record))
		for i, bucket := range normalized {
			if bucket.UpperBound <= 0 || latency <= bucket.UpperBound {
				counts[i].Jobs++
				break
			}
		}
	}
	return counts
}

// SummarizeWorkerHealth returns deterministic health rows for worker snapshots.
func SummarizeWorkerHealth(workers []WorkerSnapshot, now time.Time, staleAfter time.Duration) []WorkerHealth {
	if staleAfter <= 0 {
		staleAfter = defaultWorkerStaleAfter
	}
	if now.IsZero() {
		now = time.Now()
	}

	health := make([]WorkerHealth, 0, len(workers))
	for _, worker := range workers {
		queue := normalizeDashboardQueue(worker.Queue)
		status := WorkerHealthHealthy
		staleFor := time.Duration(0)
		if !worker.LastHeartbeat.IsZero() && now.After(worker.LastHeartbeat) {
			staleFor = now.Sub(worker.LastHeartbeat)
		}

		switch {
		case worker.ID == "":
			status = WorkerHealthUnknown
		case !worker.Active:
			status = WorkerHealthStopped
		case worker.Draining:
			status = WorkerHealthDraining
		case worker.LastHeartbeat.IsZero():
			status = WorkerHealthStale
		case staleFor > staleAfter:
			status = WorkerHealthStale
		}

		health = append(health, WorkerHealth{
			ID:            worker.ID,
			Queue:         queue,
			Status:        status,
			ActiveJobs:    worker.ActiveJobs,
			Capacity:      worker.Capacity,
			StartedAt:     worker.StartedAt,
			LastHeartbeat: worker.LastHeartbeat,
			StaleFor:      staleFor,
		})
	}

	sort.SliceStable(health, func(i, j int) bool {
		return compareWorkerHealth(health[i], health[j]) < 0
	})
	return health
}

// SummarizeRetries returns aggregate retry counters for records.
func SummarizeRetries(records []JobDashboardRecord) RetrySummary {
	var summary RetrySummary
	for _, record := range records {
		record = normalizeDashboardRecord(record)

		summary.Attempts += uint64(record.Attempts)
		summary.Retries += dashboardRecordRetries(record)
		if record.Status == JobDashboardStatusRetrying {
			summary.Retrying++
		}
		if dashboardRecordRetryable(record) {
			summary.Retryable++
		}
		if dashboardRecordExhausted(record) {
			summary.Exhausted++
		}
		if !record.NextRetryAt.IsZero() && (summary.NextRetryAt.IsZero() || record.NextRetryAt.Before(summary.NextRetryAt)) {
			summary.NextRetryAt = record.NextRetryAt
		}
	}
	return summary
}

// SummarizeDeadLetters returns aggregate dead-letter counters grouped by job.
func SummarizeDeadLetters(entries []DeadLetterEntry) DeadLetterSummary {
	type deadLetterState struct {
		feature        string
		name           string
		deadLetters    uint64
		attempts       uint64
		oldestFailedAt time.Time
		newestFailedAt time.Time
	}

	summary := DeadLetterSummary{Total: uint64(len(entries))}
	jobs := make(map[string]*deadLetterState)
	for _, entry := range entries {
		key := entry.Feature + "\x00" + entry.Name
		state := jobs[key]
		if state == nil {
			state = &deadLetterState{
				feature: entry.Feature,
				name:    entry.Name,
			}
			jobs[key] = state
		}

		state.deadLetters++
		state.attempts += uint64(entry.Attempts)
		if !entry.FailedAt.IsZero() {
			if summary.OldestFailedAt.IsZero() || entry.FailedAt.Before(summary.OldestFailedAt) {
				summary.OldestFailedAt = entry.FailedAt
			}
			if entry.FailedAt.After(summary.NewestFailedAt) {
				summary.NewestFailedAt = entry.FailedAt
			}
			if state.oldestFailedAt.IsZero() || entry.FailedAt.Before(state.oldestFailedAt) {
				state.oldestFailedAt = entry.FailedAt
			}
			if entry.FailedAt.After(state.newestFailedAt) {
				state.newestFailedAt = entry.FailedAt
			}
		}
	}

	summary.Jobs = make([]DeadLetterJobSummary, 0, len(jobs))
	for _, state := range jobs {
		summary.Jobs = append(summary.Jobs, DeadLetterJobSummary{
			Feature:        state.feature,
			Name:           state.name,
			DeadLetters:    state.deadLetters,
			Attempts:       state.attempts,
			OldestFailedAt: state.oldestFailedAt,
			NewestFailedAt: state.newestFailedAt,
		})
	}
	sort.SliceStable(summary.Jobs, func(i, j int) bool {
		return compareDeadLetterJobSummary(summary.Jobs[i], summary.Jobs[j]) < 0
	})
	return summary
}

// JobDashboardStatusFromProgress maps progress states to dashboard statuses.
func JobDashboardStatusFromProgress(state ProgressState) JobDashboardStatus {
	switch state {
	case ProgressStatePending:
		return JobDashboardStatusQueued
	case ProgressStateRunning:
		return JobDashboardStatusRunning
	case ProgressStateSucceeded:
		return JobDashboardStatusSucceeded
	case ProgressStateFailed:
		return JobDashboardStatusFailed
	case ProgressStateCanceled:
		return JobDashboardStatusCanceled
	default:
		return JobDashboardStatusUnknown
	}
}

func normalizeDashboardRecord(record JobDashboardRecord) JobDashboardRecord {
	record.Queue = normalizeDashboardQueue(record.Queue)
	record.Status = normalizeDashboardStatus(record)
	if record.Latency < 0 {
		record.Latency = 0
	}
	return record
}

func normalizeDashboardQueue(queue string) string {
	if queue == "" {
		return InlineDashboardQueue
	}
	return queue
}

func normalizeDashboardStatus(record JobDashboardRecord) JobDashboardStatus {
	switch record.Status {
	case JobDashboardStatusQueued,
		JobDashboardStatusRunning,
		JobDashboardStatusSucceeded,
		JobDashboardStatusFailed,
		JobDashboardStatusRetrying,
		JobDashboardStatusDeadLettered,
		JobDashboardStatusCanceled:
		return record.Status
	}
	if record.Error != "" {
		return JobDashboardStatusFailed
	}
	if !record.FinishedAt.IsZero() {
		return JobDashboardStatusSucceeded
	}
	if !record.StartedAt.IsZero() {
		return JobDashboardStatusRunning
	}
	if !record.EnqueuedAt.IsZero() {
		return JobDashboardStatusQueued
	}
	return JobDashboardStatusUnknown
}

func dashboardRecordLatency(record JobDashboardRecord) time.Duration {
	if record.Latency > 0 {
		return record.Latency
	}
	if !record.EnqueuedAt.IsZero() && !record.StartedAt.IsZero() {
		return nonNegativeDuration(record.StartedAt.Sub(record.EnqueuedAt))
	}
	if !record.EnqueuedAt.IsZero() && !record.FinishedAt.IsZero() {
		return nonNegativeDuration(record.FinishedAt.Sub(record.EnqueuedAt))
	}
	return 0
}

func dashboardRecordRetries(record JobDashboardRecord) uint64 {
	if record.Attempts <= 1 {
		return 0
	}
	return uint64(record.Attempts - 1)
}

func dashboardRecordRetryable(record JobDashboardRecord) bool {
	if record.MaxAttempts == 0 || record.Attempts >= record.MaxAttempts {
		return false
	}
	switch record.Status {
	case JobDashboardStatusFailed, JobDashboardStatusRetrying:
		return true
	default:
		return false
	}
}

func dashboardRecordExhausted(record JobDashboardRecord) bool {
	if record.MaxAttempts == 0 || record.Attempts < record.MaxAttempts {
		return false
	}
	switch record.Status {
	case JobDashboardStatusFailed, JobDashboardStatusDeadLettered:
		return true
	default:
		return false
	}
}

func dashboardStatusCounts(counts map[JobDashboardStatus]uint64) []JobStatusCount {
	statuses := make([]JobStatusCount, 0, len(counts))
	for status, jobs := range counts {
		statuses = append(statuses, JobStatusCount{
			Status: status,
			Jobs:   jobs,
		})
	}
	sort.SliceStable(statuses, func(i, j int) bool {
		return compareDashboardStatus(statuses[i].Status, statuses[j].Status) < 0
	})
	return statuses
}

func normalizeLatencyBuckets(buckets []LatencyBucket) []LatencyBucket {
	if len(buckets) == 0 {
		buckets = DefaultJobLatencyBuckets()
	}

	normalized := make([]LatencyBucket, len(buckets))
	hasCatchAll := false
	for i, bucket := range buckets {
		normalized[i] = bucket
		if normalized[i].Name == "" {
			normalized[i].Name = latencyBucketName(normalized[i].UpperBound)
		}
		if normalized[i].UpperBound <= 0 {
			hasCatchAll = true
		}
	}
	if !hasCatchAll {
		normalized = append(normalized, LatencyBucket{Name: latencyBucketName(0)})
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return compareLatencyBuckets(normalized[i], normalized[j]) < 0
	})
	return normalized
}

func latencyBucketName(upperBound time.Duration) string {
	if upperBound <= 0 {
		return "overflow"
	}
	return "lte_" + upperBound.String()
}

func nonNegativeDuration(duration time.Duration) time.Duration {
	if duration < 0 {
		return 0
	}
	return duration
}

func compareDashboardRecords(left, right JobDashboardRecord) int {
	for _, cmp := range []int{
		compareString(left.Queue, right.Queue),
		compareString(left.Feature, right.Feature),
		compareString(left.Name, right.Name),
		compareDashboardStatus(left.Status, right.Status),
		compareString(left.ID, right.ID),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareWorkerHealth(left, right WorkerHealth) int {
	for _, cmp := range []int{
		compareString(left.Queue, right.Queue),
		compareString(left.ID, right.ID),
		compareWorkerHealthStatus(left.Status, right.Status),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareDeadLetterJobSummary(left, right DeadLetterJobSummary) int {
	for _, cmp := range []int{
		compareString(left.Feature, right.Feature),
		compareString(left.Name, right.Name),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareLatencyBuckets(left, right LatencyBucket) int {
	if left.UpperBound <= 0 && right.UpperBound > 0 {
		return 1
	}
	if left.UpperBound > 0 && right.UpperBound <= 0 {
		return -1
	}
	if left.UpperBound < right.UpperBound {
		return -1
	}
	if left.UpperBound > right.UpperBound {
		return 1
	}
	return compareString(left.Name, right.Name)
}

func compareDashboardStatus(left, right JobDashboardStatus) int {
	return compareStatusOrder(dashboardStatusOrder(left), string(left), dashboardStatusOrder(right), string(right))
}

func compareWorkerHealthStatus(left, right WorkerHealthStatus) int {
	return compareStatusOrder(workerHealthStatusOrder(left), string(left), workerHealthStatusOrder(right), string(right))
}

func compareStatusOrder(leftOrder int, left string, rightOrder int, right string) int {
	if leftOrder < rightOrder {
		return -1
	}
	if leftOrder > rightOrder {
		return 1
	}
	return compareString(left, right)
}

func dashboardStatusOrder(status JobDashboardStatus) int {
	switch status {
	case JobDashboardStatusQueued:
		return 10
	case JobDashboardStatusRunning:
		return 20
	case JobDashboardStatusRetrying:
		return 30
	case JobDashboardStatusSucceeded:
		return 40
	case JobDashboardStatusFailed:
		return 50
	case JobDashboardStatusDeadLettered:
		return 60
	case JobDashboardStatusCanceled:
		return 70
	default:
		return 100
	}
}

func workerHealthStatusOrder(status WorkerHealthStatus) int {
	switch status {
	case WorkerHealthHealthy:
		return 10
	case WorkerHealthDraining:
		return 20
	case WorkerHealthStale:
		return 30
	case WorkerHealthStopped:
		return 40
	default:
		return 100
	}
}

func compareString(left, right string) int {
	if left < right {
		return -1
	}
	if left > right {
		return 1
	}
	return 0
}
