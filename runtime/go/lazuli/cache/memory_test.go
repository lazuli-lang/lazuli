package cache

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"
)

func TestMemoryBackendPutGetTTLAndCopies(t *testing.T) {
	backend := NewMemoryBackend()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	backend.now = func() time.Time { return now }

	ctx := context.Background()
	key := "customer.query.list|1|abc"
	payload := []byte("payload")
	if err := backend.Put(ctx, key, payload, time.Second, []string{"customer-list"}); err != nil {
		t.Fatalf("Put() error = %v", err)
	}
	payload[0] = 'P'

	value, hit, err := backend.Get(ctx, key)
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	if !hit {
		t.Fatal("Get() hit = false, want true")
	}
	if string(value) != "payload" {
		t.Fatalf("Get() value = %q, want payload", value)
	}

	value[0] = 'P'
	value, hit, err = backend.Get(ctx, key)
	if err != nil {
		t.Fatalf("Get() after returned value mutation error = %v", err)
	}
	if !hit {
		t.Fatal("Get() after returned value mutation hit = false, want true")
	}
	if string(value) != "payload" {
		t.Fatalf("Get() after returned value mutation value = %q, want payload", value)
	}

	now = now.Add(2 * time.Second)
	_, hit, err = backend.Get(ctx, key)
	if err != nil {
		t.Fatalf("Get() after ttl error = %v", err)
	}
	if hit {
		t.Fatal("Get() after ttl hit = true, want false")
	}

	stats, err := backend.Stats(ctx)
	if err != nil {
		t.Fatalf("Stats() error = %v", err)
	}
	if stats.Entries != 0 || stats.Hits != 2 || stats.Misses != 1 || stats.Evicts != 1 {
		t.Fatalf("Stats() = %+v, want entries=0 hits=2 misses=1 evicts=1", stats)
	}
}

func TestMemoryBackendDefaultTTLCanDisableExpiry(t *testing.T) {
	backend := NewMemoryBackend(WithMemoryDefaultTTL(-1))
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	backend.now = func() time.Time { return now }

	mustPutMemory(t, backend, "customer.query.list|1|a", "one", 0, nil)

	now = now.Add(24 * time.Hour)
	assertMemoryHit(t, backend, "customer.query.list|1|a", "one")
}

func TestMemoryBackendInvalidateTags(t *testing.T) {
	backend := NewMemoryBackend()
	ctx := context.Background()

	mustPutMemory(t, backend, "customer.query.list|1|a", "one", time.Minute, []string{"shared", "list", "shared", ""})
	mustPutMemory(t, backend, "customer.query.by_id|1|b", "two", time.Minute, []string{"shared"})
	mustPutMemory(t, backend, "invoice.query.list|1|c", "three", time.Minute, []string{"invoice"})

	deleted, err := backend.InvalidateTags(ctx, []string{"shared"})
	if err != nil {
		t.Fatalf("InvalidateTags() error = %v", err)
	}
	if deleted != 2 {
		t.Fatalf("InvalidateTags() deleted = %d, want 2", deleted)
	}
	assertMemoryMiss(t, backend, "customer.query.list|1|a")
	assertMemoryMiss(t, backend, "customer.query.by_id|1|b")
	assertMemoryHit(t, backend, "invoice.query.list|1|c", "three")

	stats, err := backend.Stats(ctx)
	if err != nil {
		t.Fatalf("Stats() error = %v", err)
	}
	if stats.Entries != 1 || stats.Evicts != 2 {
		t.Fatalf("Stats() = %+v, want entries=1 evicts=2", stats)
	}
}

func TestMemoryBackendInvalidateQueries(t *testing.T) {
	backend := NewMemoryBackend()
	ctx := context.Background()

	mustPutMemory(t, backend, "customer.query.list|1|a", "one", time.Minute, nil)
	mustPutMemory(t, backend, "customer.query.list|2|b", "two", time.Minute, nil)
	mustPutMemory(t, backend, "customer.query.by_id|1|c", "three", time.Minute, nil)

	deleted, err := backend.InvalidateQueries(ctx, []string{"customer.query.list"})
	if err != nil {
		t.Fatalf("InvalidateQueries() error = %v", err)
	}
	if deleted != 2 {
		t.Fatalf("InvalidateQueries() deleted = %d, want 2", deleted)
	}
	assertMemoryMiss(t, backend, "customer.query.list|1|a")
	assertMemoryMiss(t, backend, "customer.query.list|2|b")
	assertMemoryHit(t, backend, "customer.query.by_id|1|c", "three")
}

func TestMemoryBackendStatsPrunesExpiredEntries(t *testing.T) {
	backend := NewMemoryBackend()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	backend.now = func() time.Time { return now }

	ctx := context.Background()
	mustPutMemory(t, backend, "customer.query.list|1|a", "one", time.Second, nil)
	mustPutMemory(t, backend, "customer.query.by_id|1|b", "two", -1, nil)

	now = now.Add(2 * time.Second)
	stats, err := backend.Stats(ctx)
	if err != nil {
		t.Fatalf("Stats() error = %v", err)
	}
	if stats.Entries != 1 || stats.Evicts != 1 {
		t.Fatalf("Stats() = %+v, want entries=1 evicts=1", stats)
	}
	assertMemoryHit(t, backend, "customer.query.by_id|1|b", "two")
}

func TestMemoryBackendReturnsContextErrors(t *testing.T) {
	backend := NewMemoryBackend()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, _, err := backend.Get(ctx, "key"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Get() error = %v, want context.Canceled", err)
	}
	if err := backend.Put(ctx, "key", []byte("value"), time.Minute, nil); !errors.Is(err, context.Canceled) {
		t.Fatalf("Put() error = %v, want context.Canceled", err)
	}
	if _, err := backend.InvalidateQueries(ctx, []string{"customer.query.list"}); !errors.Is(err, context.Canceled) {
		t.Fatalf("InvalidateQueries() error = %v, want context.Canceled", err)
	}
	if _, err := backend.InvalidateTags(ctx, []string{"shared"}); !errors.Is(err, context.Canceled) {
		t.Fatalf("InvalidateTags() error = %v, want context.Canceled", err)
	}
	if _, err := backend.Stats(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("Stats() error = %v, want context.Canceled", err)
	}
}

func TestMemoryBackendConcurrentAccess(t *testing.T) {
	backend := NewMemoryBackend(WithMemoryDefaultTTL(-1))
	ctx := context.Background()

	var wg sync.WaitGroup
	for i := range 20 {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			key := "customer.query.list|1|" + string(rune('a'+i))
			for range 100 {
				if err := backend.Put(ctx, key, []byte("payload"), 0, []string{"shared"}); err != nil {
					t.Errorf("Put() error = %v", err)
				}
				if _, _, err := backend.Get(ctx, key); err != nil {
					t.Errorf("Get() error = %v", err)
				}
			}
		}(i)
	}
	wg.Wait()

	deleted, err := backend.InvalidateTags(ctx, []string{"shared"})
	if err != nil {
		t.Fatalf("InvalidateTags() error = %v", err)
	}
	if deleted != 20 {
		t.Fatalf("InvalidateTags() deleted = %d, want 20", deleted)
	}
}

func mustPutMemory(t *testing.T, backend *MemoryBackend, key, value string, ttl time.Duration, tags []string) {
	t.Helper()
	if err := backend.Put(context.Background(), key, []byte(value), ttl, tags); err != nil {
		t.Fatalf("Put(%q) error = %v", key, err)
	}
}

func assertMemoryHit(t *testing.T, backend *MemoryBackend, key, want string) {
	t.Helper()

	value, hit, err := backend.Get(context.Background(), key)
	if err != nil {
		t.Fatalf("Get(%q) error = %v", key, err)
	}
	if !hit {
		t.Fatalf("Get(%q) hit = false, want true", key)
	}
	if string(value) != want {
		t.Fatalf("Get(%q) value = %q, want %q", key, value, want)
	}
}

func assertMemoryMiss(t *testing.T, backend *MemoryBackend, key string) {
	t.Helper()

	_, hit, err := backend.Get(context.Background(), key)
	if err != nil {
		t.Fatalf("Get(%q) error = %v", key, err)
	}
	if hit {
		t.Fatalf("Get(%q) hit = true, want false", key)
	}
}
