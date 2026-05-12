package cache

import (
	"context"
	"errors"
	"reflect"
	"sync"
	"testing"
	"time"
)

func TestWarmerStoresLoadedBytes(t *testing.T) {
	backend := newWarmTestBackend()
	ctx := context.Background()

	tasks := []WarmTask{
		{
			Name: "customers",
			Key:  "customer.query.list|1|abc",
			TTL:  time.Minute,
			Tags: []string{"customer", "list"},
			Load: func(context.Context) ([]byte, error) {
				return []byte("customers"), nil
			},
		},
		{
			Name: "invoices",
			Key:  "invoice.query.list|1|abc",
			TTL:  2 * time.Minute,
			Tags: []string{"invoice"},
			Load: func(context.Context) ([]byte, error) {
				return []byte("invoices"), nil
			},
		},
	}

	result := (Warmer{
		Backend: backend,
		Options: WarmOptions{Concurrency: 2},
	}).Warm(ctx, tasks)

	if result.Total != 2 || result.Warmed != 2 || result.Failed != 0 || result.Skipped != 0 {
		t.Fatalf("Warm() result = %+v, want 2 warmed and no failures", result)
	}
	if len(result.Tasks) != 2 {
		t.Fatalf("Warm() task results = %d, want 2", len(result.Tasks))
	}
	for _, task := range result.Tasks {
		if task.Err != nil {
			t.Fatalf("Warm() task %q error = %v, want nil", task.Name, task.Err)
		}
	}

	assertWarmPut(t, backend, "customer.query.list|1|abc", warmPut{
		key:   "customer.query.list|1|abc",
		value: []byte("customers"),
		ttl:   time.Minute,
		tags:  []string{"customer", "list"},
	})
	assertWarmPut(t, backend, "invoice.query.list|1|abc", warmPut{
		key:   "invoice.query.list|1|abc",
		value: []byte("invoices"),
		ttl:   2 * time.Minute,
		tags:  []string{"invoice"},
	})
}

func TestWarmRecordsPerTaskErrors(t *testing.T) {
	loadErr := errors.New("load failed")
	putErr := errors.New("put failed")
	backend := newWarmTestBackend()
	backend.putErrs["put"] = putErr

	result := Warm(context.Background(), backend, []WarmTask{
		{
			Name: "load",
			Key:  "load",
			Load: func(context.Context) ([]byte, error) {
				return nil, loadErr
			},
		},
		{
			Name: "put",
			Key:  "put",
			Load: func(context.Context) ([]byte, error) {
				return []byte("payload"), nil
			},
		},
		{Name: "missing-key", Load: func(context.Context) ([]byte, error) {
			return []byte("payload"), nil
		}},
		{Name: "missing-load", Key: "missing-load"},
	}, WarmOptions{Concurrency: 4})

	if result.Total != 4 || result.Warmed != 0 || result.Failed != 4 {
		t.Fatalf("Warm() result = %+v, want 4 failures", result)
	}
	assertWarmTaskError(t, result.Tasks[0], loadErr)
	assertWarmTaskError(t, result.Tasks[1], putErr)
	assertWarmTaskError(t, result.Tasks[2], ErrWarmTaskKeyRequired)
	assertWarmTaskError(t, result.Tasks[3], ErrWarmTaskLoadRequired)
}

func TestWarmMarksTasksSkippedWhenContextCanceled(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	loads := 0
	result := Warm(ctx, newWarmTestBackend(), []WarmTask{
		{
			Name: "one",
			Key:  "one",
			Load: func(context.Context) ([]byte, error) {
				loads++
				return []byte("one"), nil
			},
		},
		{
			Name: "two",
			Key:  "two",
			Load: func(context.Context) ([]byte, error) {
				loads++
				return []byte("two"), nil
			},
		},
	}, WarmOptions{Concurrency: 2})

	if loads != 0 {
		t.Fatalf("Load called %d times, want 0", loads)
	}
	if result.Total != 2 || result.Warmed != 0 || result.Failed != 2 || result.Skipped != 2 {
		t.Fatalf("Warm() result = %+v, want 2 skipped failures", result)
	}
	for _, task := range result.Tasks {
		if !task.Skipped {
			t.Fatalf("Warm() task %q Skipped = false, want true", task.Name)
		}
		assertWarmTaskError(t, task, context.Canceled)
	}
}

func TestWarmStopsTaskWhenContextCanceledAfterLoad(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	backend := newWarmTestBackend()

	result := Warm(ctx, backend, []WarmTask{
		{
			Name: "canceling",
			Key:  "canceling",
			Load: func(context.Context) ([]byte, error) {
				cancel()
				return []byte("payload"), nil
			},
		},
	}, WarmOptions{})

	if result.Total != 1 || result.Warmed != 0 || result.Failed != 1 || result.Skipped != 0 {
		t.Fatalf("Warm() result = %+v, want one running task failure", result)
	}
	assertWarmTaskError(t, result.Tasks[0], context.Canceled)
	backend.mu.Lock()
	puts := len(backend.puts)
	backend.mu.Unlock()
	if puts != 0 {
		t.Fatalf("Put called %d times, want 0", puts)
	}
}

func TestWarmHonorsConcurrencyLimit(t *testing.T) {
	backend := newWarmTestBackend()
	started := make(chan string, 3)
	release := make(chan struct{})
	tasks := []WarmTask{
		blockingWarmTask("one", started, release),
		blockingWarmTask("two", started, release),
		blockingWarmTask("three", started, release),
	}
	done := make(chan WarmResult, 1)

	go func() {
		done <- Warm(context.Background(), backend, tasks, WarmOptions{Concurrency: 2})
	}()

	for i := 0; i < 2; i++ {
		select {
		case <-started:
		case <-time.After(time.Second):
			t.Fatal("timed out waiting for initial warm tasks to start")
		}
	}

	select {
	case name := <-started:
		t.Fatalf("task %q started before a concurrency slot was available", name)
	case <-time.After(100 * time.Millisecond):
	}

	close(release)

	select {
	case result := <-done:
		if result.Total != 3 || result.Warmed != 3 || result.Failed != 0 {
			t.Fatalf("Warm() result = %+v, want 3 warmed", result)
		}
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for warm tasks to finish")
	}
}

type warmTestBackend struct {
	mu      sync.Mutex
	puts    map[string]warmPut
	putErrs map[string]error
}

type warmPut struct {
	key   string
	value []byte
	ttl   time.Duration
	tags  []string
}

func newWarmTestBackend() *warmTestBackend {
	return &warmTestBackend{
		puts:    make(map[string]warmPut),
		putErrs: make(map[string]error),
	}
}

func (b *warmTestBackend) Get(context.Context, string) ([]byte, bool, error) {
	return nil, false, nil
}

func (b *warmTestBackend) Put(_ context.Context, key string, value []byte, ttl time.Duration, tags []string) error {
	if err := b.putErrs[key]; err != nil {
		return err
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	b.puts[key] = warmPut{
		key:   key,
		value: append([]byte(nil), value...),
		ttl:   ttl,
		tags:  append([]string(nil), tags...),
	}
	return nil
}

func (b *warmTestBackend) InvalidateQueries(context.Context, []string) (int, error) {
	return 0, nil
}

func (b *warmTestBackend) InvalidateTags(context.Context, []string) (int, error) {
	return 0, nil
}

func (b *warmTestBackend) Stats(context.Context) (QueryStats, error) {
	return QueryStats{}, nil
}

func assertWarmPut(t *testing.T, backend *warmTestBackend, key string, want warmPut) {
	t.Helper()

	backend.mu.Lock()
	got, ok := backend.puts[key]
	backend.mu.Unlock()

	if !ok {
		t.Fatalf("Put(%q) was not called", key)
	}
	if got.key != want.key || string(got.value) != string(want.value) || got.ttl != want.ttl || !reflect.DeepEqual(got.tags, want.tags) {
		t.Fatalf("Put(%q) = %+v, want %+v", key, got, want)
	}
}

func assertWarmTaskError(t *testing.T, result WarmTaskResult, want error) {
	t.Helper()
	if !errors.Is(result.Err, want) {
		t.Fatalf("Warm() task %q error = %v, want %v", result.Name, result.Err, want)
	}
}

func blockingWarmTask(name string, started chan<- string, release <-chan struct{}) WarmTask {
	return WarmTask{
		Name: name,
		Key:  name,
		Load: func(context.Context) ([]byte, error) {
			started <- name
			<-release
			return []byte(name), nil
		},
	}
}
