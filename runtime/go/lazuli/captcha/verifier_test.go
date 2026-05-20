package captcha

import (
	"context"
	"testing"
)

func TestNoopVerifierAllowsAllTokens(t *testing.T) {
	v := NoopVerifier{}

	got, err := v.Verify(context.Background(), "token", "203.0.113.10")
	if err != nil {
		t.Fatalf("Verify returned error: %v", err)
	}
	if !got.Passed {
		t.Fatal("Verify returned Passed=false")
	}
	if got.Score != 0.0 {
		t.Fatalf("Score = %v, want 0.0", got.Score)
	}
	if len(got.Reasons) != 0 {
		t.Fatalf("Reasons = %v, want empty", got.Reasons)
	}
	if err := v.Close(); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}
}
