package webhooks

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"time"
)

// ErrWebhookDLQDuplicate means the caller supplied a DLQ entry id that is
// already pending in the store.
var ErrWebhookDLQDuplicate = errors.New("webhooks: dlq entry already exists")

// DLQEntry is an exhausted inbound webhook delivery captured for later
// inspection, acknowledgement, or requeue.
//
// ID is store-local metadata assigned by Append when omitted. Feature and Name
// identify the lowered webhook contract; EnvelopeID is the verified inbound
// envelope id. Timestamp records when the entry was dead-lettered.
type DLQEntry struct {
	ID         string
	Feature    string
	Name       string
	EnvelopeID string
	Body       []byte
	Error      string
	Attempts   int
	Timestamp  time.Time
}

// DLQStore stores exhausted webhook deliveries. Implementations MUST be safe
// for concurrent use.
type DLQStore interface {
	// Append stores entry and returns the stored copy. Implementations may add
	// metadata such as ID and Timestamp when the caller leaves them empty.
	Append(ctx context.Context, entry DLQEntry) (DLQEntry, error)
	// List returns pending entries in store-defined order.
	List(ctx context.Context) ([]DLQEntry, error)
	// Ack removes a pending entry after it has been handled.
	Ack(ctx context.Context, id string) (bool, error)
	// Requeue removes and returns a pending entry for the caller to enqueue
	// again. The store does not dispatch or bind to a queue provider.
	Requeue(ctx context.Context, id string) (DLQEntry, bool, error)
}

// MemoryDLQStore is an in-process DLQStore reference implementation. It is
// safe for concurrent use; production deployments that need durable
// dead-letter handling should bind a persistent adapter.
//
// The zero value is ready to use. List returns entries in insertion order.
type MemoryDLQStore struct {
	mu      sync.RWMutex
	next    uint64
	entries []DLQEntry
	index   map[string]int
	nowFunc func() time.Time
}

// NewMemoryDLQStore returns an empty in-process DLQ store.
func NewMemoryDLQStore() *MemoryDLQStore {
	return &MemoryDLQStore{
		index: make(map[string]int),
	}
}

// Append implements DLQStore.
func (m *MemoryDLQStore) Append(ctx context.Context, entry DLQEntry) (DLQEntry, error) {
	if err := dlqContextErr(ctx); err != nil {
		return DLQEntry{}, err
	}

	m.mu.Lock()
	defer m.mu.Unlock()
	m.ensureLocked()

	if entry.ID == "" {
		entry.ID = m.nextIDLocked()
	} else if _, exists := m.index[entry.ID]; exists {
		return DLQEntry{}, ErrWebhookDLQDuplicate
	}
	if entry.Timestamp.IsZero() {
		entry.Timestamp = m.nowLocked()
	}
	entry.Body = cloneWebhookDLQBody(entry.Body)

	m.index[entry.ID] = len(m.entries)
	m.entries = append(m.entries, entry)
	return cloneWebhookDLQEntry(entry), nil
}

// List implements DLQStore.
func (m *MemoryDLQStore) List(ctx context.Context) ([]DLQEntry, error) {
	if err := dlqContextErr(ctx); err != nil {
		return nil, err
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	entries := make([]DLQEntry, len(m.entries))
	for i, entry := range m.entries {
		entries[i] = cloneWebhookDLQEntry(entry)
	}
	return entries, nil
}

// Ack implements DLQStore.
func (m *MemoryDLQStore) Ack(ctx context.Context, id string) (bool, error) {
	if err := dlqContextErr(ctx); err != nil {
		return false, err
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	idx, ok := m.index[id]
	if !ok {
		return false, nil
	}
	m.deleteLocked(idx)
	return true, nil
}

// Requeue implements DLQStore.
func (m *MemoryDLQStore) Requeue(ctx context.Context, id string) (DLQEntry, bool, error) {
	if err := dlqContextErr(ctx); err != nil {
		return DLQEntry{}, false, err
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	idx, ok := m.index[id]
	if !ok {
		return DLQEntry{}, false, nil
	}
	entry := cloneWebhookDLQEntry(m.entries[idx])
	m.deleteLocked(idx)
	return entry, true, nil
}

func (m *MemoryDLQStore) ensureLocked() {
	if m.index == nil {
		m.index = make(map[string]int)
	}
}

func (m *MemoryDLQStore) nextIDLocked() string {
	for {
		m.next++
		id := "dlq-" + strconv.FormatUint(m.next, 10)
		if _, exists := m.index[id]; !exists {
			return id
		}
	}
}

func (m *MemoryDLQStore) nowLocked() time.Time {
	if m.nowFunc != nil {
		return m.nowFunc().UTC()
	}
	return time.Now().UTC()
}

func (m *MemoryDLQStore) deleteLocked(idx int) {
	entry := m.entries[idx]
	copy(m.entries[idx:], m.entries[idx+1:])
	m.entries[len(m.entries)-1] = DLQEntry{}
	m.entries = m.entries[:len(m.entries)-1]
	delete(m.index, entry.ID)
	for i := idx; i < len(m.entries); i++ {
		m.index[m.entries[i].ID] = i
	}
}

func cloneWebhookDLQEntry(entry DLQEntry) DLQEntry {
	entry.Body = cloneWebhookDLQBody(entry.Body)
	return entry
}

func cloneWebhookDLQBody(body []byte) []byte {
	if body == nil {
		return nil
	}
	out := make([]byte, len(body))
	copy(out, body)
	return out
}

func dlqContextErr(ctx context.Context) error {
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
