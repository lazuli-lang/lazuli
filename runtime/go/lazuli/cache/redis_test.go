package cache

import (
	"context"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"
)

func TestRedisBackendPutGetAndTTL(t *testing.T) {
	backend, server := newTestRedisBackend(t)
	ctx := context.Background()
	key := "customer.list|1|abc"

	if err := backend.Put(ctx, key, []byte("payload"), time.Second, []string{"customer-list"}); err != nil {
		t.Fatalf("Put() error = %v", err)
	}

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

	server.FastForward(2 * time.Second)
	_, hit, err = backend.Get(ctx, key)
	if err != nil {
		t.Fatalf("Get() after ttl error = %v", err)
	}
	if hit {
		t.Fatal("Get() after ttl hit = true, want false")
	}
}

func TestRedisBackendInvalidateTags(t *testing.T) {
	backend, _ := newTestRedisBackend(t)
	ctx := context.Background()

	mustPut(t, backend, "customer.list|1|a", "one", []string{"shared", "list"})
	mustPut(t, backend, "customer.by_id|1|b", "two", []string{"shared"})
	mustPut(t, backend, "invoice.list|1|c", "three", []string{"invoice"})

	deleted, err := backend.InvalidateTags(ctx, []string{"shared"})
	if err != nil {
		t.Fatalf("InvalidateTags() error = %v", err)
	}
	if deleted != 2 {
		t.Fatalf("InvalidateTags() deleted = %d, want 2", deleted)
	}
	assertMiss(t, backend, "customer.list|1|a")
	assertMiss(t, backend, "customer.by_id|1|b")
	assertHit(t, backend, "invoice.list|1|c", "three")

	exists, err := backend.Client.Exists(ctx, redisTagKey("shared")).Result()
	if err != nil {
		t.Fatalf("Exists(tag) error = %v", err)
	}
	if exists != 0 {
		t.Fatalf("tag set still exists after invalidation")
	}
}

func TestRedisBackendInvalidateQueries(t *testing.T) {
	backend, _ := newTestRedisBackend(t)
	ctx := context.Background()

	mustPut(t, backend, "customer.list|1|a", "one", nil)
	mustPut(t, backend, "customer.list|2|b", "two", nil)
	mustPut(t, backend, "customer.by_id|1|c", "three", nil)

	deleted, err := backend.InvalidateQueries(ctx, []string{"customer.list"})
	if err != nil {
		t.Fatalf("InvalidateQueries() error = %v", err)
	}
	if deleted != 2 {
		t.Fatalf("InvalidateQueries() deleted = %d, want 2", deleted)
	}
	assertMiss(t, backend, "customer.list|1|a")
	assertMiss(t, backend, "customer.list|2|b")
	assertHit(t, backend, "customer.by_id|1|c", "three")
}

func TestRedisBackendStatsCountsCacheEntries(t *testing.T) {
	backend, _ := newTestRedisBackend(t)
	ctx := context.Background()

	mustPut(t, backend, "customer.list|1|a", "one", []string{"shared"})
	mustPut(t, backend, "customer.by_id|1|b", "two", []string{"shared"})

	stats, err := backend.Stats(ctx)
	if err != nil {
		t.Fatalf("Stats() error = %v", err)
	}
	if stats.Entries != 2 {
		t.Fatalf("Stats().Entries = %d, want 2", stats.Entries)
	}
}

func newTestRedisBackend(t *testing.T) (*RedisBackend, *miniredis.Miniredis) {
	t.Helper()

	server, err := miniredis.Run()
	if err != nil {
		t.Fatalf("miniredis.Run() error = %v", err)
	}
	t.Cleanup(server.Close)

	client := redis.NewClient(&redis.Options{Addr: server.Addr()})
	t.Cleanup(func() {
		if err := client.Close(); err != nil {
			t.Fatalf("redis client Close() error = %v", err)
		}
	})

	return &RedisBackend{Client: client}, server
}

func mustPut(t *testing.T, backend *RedisBackend, key, value string, tags []string) {
	t.Helper()
	if err := backend.Put(context.Background(), key, []byte(value), time.Minute, tags); err != nil {
		t.Fatalf("Put(%q) error = %v", key, err)
	}
}

func assertHit(t *testing.T, backend *RedisBackend, key, want string) {
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

func assertMiss(t *testing.T, backend *RedisBackend, key string) {
	t.Helper()

	_, hit, err := backend.Get(context.Background(), key)
	if err != nil {
		t.Fatalf("Get(%q) error = %v", key, err)
	}
	if hit {
		t.Fatalf("Get(%q) hit = true, want false", key)
	}
}
