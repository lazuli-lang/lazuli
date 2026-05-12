package jobs

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"sync"
	"time"
)

// ProgressState is the lifecycle state attached to a job progress snapshot.
type ProgressState string

const (
	// ProgressStatePending means the job is known but has not started work.
	ProgressStatePending ProgressState = "pending"
	// ProgressStateRunning means the job is currently doing work.
	ProgressStateRunning ProgressState = "running"
	// ProgressStateSucceeded means the job finished successfully.
	ProgressStateSucceeded ProgressState = "succeeded"
	// ProgressStateFailed means the job finished with an error.
	ProgressStateFailed ProgressState = "failed"
	// ProgressStateCanceled means the job was stopped before completion.
	ProgressStateCanceled ProgressState = "canceled"
)

var (
	// ErrProgressInvalid is returned when a progress update has an invalid job
	// id, percent, or state.
	ErrProgressInvalid = errors.New("jobs: invalid progress")
	// ErrProgressTerminal is returned when an update targets a job that already
	// reached a terminal progress state.
	ErrProgressTerminal = errors.New("jobs: progress is terminal")
)

// Terminal reports whether state is final and cannot be followed by more work.
func (s ProgressState) Terminal() bool {
	switch s {
	case ProgressStateSucceeded, ProgressStateFailed, ProgressStateCanceled:
		return true
	default:
		return false
	}
}

func (s ProgressState) valid() bool {
	switch s {
	case ProgressStatePending,
		ProgressStateRunning,
		ProgressStateSucceeded,
		ProgressStateFailed,
		ProgressStateCanceled:
		return true
	default:
		return false
	}
}

// Progress is the latest progress snapshot for one job execution. The store
// owns the timestamps: CreatedAt is set on first update, UpdatedAt on every
// update, and FinishedAt when State is terminal.
type Progress struct {
	// JobID is the stable id for one job execution.
	JobID string
	// State is the current lifecycle state. An empty state on Update is
	// normalized to ProgressStateRunning.
	State ProgressState
	// Percent is the current completion percentage, inclusive between 0 and 100.
	Percent int
	// Message is an optional human-readable status message.
	Message string
	// Metadata carries small adapter- or application-supplied progress labels.
	Metadata map[string]string
	// CreatedAt is when the first progress update for JobID was stored.
	CreatedAt time.Time
	// UpdatedAt is when this snapshot was stored.
	UpdatedAt time.Time
	// FinishedAt is set when State is terminal.
	FinishedAt time.Time
}

// Terminal reports whether the progress snapshot is in a final state.
func (p Progress) Terminal() bool {
	return p.State.Terminal()
}

// ProgressStore persists job progress snapshots. Implementations MUST be safe
// for concurrent use.
type ProgressStore interface {
	// Update stores the latest progress for progress.JobID and returns the
	// normalized snapshot with store-owned timestamps.
	Update(ctx context.Context, progress Progress) (Progress, error)
	// Get returns the latest progress for jobID.
	Get(ctx context.Context, jobID string) (Progress, bool, error)
	// List returns all known progress snapshots sorted by job id.
	List(ctx context.Context) ([]Progress, error)
}

// MemoryProgressStore is the in-process reference implementation for job
// progress reporting. It is intended for tests and single-instance deployments;
// production adapters can bind their own ProgressStore implementations.
type MemoryProgressStore struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu       sync.RWMutex
	progress map[string]Progress
}

var _ ProgressStore = (*MemoryProgressStore)(nil)

// NewMemoryProgressStore returns an empty in-process progress store.
func NewMemoryProgressStore() *MemoryProgressStore {
	return &MemoryProgressStore{
		progress: make(map[string]Progress),
	}
}

// Update implements ProgressStore.
func (m *MemoryProgressStore) Update(ctx context.Context, progress Progress) (Progress, error) {
	if err := ctx.Err(); err != nil {
		return Progress{}, err
	}

	progress, err := normalizeProgressUpdate(progress)
	if err != nil {
		return Progress{}, err
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	now := m.now()

	if m.progress == nil {
		m.progress = make(map[string]Progress)
	}

	existing, ok := m.progress[progress.JobID]
	if ok && existing.State.Terminal() {
		return Progress{}, fmt.Errorf("%w: job %q state %q", ErrProgressTerminal, progress.JobID, existing.State)
	}

	if ok && !existing.CreatedAt.IsZero() {
		progress.CreatedAt = existing.CreatedAt
	} else {
		progress.CreatedAt = now
	}
	progress.UpdatedAt = now
	if progress.State.Terminal() {
		progress.FinishedAt = now
	} else {
		progress.FinishedAt = time.Time{}
	}

	m.progress[progress.JobID] = progress
	return cloneProgress(progress), nil
}

// Get implements ProgressStore.
func (m *MemoryProgressStore) Get(ctx context.Context, jobID string) (Progress, bool, error) {
	if err := ctx.Err(); err != nil {
		return Progress{}, false, err
	}
	if jobID == "" {
		return Progress{}, false, fmt.Errorf("%w: job id required", ErrProgressInvalid)
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	progress, ok := m.progress[jobID]
	if !ok {
		return Progress{}, false, nil
	}
	return cloneProgress(progress), true, nil
}

// List implements ProgressStore.
func (m *MemoryProgressStore) List(ctx context.Context) ([]Progress, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	m.mu.RLock()
	progress := make([]Progress, 0, len(m.progress))
	for _, item := range m.progress {
		progress = append(progress, cloneProgress(item))
	}
	m.mu.RUnlock()

	sort.Slice(progress, func(i, j int) bool {
		return progress[i].JobID < progress[j].JobID
	})
	return progress, nil
}

func (m *MemoryProgressStore) now() time.Time {
	if m.Clock != nil {
		return m.Clock()
	}
	return time.Now()
}

func normalizeProgressUpdate(progress Progress) (Progress, error) {
	if progress.JobID == "" {
		return Progress{}, fmt.Errorf("%w: job id required", ErrProgressInvalid)
	}
	if progress.Percent < 0 || progress.Percent > 100 {
		return Progress{}, fmt.Errorf("%w: percent %d outside 0..100", ErrProgressInvalid, progress.Percent)
	}
	if progress.State == "" {
		progress.State = ProgressStateRunning
	}
	if !progress.State.valid() {
		return Progress{}, fmt.Errorf("%w: state %q", ErrProgressInvalid, progress.State)
	}
	progress.Metadata = cloneProgressMetadata(progress.Metadata)
	return progress, nil
}

func cloneProgress(progress Progress) Progress {
	progress.Metadata = cloneProgressMetadata(progress.Metadata)
	return progress
}

func cloneProgressMetadata(metadata map[string]string) map[string]string {
	if metadata == nil {
		return nil
	}
	out := make(map[string]string, len(metadata))
	for key, value := range metadata {
		out[key] = value
	}
	return out
}
