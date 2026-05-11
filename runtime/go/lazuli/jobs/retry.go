// Package jobs — retry helpers. Adapter-specific backoff (jitter,
// cap, dead-letter routing) lives in `@runtime/<queue>`; this file ships
// the closed-catalog backoff strategy resolution and the helpers
// generated code uses.
//
// Phase L Tier 3 / row 33 stubs.
package jobs

import (
	"math"
	"time"
)

// NextDelay returns the wait duration before attempt `n` (zero-based)
// for a given backoff strategy. Adapters compose this with their own
// jitter caps; the language pins only the closed-catalog policy.
func NextDelay(policy RetryPolicy, attempt uint32) time.Duration {
	if attempt == 0 {
		return 0
	}
	switch policy.Backoff {
	case BackoffFixed:
		return baseDelay
	case BackoffExponential:
		// Doubles each attempt, capped at 5 minutes.
		power := math.Pow(2, float64(attempt-1))
		delay := time.Duration(power) * baseDelay
		if delay > maxDelay {
			return maxDelay
		}
		return delay
	default:
		return baseDelay
	}
}

// ShouldRetry reports whether the dispatcher should re-fire on
// attempt `n`. Returns false once `n >= policy.Count`.
func ShouldRetry(policy *RetryPolicy, attempt uint32) bool {
	if policy == nil {
		return false
	}
	return attempt < policy.Count
}

const (
	baseDelay = 5 * time.Second
	maxDelay  = 5 * time.Minute
)
