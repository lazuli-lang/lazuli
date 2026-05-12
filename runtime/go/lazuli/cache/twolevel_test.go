package cache

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestTwoLevelBackendGetUsesLocalBeforeRemote(t *testing.T) {
	ctx := context.Background()
	local := newTestBackend()
	remote := newTestBackend()
	backend := NewTwoLevelBackend(local, remote, time.Minute)

	local.seed("customer.query.list|1|a", "local", nil)
	remote.seed("customer.query.list|1|a", "remote", nil)

	value, hit, err := backend.Get(ctx, "customer.query.list|1|a")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	if !hit {
		t.Fatal("Get() hit = false, want true")
	}
	if string(value) != "local" {
		t.Fatalf("Get() value = %q, want local", value)
	}
	if len(remote.gets) != 0 {
		t.Fatalf("remote Get() calls = %d, want 0", len(remote.gets))
	}
}

func TestTwoLevelBackendGetBackfillsLocalFromRemote(t *testing.T) {
	ctx := context.Background()
	local := newTestBackend()
	remote := newTestBackend()
	backend := NewTwoLevelBackend(local, remote, 5*time.Minute)

	remote.seed("customer.query.list|1|a", "remote", []string{"customers"})

	value, hit, err := backend.Get(ctx, "customer.query.list|1|a")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}
	if !hit {
		t.Fatal("Get() hit = false, want true")
	}
	if string(value) != "remote" {
		t.Fatalf("Get() value = %q, want remote", value)
	}
	if len(local.puts) != 1 {
		t.Fatalf("local Put() calls = %d, want 1", len(local.puts))
	}
	if local.puts[0].ttl != 5*time.Minute {
		t.Fatalf("local backfill ttl = %s, want 5m", local.puts[0].ttl)
	}
	if got := strings.Join(local.puts[0].tags, ","); got != twoLevelBackfillTag {
		t.Fatalf("local backfill tags = %q, want %q", got, twoLevelBackfillTag)
	}

	value, hit, err = local.Get(ctx, "customer.query.list|1|a")
	if err != nil {
		t.Fatalf("local Get() error = %v", err)
	}
	if !hit || string(value) != "remote" {
		t.Fatalf("local Get() = %q, %v; want remote, true", value, hit)
	}
}

func TestTwoLevelBackendPutWritesThroughAndJoinsErrors(t *testing.T) {
	ctx := context.Background()
	errLocal := errors.New("local unavailable")
	errRemote := errors.New("remote unavailable")
	local := newTestBackend()
	remote := newTestBackend()
	local.putErr = errLocal
	remote.putErr = errRemote
	backend := NewTwoLevelBackend(local, remote, time.Minute)

	err := backend.Put(ctx, "customer.query.list|1|a", []byte("payload"), time.Minute, []string{"customers"})
	if !errors.Is(err, errLocal) {
		t.Fatalf("Put() error = %v, want local error", err)
	}
	if !errors.Is(err, errRemote) {
		t.Fatalf("Put() error = %v, want remote error", err)
	}
	if len(local.puts) != 1 {
		t.Fatalf("local Put() calls = %d, want 1", len(local.puts))
	}
	if len(remote.puts) != 1 {
		t.Fatalf("remote Put() calls = %d, want 1", len(remote.puts))
	}
}

func TestTwoLevelBackendInvalidateQueriesWritesThroughAndJoinsErrors(t *testing.T) {
	ctx := context.Background()
	errLocal := errors.New("local invalidation failed")
	local := newTestBackend()
	remote := newTestBackend()
	local.invalidateQueriesErr = errLocal
	backend := NewTwoLevelBackend(local, remote, time.Minute)

	local.seed("customer.query.list|1|a", "local", nil)
	remote.seed("customer.query.list|1|a", "remote", nil)
	remote.seed("customer.query.detail|1|b", "keep", nil)

	deleted, err := backend.InvalidateQueries(ctx, []string{"customer.query.list"})
	if !errors.Is(err, errLocal) {
		t.Fatalf("InvalidateQueries() error = %v, want local error", err)
	}
	if deleted != 1 {
		t.Fatalf("InvalidateQueries() deleted = %d, want 1", deleted)
	}
	if len(local.invalidateQueriesCalls) != 1 {
		t.Fatalf("local InvalidateQueries() calls = %d, want 1", len(local.invalidateQueriesCalls))
	}
	if len(remote.invalidateQueriesCalls) != 1 {
		t.Fatalf("remote InvalidateQueries() calls = %d, want 1", len(remote.invalidateQueriesCalls))
	}
}

func TestTwoLevelBackendInvalidateTagsClearsBackfilledLocalEntries(t *testing.T) {
	ctx := context.Background()
	local := newTestBackend()
	remote := newTestBackend()
	backend := NewTwoLevelBackend(local, remote, time.Minute)

	remote.seed("customer.query.list|1|a", "remote", []string{"customers"})
	if _, hit, err := backend.Get(ctx, "customer.query.list|1|a"); err != nil || !hit {
		t.Fatalf("Get() = hit %v, err %v; want hit true, err nil", hit, err)
	}

	deleted, err := backend.InvalidateTags(ctx, []string{"customers"})
	if err != nil {
		t.Fatalf("InvalidateTags() error = %v", err)
	}
	if deleted != 2 {
		t.Fatalf("InvalidateTags() deleted = %d, want 2", deleted)
	}
	if got := strings.Join(local.invalidateTagsCalls[0], ","); got != "customers,"+twoLevelBackfillTag {
		t.Fatalf("local InvalidateTags() labels = %q, want customers plus backfill tag", got)
	}
	if _, hit, err := local.Get(ctx, "customer.query.list|1|a"); err != nil || hit {
		t.Fatalf("local Get() after invalidation = hit %v, err %v; want hit false, err nil", hit, err)
	}
	if _, hit, err := remote.Get(ctx, "customer.query.list|1|a"); err != nil || hit {
		t.Fatalf("remote Get() after invalidation = hit %v, err %v; want hit false, err nil", hit, err)
	}
}

func TestTwoLevelBackendStatsCombinesLayersAndKeepsRemoteError(t *testing.T) {
	ctx := context.Background()
	errRemote := errors.New("remote stats failed")
	local := newTestBackend()
	remote := newTestBackend()
	local.stats = QueryStats{Entries: 1, Hits: 2, Misses: 3, Evicts: 4}
	remote.stats = QueryStats{Entries: 10, Hits: 20, Misses: 30, Evicts: 40}
	remote.statsErr = errRemote
	backend := NewTwoLevelBackend(local, remote, time.Minute)

	stats, err := backend.Stats(ctx)
	if !errors.Is(err, errRemote) {
		t.Fatalf("Stats() error = %v, want remote error", err)
	}
	want := QueryStats{Entries: 11, Hits: 22, Misses: 33, Evicts: 44}
	if stats != want {
		t.Fatalf("Stats() = %+v, want %+v", stats, want)
	}
}

type testBackend struct {
	values map[string][]byte
	tags   map[string][]string

	gets                   []string
	puts                   []testBackendPut
	invalidateQueriesCalls [][]string
	invalidateTagsCalls    [][]string

	getErr               error
	putErr               error
	invalidateQueriesErr error
	invalidateTagsErr    error
	statsErr             error
	stats                QueryStats
}

type testBackendPut struct {
	key   string
	value []byte
	ttl   time.Duration
	tags  []string
}

var _ Backend = (*testBackend)(nil)

func newTestBackend() *testBackend {
	return &testBackend{
		values: make(map[string][]byte),
		tags:   make(map[string][]string),
	}
}

func (b *testBackend) seed(key, value string, tags []string) {
	b.values[key] = []byte(value)
	b.tags[key] = append([]string(nil), tags...)
}

func (b *testBackend) Get(ctx context.Context, key string) ([]byte, bool, error) {
	b.gets = append(b.gets, key)
	if b.getErr != nil {
		return nil, false, b.getErr
	}
	value, ok := b.values[key]
	if !ok {
		b.stats.Misses++
		return nil, false, nil
	}
	b.stats.Hits++
	return append([]byte(nil), value...), true, nil
}

func (b *testBackend) Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error {
	b.puts = append(b.puts, testBackendPut{
		key:   key,
		value: append([]byte(nil), value...),
		ttl:   ttl,
		tags:  append([]string(nil), tags...),
	})
	if b.putErr != nil {
		return b.putErr
	}
	b.values[key] = append([]byte(nil), value...)
	b.tags[key] = append([]string(nil), tags...)
	b.stats.Entries = uint64(len(b.values))
	return nil
}

func (b *testBackend) InvalidateQueries(ctx context.Context, names []string) (int, error) {
	b.invalidateQueriesCalls = append(b.invalidateQueriesCalls, append([]string(nil), names...))
	if b.invalidateQueriesErr != nil {
		return 0, b.invalidateQueriesErr
	}

	var deleted int
	for key := range b.values {
		for _, name := range names {
			if name == "" {
				continue
			}
			if strings.HasPrefix(key, name+"|") {
				b.delete(key)
				deleted++
				break
			}
		}
	}
	return deleted, nil
}

func (b *testBackend) InvalidateTags(ctx context.Context, labels []string) (int, error) {
	b.invalidateTagsCalls = append(b.invalidateTagsCalls, append([]string(nil), labels...))
	if b.invalidateTagsErr != nil {
		return 0, b.invalidateTagsErr
	}

	var deleted int
	for key, tags := range b.tags {
		if IntersectTags(tags, labels) {
			b.delete(key)
			deleted++
		}
	}
	return deleted, nil
}

func (b *testBackend) Stats(ctx context.Context) (QueryStats, error) {
	if b.stats.Entries == 0 {
		b.stats.Entries = uint64(len(b.values))
	}
	return b.stats, b.statsErr
}

func (b *testBackend) delete(key string) {
	delete(b.values, key)
	delete(b.tags, key)
	b.stats.Entries = uint64(len(b.values))
}
