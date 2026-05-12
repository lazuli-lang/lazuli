package cache

import (
	"context"
	"errors"
	"sync/atomic"
	"testing"
	"time"
)

type coalesceResult struct {
	value  []byte
	shared bool
	err    error
}

func TestCoalescerDoSharesConcurrentProducer(t *testing.T) {
	var coalescer Coalescer
	var calls int32
	started := make(chan struct{})
	release := make(chan struct{})
	producerDone := make(chan coalesceResult, 1)

	fn := func(ctx context.Context) ([]byte, error) {
		if atomic.AddInt32(&calls, 1) != 1 {
			t.Fatal("duplicate producer ran")
		}
		close(started)
		<-release
		return []byte("payload"), nil
	}

	go func() {
		value, shared, err := coalescer.Do(context.Background(), "key", fn)
		producerDone <- coalesceResult{value: value, shared: shared, err: err}
	}()

	<-started

	timer := time.AfterFunc(25*time.Millisecond, func() {
		close(release)
	})
	defer timer.Stop()

	waiter := mustDo(t, &coalescer, context.Background(), "key", func(context.Context) ([]byte, error) {
		t.Fatal("waiter ran producer")
		return nil, nil
	})
	producer := receiveCoalesceResult(t, producerDone)

	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Fatalf("producer calls = %d, want 1", got)
	}
	if producer.err != nil {
		t.Fatalf("producer err = %v, want nil", producer.err)
	}
	if waiter.err != nil {
		t.Fatalf("waiter err = %v, want nil", waiter.err)
	}
	if string(producer.value) != "payload" || string(waiter.value) != "payload" {
		t.Fatalf("values = %q and %q, want payload", producer.value, waiter.value)
	}
	if producer.shared {
		t.Fatal("producer shared = true, want false")
	}
	if !waiter.shared {
		t.Fatal("waiter shared = false, want true")
	}

	producer.value[0] = 'P'
	if string(waiter.value) != "payload" {
		t.Fatalf("mutating one returned slice changed another: %q", waiter.value)
	}
}

func TestCoalescerDoWaiterCancellationDoesNotCancelProducer(t *testing.T) {
	var coalescer Coalescer
	producerCtx := make(chan context.Context, 1)
	release := make(chan struct{})
	producerDone := make(chan coalesceResult, 1)

	go func() {
		value, shared, err := coalescer.Do(context.Background(), "key", func(ctx context.Context) ([]byte, error) {
			producerCtx <- ctx
			<-release
			return []byte("payload"), nil
		})
		producerDone <- coalesceResult{value: value, shared: shared, err: err}
	}()

	ctx := receiveProducerContext(t, producerCtx)
	waiterCtx, cancelWaiter := context.WithCancel(context.Background())
	timer := time.AfterFunc(25*time.Millisecond, cancelWaiter)
	defer timer.Stop()

	waiter := mustDo(t, &coalescer, waiterCtx, "key", func(context.Context) ([]byte, error) {
		t.Fatal("cancelled waiter ran producer")
		return nil, nil
	})
	if !waiter.shared {
		t.Fatal("cancelled waiter shared = false, want true")
	}
	if !errors.Is(waiter.err, context.Canceled) {
		t.Fatalf("cancelled waiter err = %v, want context.Canceled", waiter.err)
	}

	select {
	case <-ctx.Done():
		t.Fatalf("producer context was cancelled: %v", ctx.Err())
	default:
	}

	close(release)
	producer := receiveCoalesceResult(t, producerDone)
	if producer.shared {
		t.Fatal("producer shared = true, want false")
	}
	if producer.err != nil {
		t.Fatalf("producer err = %v, want nil", producer.err)
	}
	if string(producer.value) != "payload" {
		t.Fatalf("producer value = %q, want payload", producer.value)
	}
}

func TestCoalescerDoCopiesProducerBytes(t *testing.T) {
	var coalescer Coalescer
	source := []byte("payload")

	value, shared, err := coalescer.Do(context.Background(), "key", func(context.Context) ([]byte, error) {
		return source, nil
	})
	if err != nil {
		t.Fatalf("Do() err = %v, want nil", err)
	}
	if shared {
		t.Fatal("Do() shared = true, want false")
	}

	source[0] = 'P'
	if string(value) != "payload" {
		t.Fatalf("value after source mutation = %q, want payload", value)
	}
}

func TestCoalescerDoPropagatesProducerPanicToWaiters(t *testing.T) {
	var coalescer Coalescer
	started := make(chan struct{})
	release := make(chan struct{})
	producerPanic := make(chan any, 1)

	go func() {
		defer func() {
			producerPanic <- recover()
		}()
		_, _, _ = coalescer.Do(context.Background(), "key", func(context.Context) ([]byte, error) {
			close(started)
			<-release
			panic("boom")
		})
	}()

	<-started

	timer := time.AfterFunc(25*time.Millisecond, func() {
		close(release)
	})
	defer timer.Stop()

	waiterPanic := catchPanic(t, func() {
		_, _, _ = coalescer.Do(context.Background(), "key", func(context.Context) ([]byte, error) {
			t.Fatal("waiter ran producer")
			return nil, nil
		})
	})
	if waiterPanic != "boom" {
		t.Fatalf("waiter panic = %v, want boom", waiterPanic)
	}

	if got := receivePanic(t, producerPanic); got != "boom" {
		t.Fatalf("producer panic = %v, want boom", got)
	}
}

func TestCoalescerDoDoesNotCacheCompletedResults(t *testing.T) {
	var coalescer Coalescer
	var calls int32

	for i := 0; i < 2; i++ {
		value, shared, err := coalescer.Do(context.Background(), "key", func(context.Context) ([]byte, error) {
			return []byte{byte(atomic.AddInt32(&calls, 1))}, nil
		})
		if err != nil {
			t.Fatalf("Do() err = %v, want nil", err)
		}
		if shared {
			t.Fatal("Do() shared = true, want false")
		}
		if want := byte(i + 1); value[0] != want {
			t.Fatalf("Do() value[0] = %d, want %d", value[0], want)
		}
	}

	if got := atomic.LoadInt32(&calls); got != 2 {
		t.Fatalf("producer calls = %d, want 2", got)
	}
}

func receiveCoalesceResult(t *testing.T, results <-chan coalesceResult) coalesceResult {
	t.Helper()

	select {
	case result := <-results:
		return result
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for coalescer result")
		return coalesceResult{}
	}
}

func mustDo(t *testing.T, coalescer *Coalescer, ctx context.Context, key string, fn func(context.Context) ([]byte, error)) coalesceResult {
	t.Helper()

	value, shared, err := coalescer.Do(ctx, key, fn)
	return coalesceResult{value: value, shared: shared, err: err}
}

func catchPanic(t *testing.T, fn func()) (panicValue any) {
	t.Helper()

	defer func() {
		panicValue = recover()
		if panicValue == nil {
			t.Fatal("function did not panic")
		}
	}()

	fn()
	return nil
}

func receivePanic(t *testing.T, panics <-chan any) any {
	t.Helper()

	select {
	case value := <-panics:
		return value
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for panic")
		return nil
	}
}

func receiveProducerContext(t *testing.T, contexts <-chan context.Context) context.Context {
	t.Helper()

	select {
	case ctx := <-contexts:
		return ctx
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for producer context")
		return context.Background()
	}
}
