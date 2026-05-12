// Package notifications - idempotency store surface for the
// `notification.idempotency` directive. The dispatcher claims the
// evaluated key before channel fan-out; an active duplicate claim is
// an intentional skip, not a delivery failure.
package notifications

import (
	"context"
	"sync"
	"time"
)

// IdempotencyStore records active notification idempotency claims.
// Implementations MUST be safe for concurrent use.
type IdempotencyStore interface {
	// Claim records key for ttl. It returns nil for a new or expired
	// key and ErrNotificationIdempotent when the key is already
	// claimed and unexpired.
	Claim(ctx context.Context, key IdempotencyKey, ttl time.Duration) error
}

// IdempotencyKey is the address used to deduplicate notification
// dispatch. Key is the evaluated `idempotency by <path>` value;
// Notification and Tenant keep independently authored notifications
// and tenants from suppressing each other.
type IdempotencyKey struct {
	Notification string
	Tenant       string
	Key          string
}

// MemoryIdempotencyStore is the in-process reference implementation.
// Safe for concurrent use; production deployments can swap this for
// an adapter-backed store via registry binding.
type MemoryIdempotencyStore struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu     sync.Mutex
	claims map[IdempotencyKey]time.Time
}

// NewMemoryIdempotencyStore returns an empty in-process idempotency store.
func NewMemoryIdempotencyStore() *MemoryIdempotencyStore {
	return &MemoryIdempotencyStore{
		claims: make(map[IdempotencyKey]time.Time),
	}
}

// Claim implements IdempotencyStore.
func (m *MemoryIdempotencyStore) Claim(
	_ context.Context,
	key IdempotencyKey,
	ttl time.Duration,
) error {
	if ttl <= 0 {
		return ErrInvalidDuration
	}

	now := m.now()
	expiresAt := now.Add(ttl)

	m.mu.Lock()
	defer m.mu.Unlock()

	m.expireLocked(now)
	if existing, ok := m.claims[key]; ok && now.Before(existing) {
		return ErrNotificationIdempotent
	}
	m.claims[key] = expiresAt
	return nil
}

func (m *MemoryIdempotencyStore) now() time.Time {
	if m.Clock != nil {
		return m.Clock()
	}
	return time.Now()
}

func (m *MemoryIdempotencyStore) expireLocked(now time.Time) {
	for key, expiresAt := range m.claims {
		if !now.Before(expiresAt) {
			delete(m.claims, key)
		}
	}
}
