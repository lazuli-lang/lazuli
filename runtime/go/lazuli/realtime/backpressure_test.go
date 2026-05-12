package realtime

import (
	"sync"
	"testing"
	"time"
)

func TestConnectionStateTracker(t *testing.T) {
	t.Parallel()

	var zero ConnectionStateTracker
	if got := zero.State(); got != ConnectionStateDisconnected {
		t.Fatalf("zero State = %q, want %q", got, ConnectionStateDisconnected)
	}

	tracker := NewConnectionStateTracker(ConnectionStateConnected)
	if got := tracker.State(); got != ConnectionStateConnected {
		t.Fatalf("State = %q, want %q", got, ConnectionStateConnected)
	}
	if !tracker.State().Active() {
		t.Fatal("connected Active = false, want true")
	}

	if got := tracker.SetState(""); got != ConnectionStateDisconnected {
		t.Fatalf("SetState empty = %q, want disconnected", got)
	}
	if got := tracker.Snapshot().State; got != ConnectionStateDisconnected {
		t.Fatalf("Snapshot State = %q, want disconnected", got)
	}
	if !ConnectionStateClosed.Terminal() {
		t.Fatal("closed Terminal = false, want true")
	}
}

func TestMessageQueueDropsNewestWhenFull(t *testing.T) {
	t.Parallel()

	queue := NewMessageQueue[string](MessageQueueOptions{
		MaxQueuedMessages: 2,
		DropPolicy:        DropNewest,
	})

	if result := queue.Enqueue("a"); !result.Accepted || result.Dropped {
		t.Fatalf("first Enqueue = %+v, want accepted without drop", result)
	}
	if result := queue.Enqueue("b"); !result.Accepted || result.Dropped {
		t.Fatalf("second Enqueue = %+v, want accepted without drop", result)
	}
	result := queue.Enqueue("c")
	if result.Accepted {
		t.Fatalf("third Enqueue accepted = true, want false")
	}
	if !result.Dropped || result.DropPolicy != DropNewest || result.DroppedMessage != "c" {
		t.Fatalf("third Enqueue = %+v, want dropped newest c", result)
	}

	assertDequeued(t, queue, "a")
	assertDequeued(t, queue, "b")
	assertEmpty(t, queue)

	snapshot := queue.Snapshot()
	if snapshot.EnqueuedMessages != 2 {
		t.Fatalf("EnqueuedMessages = %d, want 2", snapshot.EnqueuedMessages)
	}
	if snapshot.DequeuedMessages != 2 {
		t.Fatalf("DequeuedMessages = %d, want 2", snapshot.DequeuedMessages)
	}
	if snapshot.DroppedMessages != 1 || snapshot.DroppedNewestMessages != 1 || snapshot.DroppedOldestMessages != 0 {
		t.Fatalf("drop metrics = %+v, want one newest drop", snapshot)
	}
}

func TestMessageQueueDropsOldestWhenFull(t *testing.T) {
	t.Parallel()

	queue := NewMessageQueue[string](MessageQueueOptions{
		MaxQueuedMessages: 2,
		DropPolicy:        DropOldest,
	})

	queue.Enqueue("a")
	queue.Enqueue("b")
	result := queue.Enqueue("c")
	if !result.Accepted {
		t.Fatalf("third Enqueue accepted = false, want true")
	}
	if !result.Dropped || result.DropPolicy != DropOldest || result.DroppedMessage != "a" {
		t.Fatalf("third Enqueue = %+v, want dropped oldest a", result)
	}

	assertDequeued(t, queue, "b")
	assertDequeued(t, queue, "c")
	assertEmpty(t, queue)

	snapshot := queue.Snapshot()
	if snapshot.EnqueuedMessages != 3 {
		t.Fatalf("EnqueuedMessages = %d, want 3", snapshot.EnqueuedMessages)
	}
	if snapshot.DroppedMessages != 1 || snapshot.DroppedOldestMessages != 1 || snapshot.DroppedNewestMessages != 0 {
		t.Fatalf("drop metrics = %+v, want one oldest drop", snapshot)
	}
}

func TestMessageQueueDefaultsAndConcurrentUse(t *testing.T) {
	t.Parallel()

	var queue MessageQueue[int]
	if got := queue.MaxQueuedMessages(); got != DefaultMaxQueuedMessages {
		t.Fatalf("MaxQueuedMessages = %d, want %d", got, DefaultMaxQueuedMessages)
	}

	const workers = 8
	const iterations = 100

	var wg sync.WaitGroup
	wg.Add(workers)
	for i := 0; i < workers; i++ {
		go func(offset int) {
			defer wg.Done()
			for j := 0; j < iterations; j++ {
				queue.Enqueue(offset*iterations + j)
			}
		}(i)
	}
	wg.Wait()

	snapshot := queue.Snapshot()
	if snapshot.QueuedMessages != DefaultMaxQueuedMessages {
		t.Fatalf("QueuedMessages = %d, want %d", snapshot.QueuedMessages, DefaultMaxQueuedMessages)
	}
	if snapshot.EnqueuedMessages != DefaultMaxQueuedMessages {
		t.Fatalf("EnqueuedMessages = %d, want %d", snapshot.EnqueuedMessages, DefaultMaxQueuedMessages)
	}
	wantDropped := uint64(workers*iterations - DefaultMaxQueuedMessages)
	if snapshot.DroppedNewestMessages != wantDropped {
		t.Fatalf("DroppedNewestMessages = %d, want %d", snapshot.DroppedNewestMessages, wantDropped)
	}
}

func TestReconnectRetrySchedule(t *testing.T) {
	t.Parallel()

	schedule := ReconnectRetrySchedule{
		MaxAttempts: 3,
		BaseDelay:   time.Second,
		MaxDelay:    3 * time.Second,
	}

	tests := []struct {
		attempt int
		want    time.Duration
	}{
		{attempt: 0, want: 0},
		{attempt: 1, want: time.Second},
		{attempt: 2, want: 2 * time.Second},
		{attempt: 3, want: 3 * time.Second},
		{attempt: 4, want: 0},
	}

	for _, tt := range tests {
		if got := schedule.DelayBeforeAttempt(tt.attempt); got != tt.want {
			t.Fatalf("DelayBeforeAttempt(%d) = %s, want %s", tt.attempt, got, tt.want)
		}
	}
	if schedule.ShouldAttempt(4) {
		t.Fatal("ShouldAttempt(4) = true, want false")
	}
	if got := schedule.NextDelay(1); got != 2*time.Second {
		t.Fatalf("NextDelay(1) = %s, want 2s", got)
	}

	normalized := (ReconnectRetrySchedule{MaxDelay: time.Millisecond}).Normalize()
	if normalized.BaseDelay != DefaultReconnectBaseDelay {
		t.Fatalf("BaseDelay = %s, want %s", normalized.BaseDelay, DefaultReconnectBaseDelay)
	}
	if normalized.MaxDelay != normalized.BaseDelay {
		t.Fatalf("MaxDelay = %s, want BaseDelay %s", normalized.MaxDelay, normalized.BaseDelay)
	}
}

func TestReconnectTrackerRecordsAttemptsAndReset(t *testing.T) {
	t.Parallel()

	tracker := NewReconnectTracker(ReconnectRetrySchedule{
		MaxAttempts: 2,
		BaseDelay:   time.Second,
		MaxDelay:    5 * time.Second,
	})

	delay, ok := tracker.NextDelay()
	if !ok || delay != time.Second {
		t.Fatalf("first NextDelay = %s, %t, want 1s true", delay, ok)
	}
	delay, ok = tracker.NextDelay()
	if !ok || delay != 2*time.Second {
		t.Fatalf("second NextDelay = %s, %t, want 2s true", delay, ok)
	}
	delay, ok = tracker.NextDelay()
	if ok || delay != 0 {
		t.Fatalf("third NextDelay = %s, %t, want 0 false", delay, ok)
	}

	snapshot := tracker.Snapshot()
	if snapshot.Attempts != 2 {
		t.Fatalf("Attempts = %d, want 2", snapshot.Attempts)
	}
	if snapshot.LastDelay != 2*time.Second {
		t.Fatalf("LastDelay = %s, want 2s", snapshot.LastDelay)
	}
	if !snapshot.Exhausted {
		t.Fatal("Exhausted = false, want true")
	}

	tracker.Reset()
	snapshot = tracker.Snapshot()
	if snapshot.Attempts != 0 || snapshot.LastDelay != 0 || snapshot.Exhausted {
		t.Fatalf("Snapshot after Reset = %+v, want cleared", snapshot)
	}
}

func assertDequeued[T comparable](t *testing.T, queue *MessageQueue[T], want T) {
	t.Helper()

	got, ok := queue.Dequeue()
	if !ok {
		t.Fatalf("Dequeue ok = false, want true")
	}
	if got != want {
		t.Fatalf("Dequeue = %v, want %v", got, want)
	}
}

func assertEmpty[T any](t *testing.T, queue *MessageQueue[T]) {
	t.Helper()

	if _, ok := queue.Dequeue(); ok {
		t.Fatal("Dequeue ok = true, want false")
	}
}
