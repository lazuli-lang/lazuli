package auth

import (
	"testing"
	"time"
)

func TestRenewalPolicyDisabled(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	expiresAt := now.Add(5 * time.Minute)
	policy := RenewalPolicy{
		Refresh:     false,
		RenewBefore: 10 * time.Minute,
	}

	if ShouldRenew(now, expiresAt, policy) {
		t.Fatalf("ShouldRenew = true, want false for non-refresh sessions")
	}
	if renewed := RenewedExpiry(now, expiresAt, time.Hour, policy); !renewed.Equal(expiresAt) {
		t.Fatalf("RenewedExpiry = %v, want unchanged %v", renewed, expiresAt)
	}
}

func TestShouldRenewThreshold(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	policy := RenewalPolicy{
		Refresh:     true,
		RenewBefore: 15 * time.Minute,
	}

	tests := []struct {
		name      string
		expiresAt time.Time
		want      bool
	}{
		{
			name:      "outside threshold",
			expiresAt: now.Add(15*time.Minute + time.Nanosecond),
			want:      false,
		},
		{
			name:      "at threshold",
			expiresAt: now.Add(15 * time.Minute),
			want:      true,
		},
		{
			name:      "inside threshold",
			expiresAt: now.Add(time.Minute),
			want:      true,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := ShouldRenew(now, tt.expiresAt, policy); got != tt.want {
				t.Fatalf("ShouldRenew = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestRenewedExpiryExtendsByTTL(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	currentExpiry := now.Add(2 * time.Minute)
	policy := RenewalPolicy{
		Refresh:     true,
		RenewBefore: 5 * time.Minute,
	}

	got := RenewedExpiry(now, currentExpiry, time.Hour, policy)
	want := now.Add(time.Hour)
	if !got.Equal(want) {
		t.Fatalf("RenewedExpiry = %v, want %v", got, want)
	}
}

func TestRenewedExpiryDoesNotShortenSession(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	currentExpiry := now.Add(30 * time.Minute)
	policy := RenewalPolicy{
		Refresh:     true,
		RenewBefore: time.Hour,
	}

	got := RenewedExpiry(now, currentExpiry, 10*time.Minute, policy)
	if !got.Equal(currentExpiry) {
		t.Fatalf("RenewedExpiry = %v, want unchanged %v", got, currentExpiry)
	}
}

func TestRenewedExpiryCapsAtMaxAbsoluteLifetime(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	issuedAt := now.Add(-23 * time.Hour)
	maxExpiresAt := issuedAt.Add(24 * time.Hour)
	currentExpiry := now.Add(5 * time.Minute)
	policy := RenewalPolicy{
		Refresh:             true,
		RenewBefore:         10 * time.Minute,
		IssuedAt:            issuedAt,
		MaxAbsoluteLifetime: 24 * time.Hour,
	}

	if !ShouldRenew(now, currentExpiry, policy) {
		t.Fatalf("ShouldRenew = false, want true before max absolute lifetime")
	}
	if got := RenewedExpiry(now, currentExpiry, 8*time.Hour, policy); !got.Equal(maxExpiresAt) {
		t.Fatalf("RenewedExpiry = %v, want capped expiry %v", got, maxExpiresAt)
	}

	if ShouldRenew(now, maxExpiresAt, policy) {
		t.Fatalf("ShouldRenew = true, want false once current expiry reaches max absolute lifetime")
	}
}

func TestExpiredSessionsDoNotRenew(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	policy := RenewalPolicy{
		Refresh:     true,
		RenewBefore: time.Hour,
	}

	for _, expiresAt := range []time.Time{now, now.Add(-time.Nanosecond)} {
		if ShouldRenew(now, expiresAt, policy) {
			t.Fatalf("ShouldRenew(%v) = true, want false for expired session", expiresAt)
		}
		if renewed := RenewedExpiry(now, expiresAt, time.Hour, policy); !renewed.Equal(expiresAt) {
			t.Fatalf("RenewedExpiry = %v, want unchanged expired expiry %v", renewed, expiresAt)
		}
	}
}

func TestRenewedExpiryRequiresPositiveTTL(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	currentExpiry := now.Add(5 * time.Minute)
	policy := RenewalPolicy{
		Refresh:     true,
		RenewBefore: 10 * time.Minute,
	}

	for _, ttl := range []time.Duration{0, -time.Second} {
		if renewed := RenewedExpiry(now, currentExpiry, ttl, policy); !renewed.Equal(currentExpiry) {
			t.Fatalf("RenewedExpiry ttl=%v = %v, want unchanged %v", ttl, renewed, currentExpiry)
		}
	}
}
