package webhooks

import (
	"context"
	"sync"
	"time"
)

const idempotencyGCInterval = time.Minute

// IdempotencyStore records inbound webhook envelope ids for duplicate
// suppression. Implementations MUST make Claim atomic for a single
// envelope id and return ErrWebhookIdempotent while the id is still
// within its TTL.
type IdempotencyStore interface {
	// Claim records envelopeID until ttl elapses. It returns
	// ErrWebhookIdempotent when envelopeID was already claimed and has
	// not expired yet.
	Claim(ctx context.Context, envelopeID string, ttl time.Duration) error
}

// MemoryIdempotencyStore is an in-process IdempotencyStore reference
// implementation. It is safe for concurrent use; production deployments
// that need cross-process dedupe should bind a persistent adapter.
//
// The zero value is ready to use.
type MemoryIdempotencyStore struct {
	mu      sync.Mutex
	claims  map[string]time.Time
	lastGC  time.Time
	nowFunc func() time.Time
}

// NewMemoryIdempotencyStore returns an empty in-process idempotency store.
func NewMemoryIdempotencyStore() *MemoryIdempotencyStore {
	return &MemoryIdempotencyStore{
		claims: make(map[string]time.Time),
	}
}

// Claim implements IdempotencyStore.
func (m *MemoryIdempotencyStore) Claim(ctx context.Context, envelopeID string, ttl time.Duration) error {
	if ctx != nil {
		if err := ctx.Err(); err != nil {
			return err
		}
	}

	now := m.now()
	expiresAt := now.Add(ttl)

	m.mu.Lock()
	defer m.mu.Unlock()

	if m.claims == nil {
		m.claims = make(map[string]time.Time)
	}
	if existingExpiresAt, ok := m.claims[envelopeID]; ok && now.Before(existingExpiresAt) {
		return ErrWebhookIdempotent
	}

	m.claims[envelopeID] = expiresAt
	if m.lastGC.IsZero() || now.Sub(m.lastGC) >= idempotencyGCInterval {
		m.gcLocked(now)
	}
	return nil
}

func (m *MemoryIdempotencyStore) now() time.Time {
	if m.nowFunc != nil {
		return m.nowFunc()
	}
	return time.Now()
}

func (m *MemoryIdempotencyStore) gcLocked(now time.Time) {
	m.lastGC = now
	for envelopeID, expiresAt := range m.claims {
		if !now.Before(expiresAt) {
			delete(m.claims, envelopeID)
		}
	}
}
