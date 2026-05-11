// Package notifications — throttle store surface for the
// `notification.throttle` sub-block. The dispatcher consults the
// store before each dispatch to gate per-recipient / per-channel
// bursts.
//
// Two implementations ship with Drusa core:
//
//   - `MemoryThrottleStore` — in-process token bucket; reference for
//     unit tests and single-pod deployments. Safe for `testing/synctest`.
//   - real distributed stores (Redis, Postgres) live in `@drusa/...`
//     adapter packages and bind via
//     `@adapter.notification.throttle.<store>` in `registry.lzi`.
//
// Notifications expanded bucket cycle — language-side stub.
package notifications

import (
	"context"
	"sync"
	"time"
)

// ThrottleStore gates dispatches per the contract's
// `NotificationThrottle`. The dispatcher computes the bucket key
// from `Recipient` + `Channel` according to the contract's
// `PerRecipient` / `PerChannel` flags.
type ThrottleStore interface {
	// Allow returns (true, _) when the dispatch may proceed and
	// consumes one token from the bucket; otherwise returns
	// (false, retryAt) so the caller knows when the bucket refills.
	// Implementations MUST be safe for concurrent use.
	Allow(
		ctx context.Context,
		key ThrottleKey,
		spec NotificationThrottle,
	) (allowed bool, retryAt time.Time, err error)
}

// ThrottleKey is the bucket address. `Notification` always
// participates; `Recipient` / `Channel` are populated only when the
// matching `Per*` axis is set on the contract.
type ThrottleKey struct {
	Notification string
	Recipient    string
	Channel      Channel
}

// MemoryThrottleStore is the in-process reference implementation.
// It uses a fixed-window approximation of a token bucket: each key
// gets `Burst + (Refills since open)` tokens up to `Burst`,
// refilling one token every `MaxPer / 1` (i.e. the full window
// refills the bucket from empty). The approximation is precise
// enough for `testing/synctest` and reference scenarios; production
// adapters can swap in a leaky-bucket / sliding-window backend.
type MemoryThrottleStore struct {
	mu      sync.Mutex
	buckets map[memoryThrottleKey]*memoryThrottleBucket
}

type memoryThrottleKey struct {
	notification string
	recipient    string
	channel      Channel
}

type memoryThrottleBucket struct {
	tokens     uint32
	updatedAt  time.Time
	windowSpan time.Duration
}

// NewMemoryThrottleStore returns an empty in-process throttle store.
func NewMemoryThrottleStore() *MemoryThrottleStore {
	return &MemoryThrottleStore{
		buckets: make(map[memoryThrottleKey]*memoryThrottleBucket),
	}
}

// Allow implements ThrottleStore.
func (m *MemoryThrottleStore) Allow(
	_ context.Context,
	key ThrottleKey,
	spec NotificationThrottle,
) (bool, time.Time, error) {
	window, err := parseDuration(spec.MaxPer)
	if err != nil {
		return false, time.Time{}, err
	}
	burst := spec.Burst
	if burst == 0 {
		// Without an explicit burst the bucket starts at 1 — i.e.
		// one immediate dispatch per window before throttling.
		burst = 1
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	mk := memoryThrottleKey{
		notification: key.Notification,
		recipient:    key.Recipient,
		channel:      key.Channel,
	}
	bucket, ok := m.buckets[mk]
	now := time.Now()
	if !ok {
		bucket = &memoryThrottleBucket{
			tokens:     burst,
			updatedAt:  now,
			windowSpan: window,
		}
		m.buckets[mk] = bucket
	} else {
		// Refill: one full window adds `burst` tokens, capped at
		// `burst`. Partial windows refill proportionally rounded
		// down.
		elapsed := now.Sub(bucket.updatedAt)
		if elapsed >= bucket.windowSpan {
			refills := uint32(elapsed / bucket.windowSpan)
			bucket.tokens = saturatingAdd(bucket.tokens, refills*burst, burst)
			bucket.updatedAt = now
		}
	}

	if bucket.tokens == 0 {
		retryAt := bucket.updatedAt.Add(bucket.windowSpan)
		return false, retryAt, ErrThrottleExceeded
	}
	bucket.tokens--
	return true, time.Time{}, nil
}

// saturatingAdd adds `delta` to `current`, clamped at `ceiling`.
func saturatingAdd(current, delta, ceiling uint32) uint32 {
	sum := current + delta
	if sum < current || sum > ceiling {
		return ceiling
	}
	return sum
}

// parseDuration accepts the same shape the DSL allows:
// `<N>(s|sec|seconds|m|min|minutes|h|hour|hours|d|day|days)` with
// optional whitespace. Doctor (`NOTIF-THROTTLE-001`) rejects
// malformed literals before they reach the runtime, but the parser
// is defensive so adapter packs don't have to repeat the logic.
func parseDuration(raw string) (time.Duration, error) {
	trimmed := raw
	// Strip surrounding whitespace; the DSL emits quoted values so
	// callers may pass the raw literal verbatim.
	for len(trimmed) > 0 && (trimmed[0] == ' ' || trimmed[0] == '\t') {
		trimmed = trimmed[1:]
	}
	for len(trimmed) > 0 && (trimmed[len(trimmed)-1] == ' ' || trimmed[len(trimmed)-1] == '\t') {
		trimmed = trimmed[:len(trimmed)-1]
	}

	idx := 0
	for idx < len(trimmed) && trimmed[idx] >= '0' && trimmed[idx] <= '9' {
		idx++
	}
	if idx == 0 {
		return 0, ErrInvalidDuration
	}
	num := trimmed[:idx]
	unit := trimmed[idx:]
	for len(unit) > 0 && unit[0] == ' ' {
		unit = unit[1:]
	}
	var n int64
	for _, ch := range []byte(num) {
		n = n*10 + int64(ch-'0')
	}
	switch unit {
	case "s", "sec", "secs", "second", "seconds":
		return time.Duration(n) * time.Second, nil
	case "m", "min", "mins", "minute", "minutes":
		return time.Duration(n) * time.Minute, nil
	case "h", "hr", "hrs", "hour", "hours":
		return time.Duration(n) * time.Hour, nil
	case "d", "day", "days":
		return time.Duration(n) * 24 * time.Hour, nil
	default:
		return 0, ErrInvalidDuration
	}
}
