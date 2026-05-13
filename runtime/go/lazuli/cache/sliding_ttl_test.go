package cache

import (
	"testing"
	"time"
)

func TestSlidingTTLPolicyFromQueryCacheConfig(t *testing.T) {
	config := QueryCacheConfig{
		TTL:           30 * time.Minute,
		SlidingTTL:    5 * time.Minute,
		NegativeCache: true,
	}

	policy := NewSlidingTTLPolicy(config)
	if policy.TTL != config.TTL || policy.SlidingTTL != config.SlidingTTL {
		t.Fatalf("NewSlidingTTLPolicy() = %#v, want TTL and SlidingTTL from config", policy)
	}
	if !policy.Enabled() {
		t.Fatal("Enabled() = false, want true")
	}
}

func TestSlidingTTLPolicyDisabledWithoutPositiveWindows(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	entry := SlidingTTLEntry{
		StoredAt:  now.Add(-time.Minute),
		ExpiresAt: now.Add(time.Minute),
	}

	tests := []struct {
		name   string
		policy SlidingTTLPolicy
	}{
		{name: "zero ttl", policy: SlidingTTLPolicy{SlidingTTL: time.Minute}},
		{name: "zero sliding ttl", policy: SlidingTTLPolicy{TTL: time.Minute}},
		{name: "negative ttl", policy: SlidingTTLPolicy{TTL: -time.Minute, SlidingTTL: time.Minute}},
		{name: "negative sliding ttl", policy: SlidingTTLPolicy{TTL: time.Minute, SlidingTTL: -time.Minute}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.policy.Enabled() {
				t.Fatal("Enabled() = true, want false")
			}
			if next := tt.policy.NextExpiresAt(now, entry.StoredAt); !next.IsZero() {
				t.Fatalf("NextExpiresAt() = %v, want zero", next)
			}
			if tt.policy.ShouldTouch(now, entry) {
				t.Fatal("ShouldTouch() = true, want false")
			}
		})
	}
}

func TestSlidingTTLMaxExpiresAt(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	if got := SlidingTTLMaxExpiresAt(storedAt, time.Hour); !got.Equal(storedAt.Add(time.Hour)) {
		t.Fatalf("SlidingTTLMaxExpiresAt() = %v, want %v", got, storedAt.Add(time.Hour))
	}
	if got := SlidingTTLMaxExpiresAt(time.Time{}, time.Hour); !got.IsZero() {
		t.Fatalf("SlidingTTLMaxExpiresAt(zero storedAt) = %v, want zero", got)
	}
	if got := SlidingTTLMaxExpiresAt(storedAt, 0); !got.IsZero() {
		t.Fatalf("SlidingTTLMaxExpiresAt(zero lifetime) = %v, want zero", got)
	}
}

func TestNextSlidingTTLExpiresAtUsesWindowAndCapsAtMaxLifetime(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	policy := SlidingTTLPolicy{
		TTL:        30 * time.Minute,
		SlidingTTL: 10 * time.Minute,
	}

	now := storedAt.Add(5 * time.Minute)
	if got := NextSlidingTTLExpiresAt(now, storedAt, policy); !got.Equal(now.Add(10 * time.Minute)) {
		t.Fatalf("NextSlidingTTLExpiresAt() = %v, want %v", got, now.Add(10*time.Minute))
	}

	nearCap := storedAt.Add(25 * time.Minute)
	maxExpiresAt := storedAt.Add(30 * time.Minute)
	if got := policy.NextExpiresAt(nearCap, storedAt); !got.Equal(maxExpiresAt) {
		t.Fatalf("NextExpiresAt(near cap) = %v, want capped %v", got, maxExpiresAt)
	}

	if got := policy.NextExpiresAt(now, time.Time{}); !got.IsZero() {
		t.Fatalf("NextExpiresAt(zero storedAt) = %v, want zero", got)
	}
}

func TestDecideSlidingTTLTouchesOnlyWhenExpiryExtends(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	now := storedAt.Add(5 * time.Minute)
	policy := SlidingTTLPolicy{
		TTL:        30 * time.Minute,
		SlidingTTL: 10 * time.Minute,
	}

	entry := SlidingTTLEntry{
		StoredAt:  storedAt,
		ExpiresAt: now.Add(2 * time.Minute),
	}
	decision := policy.Decide(now, entry)
	wantNext := now.Add(10 * time.Minute)
	if !decision.Touch {
		t.Fatal("Touch = false, want true")
	}
	if decision.Expired {
		t.Fatal("Expired = true, want false")
	}
	if !decision.NextExpiresAt.Equal(wantNext) {
		t.Fatalf("NextExpiresAt = %v, want %v", decision.NextExpiresAt, wantNext)
	}
	if !decision.MaxExpiresAt.Equal(storedAt.Add(30 * time.Minute)) {
		t.Fatalf("MaxExpiresAt = %v, want %v", decision.MaxExpiresAt, storedAt.Add(30*time.Minute))
	}
	if !ShouldTouchSlidingTTL(now, entry, policy) {
		t.Fatal("ShouldTouchSlidingTTL() = false, want true")
	}

	entry.ExpiresAt = now.Add(20 * time.Minute)
	if decision := DecideSlidingTTL(now, entry, policy); decision.Touch {
		t.Fatalf("Touch = true, want false when next expiry would not extend: %#v", decision)
	}
}

func TestDecideSlidingTTLTouchesUpToAbsoluteCap(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	now := storedAt.Add(25 * time.Minute)
	maxExpiresAt := storedAt.Add(30 * time.Minute)
	policy := SlidingTTLPolicy{
		TTL:        30 * time.Minute,
		SlidingTTL: 10 * time.Minute,
	}
	entry := SlidingTTLEntry{
		StoredAt:  storedAt,
		ExpiresAt: now.Add(time.Minute),
	}

	decision := DecideSlidingTTL(now, entry, policy)
	if !decision.Touch {
		t.Fatal("Touch = false, want true before cap")
	}
	if !decision.NextExpiresAt.Equal(maxExpiresAt) {
		t.Fatalf("NextExpiresAt = %v, want cap %v", decision.NextExpiresAt, maxExpiresAt)
	}

	entry.ExpiresAt = maxExpiresAt
	if decision := DecideSlidingTTL(now, entry, policy); decision.Touch {
		t.Fatalf("Touch = true, want false once expiry is already capped: %#v", decision)
	}
}

func TestDecideSlidingTTLDoesNotTouchExpiredOrNonExpiringEntries(t *testing.T) {
	storedAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	now := storedAt.Add(5 * time.Minute)
	policy := SlidingTTLPolicy{
		TTL:        30 * time.Minute,
		SlidingTTL: 10 * time.Minute,
	}

	for _, expiresAt := range []time.Time{now, now.Add(-time.Nanosecond)} {
		decision := DecideSlidingTTL(now, SlidingTTLEntry{StoredAt: storedAt, ExpiresAt: expiresAt}, policy)
		if !decision.Expired {
			t.Fatalf("Expired = false for ExpiresAt %v, want true", expiresAt)
		}
		if decision.Touch {
			t.Fatalf("Touch = true for expired entry %v, want false", expiresAt)
		}
	}

	decision := DecideSlidingTTL(now, SlidingTTLEntry{StoredAt: storedAt}, policy)
	if decision.Expired {
		t.Fatal("Expired = true for non-expiring entry, want false")
	}
	if decision.Touch {
		t.Fatal("Touch = true for non-expiring entry, want false")
	}
}
