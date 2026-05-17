// Package notifications — throttle store surface for the
// `notification.throttle` sub-block. The dispatcher consults the
// store before each dispatch to gate per-recipient / per-channel
// bursts.
//
// Two implementations ship with the Lazuli runtime core:
//
//   - `MemoryThrottleStore` — in-process token bucket; reference for
//     unit tests and single-pod deployments. Safe for `testing/synctest`.
//   - real distributed stores (Redis, Postgres) live in `@runtime/...`
//     adapter packages and bind via
//     `@adapter.notification.throttle.<store>` in `registry.lzi`.
//
// Notifications expanded bucket cycle — language-side stub.
package notifications

import (
	"context"
	"sync"
	"time"

	"golang.org/x/time/rate"
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
// Wire-thin: delegates rate decisions to `golang.org/x/time/rate`
// (`rate.Limiter` per bucket). Each key gets a limiter with a refill
// of one token per `MaxPer / Burst` and a `Burst` ceiling, so a
// freshly-opened bucket admits `Burst` tokens immediately and
// refills smoothly over `MaxPer`. Goroutine-safe (the rate.Limiter
// is, and the map allocation is gated by sync.Mutex).
type MemoryThrottleStore struct {
	mu       sync.Mutex
	limiters map[memoryThrottleKey]*rate.Limiter
}

type memoryThrottleKey struct {
	notification string
	recipient    string
	channel      Channel
}

// NewMemoryThrottleStore returns an empty in-process throttle store.
func NewMemoryThrottleStore() *MemoryThrottleStore {
	return &MemoryThrottleStore{
		limiters: make(map[memoryThrottleKey]*rate.Limiter),
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
	burst := int(spec.Burst)
	if burst == 0 {
		// Without an explicit burst the bucket starts at 1 — i.e.
		// one immediate dispatch per window before throttling.
		burst = 1
	}

	mk := memoryThrottleKey{
		notification: key.Notification,
		recipient:    key.Recipient,
		channel:      key.Channel,
	}

	m.mu.Lock()
	limiter, ok := m.limiters[mk]
	if !ok {
		// One token per `window / burst` interval, capped at `burst`.
		// `rate.Every` returns `rate.Limit` such that `Burst` tokens
		// accumulate over `window`.
		interval := window / time.Duration(burst)
		limiter = rate.NewLimiter(rate.Every(interval), burst)
		m.limiters[mk] = limiter
	}
	m.mu.Unlock()

	now := time.Now()
	r := limiter.ReserveN(now, 1)
	if !r.OK() {
		// Cannot reserve at all (event count > burst). Should not
		// happen for N=1 unless burst==0, which we coerce above.
		return false, time.Time{}, ErrThrottleExceeded
	}
	delay := r.DelayFrom(now)
	if delay == 0 {
		return true, time.Time{}, nil
	}
	// Bucket exhausted — release the reservation so the caller's
	// rejection does not consume a future token slot.
	r.CancelAt(now)
	return false, now.Add(delay), ErrThrottleExceeded
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
