package cache

import "time"

// SlidingTTLPolicy controls adapter-neutral sliding expiry for cache entries.
//
// TTL is the absolute lifetime from the entry's StoredAt timestamp. SlidingTTL
// is the access renewal window. Both must be positive for renewal decisions to
// be enabled; ValidateQueryCacheConfig rejects generated configs that do not
// satisfy those bounds.
type SlidingTTLPolicy struct {
	TTL        time.Duration
	SlidingTTL time.Duration
}

// NewSlidingTTLPolicy returns the sliding TTL policy carried by a generated
// query-cache config.
func NewSlidingTTLPolicy(config QueryCacheConfig) SlidingTTLPolicy {
	return SlidingTTLPolicy{
		TTL:        config.TTL,
		SlidingTTL: config.SlidingTTL,
	}
}

// Enabled reports whether policy can renew cache entry expiry on access.
func (p SlidingTTLPolicy) Enabled() bool {
	return p.TTL > 0 && p.SlidingTTL > 0
}

// MaxExpiresAt returns the absolute expiry cap for entries stored at storedAt.
func (p SlidingTTLPolicy) MaxExpiresAt(storedAt time.Time) time.Time {
	return SlidingTTLMaxExpiresAt(storedAt, p.TTL)
}

// NextExpiresAt returns the renewal expiry that would be applied for a touch at
// now, capped by the entry's absolute lifetime.
func (p SlidingTTLPolicy) NextExpiresAt(now, storedAt time.Time) time.Time {
	return NextSlidingTTLExpiresAt(now, storedAt, p)
}

// Decide reports whether entry should be touched for a cache hit at now.
func (p SlidingTTLPolicy) Decide(now time.Time, entry SlidingTTLEntry) SlidingTTLDecision {
	return DecideSlidingTTL(now, entry, p)
}

// ShouldTouch reports whether entry should have its expiry updated after a
// cache hit at now.
func (p SlidingTTLPolicy) ShouldTouch(now time.Time, entry SlidingTTLEntry) bool {
	return p.Decide(now, entry).Touch
}

// SlidingTTLEntry carries the timestamps adapters need to make a sliding TTL
// decision for one cache entry.
type SlidingTTLEntry struct {
	StoredAt  time.Time
	ExpiresAt time.Time
}

// SlidingTTLDecision is the result of evaluating a cache hit under a sliding
// TTL policy.
type SlidingTTLDecision struct {
	// Touch is true when adapters should persist NextExpiresAt.
	Touch bool
	// Expired is true when ExpiresAt is finite and no later than now.
	Expired bool
	// NextExpiresAt is the finite expiry to persist when Touch is true.
	NextExpiresAt time.Time
	// MaxExpiresAt is the absolute lifetime cap derived from StoredAt and TTL.
	MaxExpiresAt time.Time
}

// SlidingTTLMaxExpiresAt returns the absolute expiry cap for an entry.
//
// A zero StoredAt or non-positive maxLifetime means no deterministic cap can be
// calculated.
func SlidingTTLMaxExpiresAt(storedAt time.Time, maxLifetime time.Duration) time.Time {
	if storedAt.IsZero() || maxLifetime <= 0 {
		return time.Time{}
	}
	return storedAt.Add(maxLifetime)
}

// NextSlidingTTLExpiresAt returns the next expiry for a touch at now.
//
// The result is now plus SlidingTTL, capped at StoredAt plus TTL. If policy is
// disabled or storedAt is missing, the result is zero.
func NextSlidingTTLExpiresAt(now, storedAt time.Time, policy SlidingTTLPolicy) time.Time {
	if !policy.Enabled() {
		return time.Time{}
	}

	maxExpiresAt := policy.MaxExpiresAt(storedAt)
	if maxExpiresAt.IsZero() {
		return time.Time{}
	}

	nextExpiresAt := now.Add(policy.SlidingTTL)
	if nextExpiresAt.After(maxExpiresAt) {
		return maxExpiresAt
	}
	return nextExpiresAt
}

// DecideSlidingTTL returns the access-time sliding TTL decision for entry.
//
// Expired or non-expiring entries are never touched. A finite, unexpired entry
// is touched only when the capped next expiry would extend its current expiry.
func DecideSlidingTTL(now time.Time, entry SlidingTTLEntry, policy SlidingTTLPolicy) SlidingTTLDecision {
	decision := SlidingTTLDecision{
		MaxExpiresAt:  policy.MaxExpiresAt(entry.StoredAt),
		NextExpiresAt: NextSlidingTTLExpiresAt(now, entry.StoredAt, policy),
	}

	if !entry.ExpiresAt.IsZero() && !entry.ExpiresAt.After(now) {
		decision.Expired = true
		return decision
	}
	if entry.ExpiresAt.IsZero() || decision.NextExpiresAt.IsZero() {
		return decision
	}

	decision.Touch = decision.NextExpiresAt.After(entry.ExpiresAt)
	return decision
}

// ShouldTouchSlidingTTL reports whether entry should have its expiry updated
// after a cache hit at now.
func ShouldTouchSlidingTTL(now time.Time, entry SlidingTTLEntry, policy SlidingTTLPolicy) bool {
	return DecideSlidingTTL(now, entry, policy).Touch
}
