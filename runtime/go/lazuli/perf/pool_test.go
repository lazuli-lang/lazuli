package perf

import (
	"bytes"
	"sync"
	"testing"
)

func TestPoolGetPutAndStats(t *testing.T) {
	t.Parallel()

	created := 0
	pool := NewPool(func() *int {
		created++
		value := created
		return &value
	})

	first := pool.Get()
	if first == nil {
		t.Fatal("Get() returned nil, want allocated value")
	}
	if *first != 1 {
		t.Fatalf("*first = %d, want 1", *first)
	}
	pool.Put(first)

	second := pool.Get()
	if second != first {
		t.Fatal("Get() did not return retained value")
	}
	if created != 1 {
		t.Fatalf("created = %d, want 1", created)
	}

	stats := pool.Stats()
	if stats.Gets != 2 {
		t.Fatalf("Stats().Gets = %d, want 2", stats.Gets)
	}
	if stats.Hits != 1 {
		t.Fatalf("Stats().Hits = %d, want 1", stats.Hits)
	}
	if stats.Misses != 1 {
		t.Fatalf("Stats().Misses = %d, want 1", stats.Misses)
	}
	if stats.Puts != 1 {
		t.Fatalf("Stats().Puts = %d, want 1", stats.Puts)
	}
	if stats.Drops != 0 {
		t.Fatalf("Stats().Drops = %d, want 0", stats.Drops)
	}
}

func TestPoolDropsNilValues(t *testing.T) {
	t.Parallel()

	pool := NewPool(func() *bytes.Buffer {
		return bytes.NewBufferString("new")
	})

	pool.Put(nil)
	got := pool.Get()
	if got == nil {
		t.Fatal("Get() returned nil after nil Put, want factory value")
	}
	if got.String() != "new" {
		t.Fatalf("Get().String() = %q, want new", got.String())
	}

	stats := pool.Stats()
	if stats.Drops != 1 {
		t.Fatalf("Stats().Drops = %d, want 1", stats.Drops)
	}
	if stats.Puts != 0 {
		t.Fatalf("Stats().Puts = %d, want 0", stats.Puts)
	}
	if stats.Misses != 1 {
		t.Fatalf("Stats().Misses = %d, want 1", stats.Misses)
	}
}

func TestPoolConcurrentCounters(t *testing.T) {
	t.Parallel()

	pool := NewPool(func() []byte {
		return make([]byte, 0, 8)
	})
	const (
		workers    = 8
		iterations = 500
	)

	var wg sync.WaitGroup
	wg.Add(workers)
	for i := 0; i < workers; i++ {
		go func() {
			defer wg.Done()
			for j := 0; j < iterations; j++ {
				buf := pool.Get()
				pool.Put(buf[:0])
			}
		}()
	}
	wg.Wait()

	stats := pool.Stats()
	want := uint64(workers * iterations)
	if stats.Gets != want {
		t.Fatalf("Stats().Gets = %d, want %d", stats.Gets, want)
	}
	if stats.Puts != want {
		t.Fatalf("Stats().Puts = %d, want %d", stats.Puts, want)
	}
	if stats.Hits+stats.Misses != stats.Gets {
		t.Fatalf("hits + misses = %d, want gets %d", stats.Hits+stats.Misses, stats.Gets)
	}
}

func TestByteBufferPoolResetsAndRetainsSmallBuffers(t *testing.T) {
	t.Parallel()

	pool := NewByteBufferPool(64)
	buf := pool.Get()
	buf.WriteString("payload")

	pool.Put(buf)
	got := pool.Get()
	if got != buf {
		t.Fatal("Get() did not return retained buffer")
	}
	if got.Len() != 0 {
		t.Fatalf("Get().Len() = %d, want 0", got.Len())
	}

	stats := pool.Stats()
	if stats.Gets != 2 {
		t.Fatalf("Stats().Gets = %d, want 2", stats.Gets)
	}
	if stats.Hits != 1 {
		t.Fatalf("Stats().Hits = %d, want 1", stats.Hits)
	}
	if stats.Misses != 1 {
		t.Fatalf("Stats().Misses = %d, want 1", stats.Misses)
	}
	if stats.Puts != 1 {
		t.Fatalf("Stats().Puts = %d, want 1", stats.Puts)
	}
	if stats.Drops != 0 {
		t.Fatalf("Stats().Drops = %d, want 0", stats.Drops)
	}
}

func TestByteBufferPoolDropsOversizedBuffers(t *testing.T) {
	t.Parallel()

	pool := NewByteBufferPool(4)
	buf := pool.Get()
	buf.Grow(32)
	buf.WriteString("larger than retained")

	pool.Put(buf)
	got := pool.Get()
	if got == buf {
		t.Fatal("Get() returned oversized dropped buffer")
	}
	if got.Len() != 0 {
		t.Fatalf("Get().Len() = %d, want 0", got.Len())
	}

	stats := pool.Stats()
	if stats.Gets != 2 {
		t.Fatalf("Stats().Gets = %d, want 2", stats.Gets)
	}
	if stats.Hits != 0 {
		t.Fatalf("Stats().Hits = %d, want 0", stats.Hits)
	}
	if stats.Misses != 2 {
		t.Fatalf("Stats().Misses = %d, want 2", stats.Misses)
	}
	if stats.Puts != 0 {
		t.Fatalf("Stats().Puts = %d, want 0", stats.Puts)
	}
	if stats.Drops != 1 {
		t.Fatalf("Stats().Drops = %d, want 1", stats.Drops)
	}
}

func TestByteBufferPoolZeroValueUsesDefaultLimit(t *testing.T) {
	t.Parallel()

	var pool ByteBufferPool
	buf := pool.Get()
	if buf == nil {
		t.Fatal("zero-value Get() returned nil")
	}
	buf.WriteString("small")
	pool.Put(buf)

	got := pool.Get()
	if got != buf {
		t.Fatal("zero-value pool did not retain default-sized buffer")
	}
}
