package lazuli

import (
	"testing"
	"time"
)

func TestCacheLRUStats(t *testing.T) {
	c := newCache(2)
	now := time.Unix(100, 0)

	c.put("a", "q.a", "A", time.Minute, now)
	c.put("b", "q.b", "B", time.Minute, now)
	if got, ok := c.get("a", now); !ok || got.(string) != "A" {
		t.Fatalf("get(a) = %v, %v; want A, true", got, ok)
	}

	c.put("c", "q.c", "C", time.Minute, now)
	if _, ok := c.get("b", now); ok {
		t.Fatal("expected b to be evicted")
	}

	stats := c.stats()
	if stats.Size != 2 || stats.Capacity != 2 || stats.Hits != 1 || stats.Misses != 1 || stats.Evicts != 1 {
		t.Fatalf("stats = %+v; want size=2 capacity=2 hits=1 misses=1 evicts=1", stats)
	}
}

func TestCacheTTLAndInvalidation(t *testing.T) {
	c := newCache(4)
	now := time.Unix(200, 0)

	c.put("short", "q.short", "S", time.Second, now)
	if _, ok := c.get("short", now.Add(2*time.Second)); ok {
		t.Fatal("expected short entry to expire")
	}

	c.put("a", "q.a", "A", -1, now)
	c.put("b", "q.b", "B", time.Minute, now)
	if dropped := c.invalidateQueries([]string{"q.a", "q.b"}); dropped != 2 {
		t.Fatalf("dropped = %d, want 2", dropped)
	}
	if stats := c.stats(); stats.Size != 0 {
		t.Fatalf("stats.Size = %d, want 0", stats.Size)
	}
}
