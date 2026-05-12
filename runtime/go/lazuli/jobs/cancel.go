package jobs

import (
	"context"
	"sync"
)

// CancellationRegistry tracks cancellable job contexts by job id.
//
// The zero value is ready to use. Call Start when a job begins, pass the
// returned context through the job execution path, and call Finish when the job
// exits so the registry can drop its entry.
type CancellationRegistry struct {
	mu      sync.RWMutex
	entries map[string]*cancellationEntry
}

type cancellationEntry struct {
	ctx       context.Context
	cancel    context.CancelCauseFunc
	cancelled bool
	reason    string
}

type cancellationReason string

func (r cancellationReason) Error() string { return string(r) }

// NewCancellationRegistry returns an empty cancellation registry.
func NewCancellationRegistry() *CancellationRegistry {
	return &CancellationRegistry{entries: make(map[string]*cancellationEntry)}
}

// Start returns the cancellable context registered for id.
//
// If id is already active, Start returns the existing context so every caller
// observes the same cancellation signal. A later Finish removes the active
// entry and allows the id to be reused for a fresh context.
func (r *CancellationRegistry) Start(ctx context.Context, id string) context.Context {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	if entry, ok := r.entries[id]; ok {
		return entry.ctx
	}
	jobCtx, cancel := context.WithCancelCause(ctx)
	r.entries[id] = &cancellationEntry{ctx: jobCtx, cancel: cancel}
	return jobCtx
}

// Cancel cancels the active context for id and records reason.
//
// Cancel returns false when id is not active. Repeated calls for the same active
// id keep the first cancellation reason because contexts can only be cancelled
// once.
func (r *CancellationRegistry) Cancel(id, reason string) bool {
	r.mu.Lock()
	entry, ok := r.entries[id]
	if !ok {
		r.mu.Unlock()
		return false
	}
	if entry.cancelled {
		r.mu.Unlock()
		return true
	}
	entry.cancelled = true
	entry.reason = reason
	cancel := entry.cancel
	r.mu.Unlock()

	if reason == "" {
		cancel(context.Canceled)
		return true
	}
	cancel(cancellationReason(reason))
	return true
}

// IsCancelled reports whether Cancel has been called for id while it is active.
func (r *CancellationRegistry) IsCancelled(id string) bool {
	r.mu.RLock()
	defer r.mu.RUnlock()
	entry, ok := r.entries[id]
	return ok && entry.cancelled
}

// Reason returns the recorded cancellation reason for id.
//
// Reason returns an empty string when id is unknown, active but not cancelled,
// or cancelled with an empty reason.
func (r *CancellationRegistry) Reason(id string) string {
	r.mu.RLock()
	defer r.mu.RUnlock()
	entry, ok := r.entries[id]
	if !ok || !entry.cancelled {
		return ""
	}
	return entry.reason
}

// Finish removes id from the registry and releases its context resources.
//
// Finish is safe to call more than once.
func (r *CancellationRegistry) Finish(id string) {
	r.mu.Lock()
	entry, ok := r.entries[id]
	if ok {
		delete(r.entries, id)
	}
	r.mu.Unlock()
	if ok {
		entry.cancel(context.Canceled)
	}
}

func (r *CancellationRegistry) initLocked() {
	if r.entries == nil {
		r.entries = make(map[string]*cancellationEntry)
	}
}
