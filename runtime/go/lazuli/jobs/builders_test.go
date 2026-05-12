package jobs

import (
	"testing"
	"time"
)

func TestRetryBuilderDefaultSpec(t *testing.T) {
	t.Parallel()
	spec := Retry(3).Spec()
	if spec.Count != 3 {
		t.Fatalf("Count = %d, want 3", spec.Count)
	}
	if spec.Backoff != BackoffExponential {
		t.Fatalf("Backoff = %q, want %q", spec.Backoff, BackoffExponential)
	}
}

func TestRetryBuilderWithBackoff(t *testing.T) {
	t.Parallel()
	base := Retry(2)
	spec := base.WithBackoff(BackoffFixed).Spec()
	if spec.Count != 2 {
		t.Fatalf("Count = %d, want 2", spec.Count)
	}
	if spec.Backoff != BackoffFixed {
		t.Fatalf("Backoff = %q, want %q", spec.Backoff, BackoffFixed)
	}
	if base.Backoff != BackoffExponential {
		t.Fatalf("base Backoff = %q, want unchanged %q", base.Backoff, BackoffExponential)
	}
}

func TestIdempotentAndWithTimeout(t *testing.T) {
	t.Parallel()
	idempotency := Idempotent("payload.batch_id")
	if idempotency == nil {
		t.Fatal("Idempotent returned nil")
	}
	if idempotency.Path != "payload.batch_id" {
		t.Fatalf("Path = %q, want %q", idempotency.Path, "payload.batch_id")
	}
	if timeout := WithTimeout(30 * time.Second); timeout != "30s" {
		t.Fatalf("WithTimeout = %q, want %q", timeout, "30s")
	}
}
