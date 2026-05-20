package reputation

import (
	"context"
	"testing"
)

func TestNeutralScorerReturnsZeroRisk(t *testing.T) {
	s := NeutralScorer{}

	got, err := s.Score(context.Background(), "203.0.113.10")
	if err != nil {
		t.Fatalf("Score returned error: %v", err)
	}
	if got.Risk != 0.0 {
		t.Fatalf("Risk = %v, want 0.0", got.Risk)
	}
	if got.ScoredAt.IsZero() {
		t.Fatal("ScoredAt is zero")
	}
	if err := s.Close(); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}
}
