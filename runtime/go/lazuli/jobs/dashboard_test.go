package jobs

import (
	"reflect"
	"testing"
	"time"
)

func TestBuildDashboardSummaryAggregatesAndSorts(t *testing.T) {
	t.Parallel()

	base := time.Date(2026, 5, 12, 18, 0, 0, 0, time.UTC)
	summary := BuildDashboardSummary(DashboardInput{
		Jobs: []JobDashboardRecord{
			{
				ID:          "job-3",
				Feature:     "billing",
				Name:        "settle",
				Queue:       "critical",
				Status:      JobDashboardStatusRetrying,
				Attempts:    2,
				MaxAttempts: 4,
				EnqueuedAt:  base.Add(-4 * time.Second),
				StartedAt:   base.Add(-2 * time.Second),
				NextRetryAt: base.Add(10 * time.Second),
			},
			{
				ID:         "job-1",
				Feature:    "email",
				Name:       "send",
				Status:     JobDashboardStatusSucceeded,
				Attempts:   1,
				Latency:    50 * time.Millisecond,
				EnqueuedAt: base.Add(-time.Minute),
			},
			{
				ID:          "job-2",
				Feature:     "billing",
				Name:        "settle",
				Queue:       "critical",
				Status:      JobDashboardStatusDeadLettered,
				Attempts:    3,
				MaxAttempts: 3,
				Latency:     1500 * time.Millisecond,
				EnqueuedAt:  base.Add(-30 * time.Second),
			},
		},
		Workers: []WorkerSnapshot{
			{ID: "worker-stale", Queue: "critical", Active: true, LastHeartbeat: base.Add(-2 * time.Minute)},
			{ID: "worker-healthy", Active: true, LastHeartbeat: base.Add(-10 * time.Second), ActiveJobs: 1, Capacity: 5},
		},
		DeadLetters: []DeadLetterEntry{
			{Feature: "billing", Name: "settle", Attempts: 3, FailedAt: base.Add(-time.Minute)},
		},
		LatencyBuckets: []LatencyBucket{
			{Name: "slow"},
			{Name: "fast", UpperBound: time.Second},
		},
		Now:              base,
		WorkerStaleAfter: time.Minute,
	})

	if got, want := queueNames(summary.Queues), []string{"critical", "inline"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("queue names = %#v, want %#v", got, want)
	}
	critical := summary.Queues[0]
	if critical.Jobs != 2 || critical.Retries != 3 || critical.DeadLettered != 1 {
		t.Fatalf("critical summary = %+v, want jobs=2 retries=3 dead_lettered=1", critical)
	}
	if critical.AverageLatency != 1750*time.Millisecond || critical.MaxLatency != 2*time.Second {
		t.Fatalf("critical latency = avg %v max %v, want 1.75s/2s", critical.AverageLatency, critical.MaxLatency)
	}
	if !critical.OldestEnqueuedAt.Equal(base.Add(-30 * time.Second)) {
		t.Fatalf("critical oldest = %v, want %v", critical.OldestEnqueuedAt, base.Add(-30*time.Second))
	}

	wantStatuses := []JobStatusCount{
		{Status: JobDashboardStatusRetrying, Jobs: 1},
		{Status: JobDashboardStatusSucceeded, Jobs: 1},
		{Status: JobDashboardStatusDeadLettered, Jobs: 1},
	}
	if !reflect.DeepEqual(summary.Statuses, wantStatuses) {
		t.Fatalf("statuses = %#v, want %#v", summary.Statuses, wantStatuses)
	}

	wantLatencies := []LatencyBucketCount{
		{Name: "fast", UpperBound: time.Second, Jobs: 1},
		{Name: "slow", Jobs: 2},
	}
	if !reflect.DeepEqual(summary.Latencies, wantLatencies) {
		t.Fatalf("latencies = %#v, want %#v", summary.Latencies, wantLatencies)
	}

	if got, want := workerIDs(summary.Workers), []string{"worker-stale", "worker-healthy"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("workers = %#v, want %#v", got, want)
	}
	if summary.Workers[0].Status != WorkerHealthStale || summary.Workers[1].Status != WorkerHealthHealthy {
		t.Fatalf("worker statuses = %#v, want stale/healthy", summary.Workers)
	}

	if summary.Retries.Attempts != 6 || summary.Retries.Retries != 3 || summary.Retries.Retrying != 1 ||
		summary.Retries.Retryable != 1 || summary.Retries.Exhausted != 1 {
		t.Fatalf("retry summary = %+v, want attempts=6 retries=3 retrying=1 retryable=1 exhausted=1", summary.Retries)
	}
	if !summary.Retries.NextRetryAt.Equal(base.Add(10 * time.Second)) {
		t.Fatalf("NextRetryAt = %v, want %v", summary.Retries.NextRetryAt, base.Add(10*time.Second))
	}

	if summary.DeadLetters.Total != 1 || len(summary.DeadLetters.Jobs) != 1 {
		t.Fatalf("dead letters = %+v, want one grouped entry", summary.DeadLetters)
	}
	if summary.DeadLetters.Jobs[0].Feature != "billing" || summary.DeadLetters.Jobs[0].Attempts != 3 {
		t.Fatalf("dead letter job = %+v, want billing attempts=3", summary.DeadLetters.Jobs[0])
	}
}

func TestSortedJobDashboardRecordsNormalizesWithoutMutating(t *testing.T) {
	t.Parallel()

	base := time.Date(2026, 5, 12, 19, 0, 0, 0, time.UTC)
	records := []JobDashboardRecord{
		{ID: "b", Feature: "z", Name: "job", Queue: "slow", EnqueuedAt: base},
		{ID: "a", Feature: "a", Name: "job", Latency: -time.Second, Error: "boom"},
	}

	sorted := SortedJobDashboardRecords(records)
	if records[1].Queue != "" || records[1].Latency != -time.Second {
		t.Fatalf("input records were mutated: %#v", records)
	}
	if sorted[0].ID != "a" || sorted[0].Queue != InlineDashboardQueue {
		t.Fatalf("first sorted record = %+v, want normalized inline record a", sorted[0])
	}
	if sorted[0].Status != JobDashboardStatusFailed || sorted[0].Latency != 0 {
		t.Fatalf("normalized first record = %+v, want failed with non-negative latency", sorted[0])
	}
	if sorted[1].Status != JobDashboardStatusQueued {
		t.Fatalf("second status = %q, want queued", sorted[1].Status)
	}
}

func TestSummarizeDeadLettersGroupsAndSorts(t *testing.T) {
	t.Parallel()

	base := time.Date(2026, 5, 12, 20, 0, 0, 0, time.UTC)
	summary := SummarizeDeadLetters([]DeadLetterEntry{
		{Feature: "orders", Name: "close", Attempts: 4, FailedAt: base.Add(-time.Hour)},
		{Feature: "billing", Name: "charge", Attempts: 3, FailedAt: base},
		{Feature: "billing", Name: "charge", Attempts: 2, FailedAt: base.Add(-2 * time.Hour)},
	})

	if summary.Total != 3 {
		t.Fatalf("Total = %d, want 3", summary.Total)
	}
	if !summary.OldestFailedAt.Equal(base.Add(-2*time.Hour)) || !summary.NewestFailedAt.Equal(base) {
		t.Fatalf("failed range = %v..%v, want %v..%v",
			summary.OldestFailedAt, summary.NewestFailedAt, base.Add(-2*time.Hour), base)
	}

	want := []DeadLetterJobSummary{
		{
			Feature:        "billing",
			Name:           "charge",
			DeadLetters:    2,
			Attempts:       5,
			OldestFailedAt: base.Add(-2 * time.Hour),
			NewestFailedAt: base,
		},
		{
			Feature:        "orders",
			Name:           "close",
			DeadLetters:    1,
			Attempts:       4,
			OldestFailedAt: base.Add(-time.Hour),
			NewestFailedAt: base.Add(-time.Hour),
		},
	}
	if !reflect.DeepEqual(summary.Jobs, want) {
		t.Fatalf("Jobs = %#v, want %#v", summary.Jobs, want)
	}
}

func TestBucketJobLatenciesAddsOverflowBucket(t *testing.T) {
	t.Parallel()

	buckets := BucketJobLatencies(
		[]JobDashboardRecord{
			{ID: "fast", Latency: time.Millisecond},
			{ID: "slow", Latency: 5 * time.Second},
		},
		[]LatencyBucket{{Name: "fast", UpperBound: time.Second}},
	)

	want := []LatencyBucketCount{
		{Name: "fast", UpperBound: time.Second, Jobs: 1},
		{Name: "overflow", Jobs: 1},
	}
	if !reflect.DeepEqual(buckets, want) {
		t.Fatalf("buckets = %#v, want %#v", buckets, want)
	}
}

func TestWorkerHealthStatuses(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 21, 0, 0, 0, time.UTC)
	workers := SummarizeWorkerHealth([]WorkerSnapshot{
		{ID: "stopped", Queue: "q", Active: false, LastHeartbeat: now},
		{ID: "draining", Queue: "q", Active: true, Draining: true, LastHeartbeat: now},
		{ID: "missing-heartbeat", Queue: "q", Active: true},
		{ID: "healthy", Queue: "q", Active: true, LastHeartbeat: now.Add(-time.Second)},
		{Queue: "q", Active: true, LastHeartbeat: now},
	}, now, time.Minute)

	got := make(map[string]WorkerHealthStatus)
	for _, worker := range workers {
		got[worker.ID] = worker.Status
	}
	want := map[string]WorkerHealthStatus{
		"":                  WorkerHealthUnknown,
		"draining":          WorkerHealthDraining,
		"healthy":           WorkerHealthHealthy,
		"missing-heartbeat": WorkerHealthStale,
		"stopped":           WorkerHealthStopped,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("worker statuses = %#v, want %#v", got, want)
	}
}

func TestJobDashboardStatusFromProgress(t *testing.T) {
	t.Parallel()

	tests := []struct {
		state ProgressState
		want  JobDashboardStatus
	}{
		{ProgressStatePending, JobDashboardStatusQueued},
		{ProgressStateRunning, JobDashboardStatusRunning},
		{ProgressStateSucceeded, JobDashboardStatusSucceeded},
		{ProgressStateFailed, JobDashboardStatusFailed},
		{ProgressStateCanceled, JobDashboardStatusCanceled},
		{ProgressState("paused"), JobDashboardStatusUnknown},
	}

	for _, tt := range tests {
		if got := JobDashboardStatusFromProgress(tt.state); got != tt.want {
			t.Fatalf("JobDashboardStatusFromProgress(%q) = %q, want %q", tt.state, got, tt.want)
		}
	}
}

func queueNames(queues []QueueSummary) []string {
	out := make([]string, len(queues))
	for i, queue := range queues {
		out[i] = queue.Queue
	}
	return out
}

func workerIDs(workers []WorkerHealth) []string {
	out := make([]string, len(workers))
	for i, worker := range workers {
		out[i] = worker.ID
	}
	return out
}
