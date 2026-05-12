package jobs

import "time"

// Retry starts a RetryPolicy with the given attempt count.
func Retry(count int) RetryBuilder {
	return RetryBuilder{Count: uint32(count), Backoff: BackoffExponential}
}

type RetryBuilder struct {
	Count   uint32
	Backoff BackoffStrategy
}

func (b RetryBuilder) WithBackoff(b2 BackoffStrategy) RetryBuilder {
	b.Backoff = b2
	return b
}

func (b RetryBuilder) Spec() RetryPolicy {
	return RetryPolicy{Count: b.Count, Backoff: b.Backoff}
}

// Idempotent builds an IdempotencyKeySpec from a path.
func Idempotent(path string) *IdempotencyKeySpec {
	return &IdempotencyKeySpec{Path: path}
}

// WithTimeout returns the given duration as a Go time.Duration helper string ("30s", "10m", etc).
// Use for JobContract.Timeout fields.
func WithTimeout(d time.Duration) string {
	return d.String()
}
