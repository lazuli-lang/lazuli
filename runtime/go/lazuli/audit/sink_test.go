package audit

import (
	"context"
	"testing"
	"time"
)

func TestNoopSinkShip(t *testing.T) {
	s := NoopSink{}
	err := s.Ship(context.Background(), Row{
		ID:         1,
		Command:    "test.x",
		Actor:      "user:42",
		Decision:   "allowed",
		RecordedAt: time.Now(),
	})
	if err != nil {
		t.Fatalf("NoopSink.Ship should never error; got %v", err)
	}
}

func TestNoopSinkClose(t *testing.T) {
	if err := (NoopSink{}).Close(); err != nil {
		t.Fatalf("NoopSink.Close should never error; got %v", err)
	}
}
