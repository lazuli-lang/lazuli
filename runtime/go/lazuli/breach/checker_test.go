package breach

import (
	"context"
	"testing"
)

func TestNoopCheckerTreatsPasswordAsClean(t *testing.T) {
	var checker Checker = NoopChecker{}

	count, err := checker.PasswordBreached(context.Background(), "not-transmitted")
	if err != nil {
		t.Fatalf("PasswordBreached returned error: %v", err)
	}
	if count != 0 {
		t.Fatalf("PasswordBreached count = %d, want 0", count)
	}
	if err := checker.Close(); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}
}
