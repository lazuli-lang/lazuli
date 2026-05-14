package lifecycle

import (
	"context"
	"testing"
)

type publicationStatus string

const (
	scheduled  publicationStatus = "scheduled"
	publishing publicationStatus = "publishing"
	published  publicationStatus = "published"
	failed     publicationStatus = "failed"
	cancelled  publicationStatus = "cancelled"
)

func mkMachine() *Machine[publicationStatus] {
	return New[publicationStatus](scheduled, []Transition[publicationStatus]{
		{Name: "begin_publishing", From: []string{"scheduled"}, To: "publishing"},
		{Name: "mark_published", From: []string{"publishing"}, To: "published"},
		{Name: "mark_failed", From: []string{"publishing"}, To: "failed"},
		{Name: "cancel", From: []string{"scheduled", "publishing"}, To: "cancelled"},
	})
}

func TestApplyValidTransition(t *testing.T) {
	m := mkMachine()
	got, err := m.Apply(context.Background(), scheduled, "begin_publishing")
	if err != nil {
		t.Fatalf("unexpected: %v", err)
	}
	if got != publishing {
		t.Errorf("got %q, want %q", got, publishing)
	}
}

func TestApplyInvalidFromState(t *testing.T) {
	m := mkMachine()
	_, err := m.Apply(context.Background(), published, "mark_published")
	if err == nil {
		t.Fatal("expected error for invalid transition")
	}
}

func TestApplyMultiSourceTransition(t *testing.T) {
	m := mkMachine()
	for _, from := range []publicationStatus{scheduled, publishing} {
		got, err := m.Apply(context.Background(), from, "cancel")
		if err != nil {
			t.Fatalf("cancel from %q: %v", from, err)
		}
		if got != cancelled {
			t.Errorf("got %q, want %q", got, cancelled)
		}
	}
}

func TestApplyUnknownTransition(t *testing.T) {
	m := mkMachine()
	_, err := m.Apply(context.Background(), scheduled, "teleport")
	if err == nil {
		t.Fatal("expected error for unknown transition")
	}
}
