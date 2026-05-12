package jobs

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestMemoryProgressStoreUpdateGetList(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := NewMemoryProgressStore()
	store.Clock = func() time.Time { return now }

	metadata := map[string]string{"batch": "b1"}
	first, err := store.Update(context.Background(), Progress{
		JobID:    "job-2",
		Percent:  25,
		Message:  "importing",
		Metadata: metadata,
	})
	if err != nil {
		t.Fatalf("Update first: %v", err)
	}
	if first.State != ProgressStateRunning {
		t.Fatalf("State = %q, want default %q", first.State, ProgressStateRunning)
	}
	if first.CreatedAt != now || first.UpdatedAt != now {
		t.Fatalf("timestamps = (%v, %v), want %v", first.CreatedAt, first.UpdatedAt, now)
	}
	if !first.FinishedAt.IsZero() {
		t.Fatalf("FinishedAt = %v, want zero", first.FinishedAt)
	}

	metadata["batch"] = "mutated"
	got, ok, err := store.Get(context.Background(), "job-2")
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if !ok {
		t.Fatal("Get returned ok=false")
	}
	if got.Metadata["batch"] != "b1" {
		t.Fatalf("Metadata was not cloned: %+v", got.Metadata)
	}
	got.Metadata["batch"] = "changed-through-snapshot"
	gotAgain, ok, err := store.Get(context.Background(), "job-2")
	if err != nil {
		t.Fatalf("Get again: %v", err)
	}
	if !ok {
		t.Fatal("Get again returned ok=false")
	}
	if gotAgain.Metadata["batch"] != "b1" {
		t.Fatalf("snapshot mutation leaked into store: %+v", gotAgain.Metadata)
	}

	later := now.Add(2 * time.Second)
	now = later
	done, err := store.Update(context.Background(), Progress{
		JobID:    "job-2",
		State:    ProgressStateSucceeded,
		Percent:  100,
		Message:  "done",
		Metadata: map[string]string{"rows": "10"},
	})
	if err != nil {
		t.Fatalf("Update terminal: %v", err)
	}
	if done.CreatedAt != first.CreatedAt {
		t.Fatalf("CreatedAt = %v, want preserved %v", done.CreatedAt, first.CreatedAt)
	}
	if done.UpdatedAt != later || done.FinishedAt != later {
		t.Fatalf("terminal timestamps = (%v, %v), want %v", done.UpdatedAt, done.FinishedAt, later)
	}
	if !done.Terminal() {
		t.Fatal("Terminal returned false for succeeded progress")
	}

	now = now.Add(time.Second)
	if _, err := store.Update(context.Background(), Progress{
		JobID:   "job-1",
		State:   ProgressStatePending,
		Percent: 0,
	}); err != nil {
		t.Fatalf("Update second job: %v", err)
	}

	list, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(list) != 2 {
		t.Fatalf("List len = %d, want 2", len(list))
	}
	if list[0].JobID != "job-1" || list[1].JobID != "job-2" {
		t.Fatalf("List order = [%q, %q], want sorted by job id", list[0].JobID, list[1].JobID)
	}
	list[1].Metadata["rows"] = "mutated"
	gotDone, ok, err := store.Get(context.Background(), "job-2")
	if err != nil {
		t.Fatalf("Get done: %v", err)
	}
	if !ok {
		t.Fatal("Get done returned ok=false")
	}
	if gotDone.Metadata["rows"] != "10" {
		t.Fatalf("list mutation leaked into store: %+v", gotDone.Metadata)
	}
}

func TestMemoryProgressStoreRejectsInvalidUpdates(t *testing.T) {
	t.Parallel()

	store := NewMemoryProgressStore()
	for _, progress := range []Progress{
		{Percent: 50},
		{JobID: "job-1", Percent: -1},
		{JobID: "job-1", Percent: 101},
		{JobID: "job-1", State: ProgressState("paused"), Percent: 50},
	} {
		_, err := store.Update(context.Background(), progress)
		if !errors.Is(err, ErrProgressInvalid) {
			t.Fatalf("Update(%+v) error = %v, want ErrProgressInvalid", progress, err)
		}
	}

	if _, _, err := store.Get(context.Background(), ""); !errors.Is(err, ErrProgressInvalid) {
		t.Fatalf("Get empty job id error = %v, want ErrProgressInvalid", err)
	}
}

func TestMemoryProgressStoreRejectsUpdatesAfterTerminalState(t *testing.T) {
	t.Parallel()

	store := NewMemoryProgressStore()
	if _, err := store.Update(context.Background(), Progress{
		JobID:   "job-1",
		State:   ProgressStateFailed,
		Percent: 80,
	}); err != nil {
		t.Fatalf("Update terminal: %v", err)
	}

	_, err := store.Update(context.Background(), Progress{
		JobID:   "job-1",
		State:   ProgressStateRunning,
		Percent: 90,
	})
	if !errors.Is(err, ErrProgressTerminal) {
		t.Fatalf("Update after terminal error = %v, want ErrProgressTerminal", err)
	}
}

func TestProgressStateTerminal(t *testing.T) {
	t.Parallel()

	tests := map[ProgressState]bool{
		ProgressStatePending:   false,
		ProgressStateRunning:   false,
		ProgressStateSucceeded: true,
		ProgressStateFailed:    true,
		ProgressStateCanceled:  true,
	}
	for state, want := range tests {
		if got := state.Terminal(); got != want {
			t.Fatalf("%q Terminal() = %v, want %v", state, got, want)
		}
	}
}

func TestMemoryProgressStoreZeroValueAndContext(t *testing.T) {
	t.Parallel()

	var store MemoryProgressStore
	if _, err := store.Update(context.Background(), Progress{
		JobID:   "job-1",
		Percent: 1,
	}); err != nil {
		t.Fatalf("zero-value Update: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, err := store.List(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("List canceled error = %v, want context.Canceled", err)
	}
}
