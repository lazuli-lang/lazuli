package auth

import "time"

// RenewalPolicy controls sliding expiry renewal for persisted sessions.
//
// The helpers are intentionally pure so adapters can decide whether to
// update a stored session without requiring a live database handle.
type RenewalPolicy struct {
	// Refresh enables sliding renewal. Leave false for fixed-expiry sessions.
	Refresh bool
	// RenewBefore limits renewal to sessions expiring within this duration.
	// A non-positive duration renews every unexpired refresh-enabled session.
	RenewBefore time.Duration
	// IssuedAt anchors MaxAbsoluteLifetime. Leave zero when no absolute cap
	// is enforced or when the session creation time is unavailable.
	IssuedAt time.Time
	// MaxAbsoluteLifetime caps renewed expiries at IssuedAt plus this duration.
	// Non-positive values disable the cap.
	MaxAbsoluteLifetime time.Duration
}

// ShouldRenew reports whether an unexpired session should receive a sliding
// expiry renewal under policy.
func ShouldRenew(now, expiresAt time.Time, policy RenewalPolicy) bool {
	if !policy.Refresh || !expiresAt.After(now) {
		return false
	}
	if maxExpiresAt := renewalMaxExpiresAt(policy); !maxExpiresAt.IsZero() {
		if !now.Before(maxExpiresAt) || !expiresAt.Before(maxExpiresAt) {
			return false
		}
	}
	if policy.RenewBefore <= 0 {
		return true
	}
	return !expiresAt.After(now.Add(policy.RenewBefore))
}

// RenewedExpiry returns the expiry after applying a sliding session TTL.
//
// If policy does not call for renewal, ttl is non-positive, the session is
// expired, or the renewed expiry would not extend the session, RenewedExpiry
// returns currentExpiry unchanged. When a max absolute lifetime is configured,
// the returned expiry is capped at the absolute limit.
func RenewedExpiry(now, currentExpiry time.Time, ttl time.Duration, policy RenewalPolicy) time.Time {
	if ttl <= 0 || !ShouldRenew(now, currentExpiry, policy) {
		return currentExpiry
	}

	renewedExpiry := now.Add(ttl)
	if maxExpiresAt := renewalMaxExpiresAt(policy); !maxExpiresAt.IsZero() && renewedExpiry.After(maxExpiresAt) {
		renewedExpiry = maxExpiresAt
	}
	if !renewedExpiry.After(currentExpiry) {
		return currentExpiry
	}
	return renewedExpiry
}

func renewalMaxExpiresAt(policy RenewalPolicy) time.Time {
	if policy.IssuedAt.IsZero() || policy.MaxAbsoluteLifetime <= 0 {
		return time.Time{}
	}
	return policy.IssuedAt.Add(policy.MaxAbsoluteLifetime)
}
