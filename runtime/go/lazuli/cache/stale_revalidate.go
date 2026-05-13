package cache

import (
	"errors"
	"fmt"
	"time"
)

const staleRevalidateRefreshMarkerPrefix = "refresh"

var (
	// ErrInvalidStaleRevalidatePolicy reports an invalid stale-while-revalidate policy.
	ErrInvalidStaleRevalidatePolicy = errors.New("lazuli/cache: invalid stale-while-revalidate policy")
)

// StaleRevalidatePolicy describes adapter-neutral stale-while-revalidate
// behavior for one cache entry.
type StaleRevalidatePolicy struct {
	// FreshFor is the time an entry is fresh. Zero makes the entry immediately
	// stale or expired. Negative means the entry does not expire.
	FreshFor time.Duration
	// StaleFor is the time after FreshFor when stale bytes may still be served.
	StaleFor time.Duration
	// RefreshMarkerTTL is how long a background refresh marker suppresses
	// duplicate refreshes. Zero uses the remaining stale window.
	RefreshMarkerTTL time.Duration
}

// Validate reports invalid stale-while-revalidate policy combinations.
func (p StaleRevalidatePolicy) Validate() error {
	var errs []error
	if p.StaleFor < 0 {
		errs = append(errs, fmt.Errorf("%w: StaleFor must not be negative", ErrInvalidStaleRevalidatePolicy))
	}
	if p.RefreshMarkerTTL < 0 {
		errs = append(errs, fmt.Errorf("%w: RefreshMarkerTTL must not be negative", ErrInvalidStaleRevalidatePolicy))
	}
	if p.FreshFor < 0 && p.StaleFor > 0 {
		errs = append(errs, fmt.Errorf("%w: StaleFor requires an expiring FreshFor", ErrInvalidStaleRevalidatePolicy))
	}
	return errors.Join(errs...)
}

// Window returns freshness metadata for an entry stored at storedAt.
func (p StaleRevalidatePolicy) Window(storedAt time.Time) StaleRevalidateWindow {
	if p.FreshFor < 0 {
		return StaleRevalidateWindow{StoredAt: storedAt}
	}

	staleFor := p.StaleFor
	if staleFor < 0 {
		staleFor = 0
	}

	freshUntil := storedAt.Add(p.FreshFor)
	return StaleRevalidateWindow{
		StoredAt:   storedAt,
		FreshUntil: freshUntil,
		StaleUntil: freshUntil.Add(staleFor),
	}
}

// Decide returns the serve and refresh behavior for a stored entry at now.
func (p StaleRevalidatePolicy) Decide(window StaleRevalidateWindow, now time.Time, marker StaleRevalidateRefreshMarker) StaleRevalidateDecision {
	state := window.State(now)
	decision := StaleRevalidateDecision{
		State:      state,
		Serve:      state != StaleRevalidateExpired,
		ServeStale: state == StaleRevalidateStale,
		Refresh:    state != StaleRevalidateFresh,
	}
	if decision.ServeStale && !marker.Active(now) {
		decision.BackgroundRefresh = true
		decision.RefreshMarker = p.RefreshMarker(window, now)
	}
	return decision
}

// RefreshMarker returns a marker value for a background refresh starting at now.
//
// A zero RefreshMarkerTTL keeps the marker active until the stale window closes.
// Positive TTLs are capped to the stale window so markers never outlive the
// entry they protect.
func (p StaleRevalidatePolicy) RefreshMarker(window StaleRevalidateWindow, now time.Time) StaleRevalidateRefreshMarker {
	if window.State(now) != StaleRevalidateStale {
		return StaleRevalidateRefreshMarker{}
	}

	expiresAt := window.StaleUntil
	if p.RefreshMarkerTTL > 0 {
		expiresAt = now.Add(p.RefreshMarkerTTL)
		if expiresAt.After(window.StaleUntil) {
			expiresAt = window.StaleUntil
		}
	}
	if !expiresAt.After(now) {
		return StaleRevalidateRefreshMarker{}
	}

	return StaleRevalidateRefreshMarker{
		MarkedAt:  now,
		ExpiresAt: expiresAt,
	}
}

// StaleRevalidateWindow records freshness and staleness boundaries for an entry.
type StaleRevalidateWindow struct {
	StoredAt   time.Time
	FreshUntil time.Time
	StaleUntil time.Time
}

// StaleRevalidateState describes whether an entry may be served at a point in time.
type StaleRevalidateState int

const (
	// StaleRevalidateFresh means the entry is within its freshness window.
	StaleRevalidateFresh StaleRevalidateState = iota
	// StaleRevalidateStale means the entry is outside freshness but inside the stale window.
	StaleRevalidateStale
	// StaleRevalidateExpired means the entry should not be served.
	StaleRevalidateExpired
)

// State reports the entry freshness state at now.
func (w StaleRevalidateWindow) State(now time.Time) StaleRevalidateState {
	if w.FreshUntil.IsZero() || now.Before(w.FreshUntil) {
		return StaleRevalidateFresh
	}
	if w.StaleUntil.After(w.FreshUntil) && now.Before(w.StaleUntil) {
		return StaleRevalidateStale
	}
	return StaleRevalidateExpired
}

// CanServe reports whether the entry may be returned without blocking.
func (w StaleRevalidateWindow) CanServe(now time.Time) bool {
	return w.State(now) != StaleRevalidateExpired
}

// CanServeStale reports whether stale bytes may be returned while refreshing.
func (w StaleRevalidateWindow) CanServeStale(now time.Time) bool {
	return w.State(now) == StaleRevalidateStale
}

// NeedsRefresh reports whether the entry should be refreshed before future use.
func (w StaleRevalidateWindow) NeedsRefresh(now time.Time) bool {
	return w.State(now) != StaleRevalidateFresh
}

// StaleRevalidateDecision summarizes how a caller should treat a cached entry.
type StaleRevalidateDecision struct {
	State             StaleRevalidateState
	Serve             bool
	ServeStale        bool
	Refresh           bool
	BackgroundRefresh bool
	RefreshMarker     StaleRevalidateRefreshMarker
}

// StaleRevalidateRefreshMarker records an in-flight background refresh lease.
type StaleRevalidateRefreshMarker struct {
	MarkedAt  time.Time
	ExpiresAt time.Time
}

// Active reports whether the marker still suppresses another background refresh.
func (m StaleRevalidateRefreshMarker) Active(now time.Time) bool {
	return !m.ExpiresAt.IsZero() && now.Before(m.ExpiresAt)
}

// BuildStaleRevalidateRefreshMarkerKey returns a backend-neutral marker key for a cache key.
func BuildStaleRevalidateRefreshMarkerKey(cacheKey string) string {
	return staleRevalidateRefreshMarkerPrefix + keySeparator + cacheKey
}
