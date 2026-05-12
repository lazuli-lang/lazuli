package jobs

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"sync"
	"time"
)

// ErrDeadLetterEntryNotFound is returned when a DLQ helper cannot find
// the requested entry.
var ErrDeadLetterEntryNotFound = errors.New("jobs: dead-letter entry not found")

// DeadLetterEntry is the stored snapshot for a job that exhausted its
// retry budget.
type DeadLetterEntry struct {
	// ID is the store-local dead-letter entry identifier. Memory stores
	// generate one when callers leave it empty.
	ID string
	// Feature is the owning feature name.
	Feature string
	// Name is the job identifier within Feature.
	Name string
	// EnvelopeID is the failed job envelope id.
	EnvelopeID string
	// Tenant is the tenant resolved for the failed execution.
	Tenant string
	// Payload is the failed job payload snapshot.
	Payload map[string]any
	// Attempts is the number of attempted executions before dead-lettering.
	Attempts uint32
	// Error is the terminal error message captured at failure time.
	Error string
	// FailedAt is the time the job was routed to the dead-letter store.
	FailedAt time.Time
}

// DeadLetterFilter selects entries for List. Zero values are wildcards.
type DeadLetterFilter struct {
	Feature string
	Name    string
	Tenant  string
}

// DeadLetterStore persists exhausted job executions. Implementations
// must be safe for concurrent use.
type DeadLetterStore interface {
	// Append stores entry and returns the stored snapshot. Stores may add
	// metadata such as ID or FailedAt when callers leave them empty.
	Append(ctx context.Context, entry DeadLetterEntry) (DeadLetterEntry, error)
	// List returns active dead-letter entries matching filter in
	// store-defined order.
	List(ctx context.Context, filter DeadLetterFilter) ([]DeadLetterEntry, error)
	// Ack acknowledges and removes an active dead-letter entry.
	Ack(ctx context.Context, id string) error
	// RequeueReady returns active entries whose FailedAt is at or before
	// readyAt. A limit less than or equal to zero means no limit.
	RequeueReady(ctx context.Context, readyAt time.Time, limit int) ([]DeadLetterEntry, error)
}

// MemoryDeadLetterStore is an in-memory DeadLetterStore safe for
// concurrent use. The zero value is ready to use.
type MemoryDeadLetterStore struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu      sync.RWMutex
	nextID  uint64
	entries []DeadLetterEntry
	index   map[string]int
}

var _ DeadLetterStore = (*MemoryDeadLetterStore)(nil)

// NewMemoryDeadLetterStore returns an empty in-process dead-letter store.
func NewMemoryDeadLetterStore() *MemoryDeadLetterStore {
	return &MemoryDeadLetterStore{
		index: make(map[string]int),
	}
}

// Append stores entry and returns the stored snapshot.
func (m *MemoryDeadLetterStore) Append(ctx context.Context, entry DeadLetterEntry) (DeadLetterEntry, error) {
	if err := deadLetterContextErr(ctx); err != nil {
		return DeadLetterEntry{}, err
	}

	m.mu.Lock()
	defer m.mu.Unlock()
	m.ensureLocked()

	entry = cloneDeadLetterEntry(entry)
	if entry.ID == "" {
		entry.ID = m.nextIDLocked()
	} else if _, exists := m.index[entry.ID]; exists {
		return DeadLetterEntry{}, fmt.Errorf("jobs: dead-letter entry %q already exists", entry.ID)
	}
	if entry.FailedAt.IsZero() {
		entry.FailedAt = m.nowLocked().UTC()
	}

	m.index[entry.ID] = len(m.entries)
	m.entries = append(m.entries, entry)
	return cloneDeadLetterEntry(entry), nil
}

// List returns active dead-letter entries matching filter in insertion order.
func (m *MemoryDeadLetterStore) List(ctx context.Context, filter DeadLetterFilter) ([]DeadLetterEntry, error) {
	if err := deadLetterContextErr(ctx); err != nil {
		return nil, err
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	out := make([]DeadLetterEntry, 0, len(m.entries))
	for _, entry := range m.entries {
		if filter.matches(entry) {
			out = append(out, cloneDeadLetterEntry(entry))
		}
	}
	return out, nil
}

// Ack acknowledges and removes an active dead-letter entry.
func (m *MemoryDeadLetterStore) Ack(ctx context.Context, id string) error {
	if err := deadLetterContextErr(ctx); err != nil {
		return err
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	idx, ok := m.index[id]
	if !ok {
		return ErrDeadLetterEntryNotFound
	}

	var zero DeadLetterEntry
	copy(m.entries[idx:], m.entries[idx+1:])
	m.entries[len(m.entries)-1] = zero
	m.entries = m.entries[:len(m.entries)-1]
	delete(m.index, id)
	for i := idx; i < len(m.entries); i++ {
		m.index[m.entries[i].ID] = i
	}
	return nil
}

// RequeueReady returns active entries whose FailedAt is at or before readyAt.
func (m *MemoryDeadLetterStore) RequeueReady(
	ctx context.Context,
	readyAt time.Time,
	limit int,
) ([]DeadLetterEntry, error) {
	if err := deadLetterContextErr(ctx); err != nil {
		return nil, err
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	out := make([]DeadLetterEntry, 0)
	for _, entry := range m.entries {
		if entry.FailedAt.After(readyAt) {
			continue
		}
		out = append(out, cloneDeadLetterEntry(entry))
		if limit > 0 && len(out) >= limit {
			break
		}
	}
	return out, nil
}

func (m *MemoryDeadLetterStore) ensureLocked() {
	if m.index == nil {
		m.index = make(map[string]int)
		for i := range m.entries {
			m.index[m.entries[i].ID] = i
		}
	}
}

func (m *MemoryDeadLetterStore) nextIDLocked() string {
	for {
		m.nextID++
		id := "dlq-" + strconv.FormatUint(m.nextID, 10)
		if _, exists := m.index[id]; !exists {
			return id
		}
	}
}

func (m *MemoryDeadLetterStore) nowLocked() time.Time {
	if m.Clock != nil {
		return m.Clock()
	}
	return time.Now()
}

func (f DeadLetterFilter) matches(entry DeadLetterEntry) bool {
	if f.Feature != "" && f.Feature != entry.Feature {
		return false
	}
	if f.Name != "" && f.Name != entry.Name {
		return false
	}
	if f.Tenant != "" && f.Tenant != entry.Tenant {
		return false
	}
	return true
}

func deadLetterContextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}

func cloneDeadLetterEntry(entry DeadLetterEntry) DeadLetterEntry {
	entry.Payload = cloneDeadLetterPayload(entry.Payload)
	return entry
}

func cloneDeadLetterPayload(payload map[string]any) map[string]any {
	if payload == nil {
		return nil
	}
	out := make(map[string]any, len(payload))
	for k, v := range payload {
		out[k] = cloneDeadLetterValue(v)
	}
	return out
}

func cloneDeadLetterValue(value any) any {
	switch v := value.(type) {
	case map[string]any:
		return cloneDeadLetterPayload(v)
	case []any:
		out := make([]any, len(v))
		for i := range v {
			out[i] = cloneDeadLetterValue(v[i])
		}
		return out
	case []byte:
		out := make([]byte, len(v))
		copy(out, v)
		return out
	default:
		return v
	}
}
