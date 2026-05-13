package cache

import (
	"errors"
	"testing"
	"time"
)

func TestStaleRevalidatePolicyValidate(t *testing.T) {
	err := StaleRevalidatePolicy{
		FreshFor:         -time.Minute,
		StaleFor:         time.Second,
		RefreshMarkerTTL: -time.Second,
	}.Validate()
	if !errors.Is(err, ErrInvalidStaleRevalidatePolicy) {
		t.Fatalf("Validate() error = %v, want %v", err, ErrInvalidStaleRevalidatePolicy)
	}

	err = StaleRevalidatePolicy{
		FreshFor:         5 * time.Minute,
		StaleFor:         time.Minute,
		RefreshMarkerTTL: 10 * time.Second,
	}.Validate()
	if err != nil {
		t.Fatalf("Validate(valid) error = %v", err)
	}
}

func TestStaleRevalidateWindowStates(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	window := StaleRevalidatePolicy{
		FreshFor: 10 * time.Second,
		StaleFor: 20 * time.Second,
	}.Window(storedAt)

	if !window.StoredAt.Equal(storedAt) {
		t.Fatalf("StoredAt = %v, want %v", window.StoredAt, storedAt)
	}
	if want := storedAt.Add(10 * time.Second); !window.FreshUntil.Equal(want) {
		t.Fatalf("FreshUntil = %v, want %v", window.FreshUntil, want)
	}
	if want := storedAt.Add(30 * time.Second); !window.StaleUntil.Equal(want) {
		t.Fatalf("StaleUntil = %v, want %v", window.StaleUntil, want)
	}

	tests := []struct {
		name       string
		at         time.Time
		state      StaleRevalidateState
		serve      bool
		serveStale bool
		refresh    bool
	}{
		{
			name:    "fresh",
			at:      storedAt.Add(9 * time.Second),
			state:   StaleRevalidateFresh,
			serve:   true,
			refresh: false,
		},
		{
			name:       "stale at fresh boundary",
			at:         storedAt.Add(10 * time.Second),
			state:      StaleRevalidateStale,
			serve:      true,
			serveStale: true,
			refresh:    true,
		},
		{
			name:       "stale before stale boundary",
			at:         storedAt.Add(29 * time.Second),
			state:      StaleRevalidateStale,
			serve:      true,
			serveStale: true,
			refresh:    true,
		},
		{
			name:    "expired at stale boundary",
			at:      storedAt.Add(30 * time.Second),
			state:   StaleRevalidateExpired,
			serve:   false,
			refresh: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if state := window.State(tt.at); state != tt.state {
				t.Fatalf("State() = %v, want %v", state, tt.state)
			}
			if got := window.CanServe(tt.at); got != tt.serve {
				t.Fatalf("CanServe() = %v, want %v", got, tt.serve)
			}
			if got := window.CanServeStale(tt.at); got != tt.serveStale {
				t.Fatalf("CanServeStale() = %v, want %v", got, tt.serveStale)
			}
			if got := window.NeedsRefresh(tt.at); got != tt.refresh {
				t.Fatalf("NeedsRefresh() = %v, want %v", got, tt.refresh)
			}
		})
	}
}

func TestStaleRevalidateWindowWithoutStaleExpiresAtFreshBoundary(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	window := StaleRevalidatePolicy{FreshFor: 10 * time.Second}.Window(storedAt)

	if state := window.State(storedAt.Add(9 * time.Second)); state != StaleRevalidateFresh {
		t.Fatalf("State(before fresh boundary) = %v, want %v", state, StaleRevalidateFresh)
	}
	if state := window.State(storedAt.Add(10 * time.Second)); state != StaleRevalidateExpired {
		t.Fatalf("State(at fresh boundary) = %v, want %v", state, StaleRevalidateExpired)
	}
}

func TestStaleRevalidateWindowNegativeFreshnessNeverExpires(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	window := StaleRevalidatePolicy{FreshFor: -time.Second}.Window(storedAt)

	if !window.FreshUntil.IsZero() {
		t.Fatalf("FreshUntil = %v, want zero", window.FreshUntil)
	}
	if !window.StaleUntil.IsZero() {
		t.Fatalf("StaleUntil = %v, want zero", window.StaleUntil)
	}

	later := storedAt.Add(365 * 24 * time.Hour)
	if state := window.State(later); state != StaleRevalidateFresh {
		t.Fatalf("State(negative FreshFor) = %v, want %v", state, StaleRevalidateFresh)
	}
	if !window.CanServe(later) {
		t.Fatal("CanServe(negative FreshFor) = false, want true")
	}
	if window.NeedsRefresh(later) {
		t.Fatal("NeedsRefresh(negative FreshFor) = true, want false")
	}
}

func TestStaleRevalidateDecisionStartsBackgroundRefresh(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	policy := StaleRevalidatePolicy{
		FreshFor:         10 * time.Second,
		StaleFor:         20 * time.Second,
		RefreshMarkerTTL: 5 * time.Second,
	}
	at := storedAt.Add(12 * time.Second)

	decision := policy.Decide(policy.Window(storedAt), at, StaleRevalidateRefreshMarker{})
	if decision.State != StaleRevalidateStale {
		t.Fatalf("State = %v, want %v", decision.State, StaleRevalidateStale)
	}
	if !decision.Serve || !decision.ServeStale || !decision.Refresh || !decision.BackgroundRefresh {
		t.Fatalf("decision = %+v, want serve stale with background refresh", decision)
	}
	if !decision.RefreshMarker.MarkedAt.Equal(at) {
		t.Fatalf("RefreshMarker.MarkedAt = %v, want %v", decision.RefreshMarker.MarkedAt, at)
	}
	if want := at.Add(5 * time.Second); !decision.RefreshMarker.ExpiresAt.Equal(want) {
		t.Fatalf("RefreshMarker.ExpiresAt = %v, want %v", decision.RefreshMarker.ExpiresAt, want)
	}
	if !decision.RefreshMarker.Active(at.Add(5*time.Second - time.Nanosecond)) {
		t.Fatal("RefreshMarker.Active(before expiry) = false, want true")
	}
	if decision.RefreshMarker.Active(at.Add(5 * time.Second)) {
		t.Fatal("RefreshMarker.Active(at expiry) = true, want false")
	}
}

func TestStaleRevalidateDecisionSuppressesActiveMarker(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	policy := StaleRevalidatePolicy{
		FreshFor: 10 * time.Second,
		StaleFor: 20 * time.Second,
	}
	at := storedAt.Add(12 * time.Second)
	activeMarker := StaleRevalidateRefreshMarker{
		MarkedAt:  at.Add(-time.Second),
		ExpiresAt: at.Add(time.Second),
	}

	decision := policy.Decide(policy.Window(storedAt), at, activeMarker)
	if !decision.ServeStale || !decision.Refresh {
		t.Fatalf("decision = %+v, want stale entry to be served and refreshed later", decision)
	}
	if decision.BackgroundRefresh {
		t.Fatalf("BackgroundRefresh = true with active marker")
	}
	if !decision.RefreshMarker.ExpiresAt.IsZero() {
		t.Fatalf("RefreshMarker = %+v, want zero marker", decision.RefreshMarker)
	}
}

func TestStaleRevalidateDecisionExpiredRequiresBlockingRefresh(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	policy := StaleRevalidatePolicy{
		FreshFor: 10 * time.Second,
		StaleFor: 20 * time.Second,
	}
	at := storedAt.Add(30 * time.Second)

	decision := policy.Decide(policy.Window(storedAt), at, StaleRevalidateRefreshMarker{})
	if decision.State != StaleRevalidateExpired {
		t.Fatalf("State = %v, want %v", decision.State, StaleRevalidateExpired)
	}
	if decision.Serve || decision.ServeStale || decision.BackgroundRefresh || !decision.Refresh {
		t.Fatalf("decision = %+v, want blocking refresh without serving stale", decision)
	}
}

func TestStaleRevalidateRefreshMarkerUsesRemainingStaleWindow(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	window := StaleRevalidatePolicy{
		FreshFor: 10 * time.Second,
		StaleFor: 20 * time.Second,
	}.Window(storedAt)
	at := storedAt.Add(12 * time.Second)

	marker := StaleRevalidatePolicy{}.RefreshMarker(window, at)
	if want := storedAt.Add(30 * time.Second); !marker.ExpiresAt.Equal(want) {
		t.Fatalf("default marker ExpiresAt = %v, want %v", marker.ExpiresAt, want)
	}

	marker = StaleRevalidatePolicy{RefreshMarkerTTL: time.Minute}.RefreshMarker(window, storedAt.Add(25*time.Second))
	if want := storedAt.Add(30 * time.Second); !marker.ExpiresAt.Equal(want) {
		t.Fatalf("capped marker ExpiresAt = %v, want %v", marker.ExpiresAt, want)
	}
}

func TestBuildStaleRevalidateRefreshMarkerKey(t *testing.T) {
	key := "customer.query.list|tenant-1|abc123"
	got := BuildStaleRevalidateRefreshMarkerKey(key)
	want := "refresh|customer.query.list|tenant-1|abc123"
	if got != want {
		t.Fatalf("BuildStaleRevalidateRefreshMarkerKey() = %q, want %q", got, want)
	}
}
