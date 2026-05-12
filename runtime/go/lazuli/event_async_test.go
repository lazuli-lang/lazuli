package lazuli

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"
)

func TestAsyncEventBusPublishesToSubscribers(t *testing.T) {
	bus := NewAsyncEventBus(AsyncEventBusOptions{Workers: 2, QueueSize: 4})
	t.Cleanup(func() {
		if err := bus.Shutdown(context.Background()); err != nil {
			t.Fatalf("Shutdown: %v", err)
		}
	})

	var mu sync.Mutex
	got := make([]string, 0, 2)
	bus.Subscribe("customer.created", func(_ context.Context, e Event) error {
		mu.Lock()
		defer mu.Unlock()
		got = append(got, e.Payload["id"].(string))
		return nil
	})
	bus.Subscribe("customer.created", func(_ context.Context, e Event) error {
		mu.Lock()
		defer mu.Unlock()
		got = append(got, e.Payload["id"].(string))
		return nil
	})

	event := Event{
		Name:    "customer.created",
		Payload: map[string]any{"id": "cus_1"},
	}
	if err := bus.PublishAsync(context.Background(), event); err != nil {
		t.Fatalf("PublishAsync: %v", err)
	}
	if err := bus.Drain(asyncTestContext(t)); err != nil {
		t.Fatalf("Drain: %v", err)
	}

	mu.Lock()
	defer mu.Unlock()
	if len(got) != 2 {
		t.Fatalf("subscriber calls = %d, want 2", len(got))
	}
	for _, id := range got {
		if id != "cus_1" {
			t.Fatalf("subscriber payload id = %q, want cus_1", id)
		}
	}
}

func TestAsyncEventBusQueueFull(t *testing.T) {
	bus := NewAsyncEventBus(AsyncEventBusOptions{Workers: 1, QueueSize: 1})
	release := make(chan struct{})
	started := make(chan struct{})
	var startedOnce sync.Once
	t.Cleanup(func() {
		close(release)
		_ = bus.Shutdown(context.Background())
	})

	bus.Subscribe("slow", func(context.Context, Event) error {
		startedOnce.Do(func() { close(started) })
		<-release
		return nil
	})

	if err := bus.PublishAsync(context.Background(), Event{Name: "slow"}); err != nil {
		t.Fatalf("first PublishAsync: %v", err)
	}
	waitForClose(t, started)

	if err := bus.PublishAsync(context.Background(), Event{Name: "slow"}); err != nil {
		t.Fatalf("second PublishAsync: %v", err)
	}
	if err := bus.PublishAsync(context.Background(), Event{Name: "slow"}); !errors.Is(err, ErrAsyncEventBusQueueFull) {
		t.Fatalf("third PublishAsync error = %v, want ErrAsyncEventBusQueueFull", err)
	}
}

func TestAsyncEventBusErrorHandler(t *testing.T) {
	wantErr := errors.New("boom")
	handled := make(chan error, 1)
	bus := NewAsyncEventBus(AsyncEventBusOptions{
		Workers:   1,
		QueueSize: 2,
		ErrorHandler: func(_ context.Context, e Event, err error) {
			if e.Name != "fails" {
				t.Errorf("handler event = %q, want fails", e.Name)
			}
			handled <- err
		},
	})
	t.Cleanup(func() {
		if err := bus.Shutdown(context.Background()); err != nil {
			t.Fatalf("Shutdown: %v", err)
		}
	})

	bus.Subscribe("fails", func(context.Context, Event) error {
		return wantErr
	})
	if err := bus.PublishAsync(context.Background(), Event{Name: "fails"}); err != nil {
		t.Fatalf("PublishAsync: %v", err)
	}
	if err := bus.Drain(asyncTestContext(t)); err != nil {
		t.Fatalf("Drain: %v", err)
	}

	select {
	case got := <-handled:
		if !errors.Is(got, wantErr) {
			t.Fatalf("handled error = %v, want %v", got, wantErr)
		}
	default:
		t.Fatal("error handler was not called")
	}
}

func TestAsyncEventBusShutdownDrainsAndRejectsPublish(t *testing.T) {
	bus := NewAsyncEventBus(AsyncEventBusOptions{Workers: 1, QueueSize: 2})
	delivered := make(chan struct{}, 1)
	bus.Subscribe("done", func(context.Context, Event) error {
		delivered <- struct{}{}
		return nil
	})

	if err := bus.PublishAsync(context.Background(), Event{Name: "done"}); err != nil {
		t.Fatalf("PublishAsync: %v", err)
	}
	if err := bus.Shutdown(asyncTestContext(t)); err != nil {
		t.Fatalf("Shutdown: %v", err)
	}

	select {
	case <-delivered:
	default:
		t.Fatal("Shutdown returned before delivering queued event")
	}

	if err := bus.PublishAsync(context.Background(), Event{Name: "done"}); !errors.Is(err, ErrAsyncEventBusClosed) {
		t.Fatalf("PublishAsync after Shutdown error = %v, want ErrAsyncEventBusClosed", err)
	}
	if err := bus.Shutdown(context.Background()); err != nil {
		t.Fatalf("second Shutdown: %v", err)
	}
}

func TestAsyncEventBusOptions(t *testing.T) {
	bus := NewAsyncEventBus(AsyncEventBusOptions{Workers: 3, QueueSize: 7})
	t.Cleanup(func() {
		if err := bus.Shutdown(context.Background()); err != nil {
			t.Fatalf("Shutdown: %v", err)
		}
	})

	if got := bus.WorkerCount(); got != 3 {
		t.Fatalf("WorkerCount = %d, want 3", got)
	}
	if got := bus.QueueSize(); got != 7 {
		t.Fatalf("QueueSize = %d, want 7", got)
	}
}

func TestAsyncEventBusClonesPayloadAtPublish(t *testing.T) {
	bus := NewAsyncEventBus(AsyncEventBusOptions{Workers: 1, QueueSize: 2})
	release := make(chan struct{})
	var releaseOnce sync.Once
	firstStarted := make(chan struct{})
	var firstOnce sync.Once
	got := make(chan string, 1)
	t.Cleanup(func() {
		releaseOnce.Do(func() { close(release) })
		_ = bus.Shutdown(context.Background())
	})

	bus.Subscribe("event", func(_ context.Context, e Event) error {
		if e.Payload["id"] == "first" {
			firstOnce.Do(func() { close(firstStarted) })
			<-release
			return nil
		}
		got <- e.Payload["id"].(string)
		return nil
	})

	if err := bus.PublishAsync(context.Background(), Event{
		Name:    "event",
		Payload: map[string]any{"id": "first"},
	}); err != nil {
		t.Fatalf("first PublishAsync: %v", err)
	}
	waitForClose(t, firstStarted)

	payload := map[string]any{"id": "original"}
	if err := bus.PublishAsync(context.Background(), Event{
		Name:    "event",
		Payload: payload,
	}); err != nil {
		t.Fatalf("second PublishAsync: %v", err)
	}
	payload["id"] = "mutated"
	releaseOnce.Do(func() { close(release) })

	if err := bus.Drain(asyncTestContext(t)); err != nil {
		t.Fatalf("Drain: %v", err)
	}
	select {
	case id := <-got:
		if id != "original" {
			t.Fatalf("payload id = %q, want original", id)
		}
	default:
		t.Fatal("queued subscriber was not called")
	}
}

func asyncTestContext(t *testing.T) context.Context {
	t.Helper()

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	t.Cleanup(cancel)
	return ctx
}

func waitForClose(t *testing.T, ch <-chan struct{}) {
	t.Helper()

	select {
	case <-ch:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for async event subscriber")
	}
}
