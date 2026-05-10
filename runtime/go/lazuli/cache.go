package lazuli

import (
	"container/list"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"strconv"
	"sync"
	"time"
)

// CacheSpec declares per-query cache behaviour. Mirrors the DSL `cache key
// "..." ttl 5m` block. A zero CacheSpec on a Query disables caching.
type CacheSpec struct {
	// TTL bounds how long a cached entry is served. Zero falls back to the
	// runtime default (60s); negative values disable expiry.
	TTL time.Duration

	// Key is a stable label used in logs and for manual eviction. The
	// effective cache key always includes query name + tenant + args hash;
	// Key is informational only.
	Key string
}

// queryCache is the process-global cache. Entries are keyed by
// `<query name>|<org id>|<args hash>` so tenants never collide. The cache
// is LRU-bounded (oldest entry evicted when capacity is exceeded) and
// optionally TTL-bounded (entry expires after CacheSpec.TTL).
//
// Invalidates fire from successful commands: the cache wipes every entry
// whose query name matches a `c.Invalidates` entry.
var queryCache = newCache(1024)

type cacheEntry struct {
	key       string
	queryName string
	value     any
	expiresAt time.Time // zero => no expiry
}

type cache struct {
	mu       sync.Mutex
	capacity int
	items    map[string]*list.Element // key -> element holding *cacheEntry
	lru      *list.List               // front = most recent
	hits     uint64
	misses   uint64
	evicts   uint64
}

func newCache(capacity int) *cache {
	if capacity <= 0 {
		capacity = 1024
	}
	return &cache{
		capacity: capacity,
		items:    make(map[string]*list.Element, capacity),
		lru:      list.New(),
	}
}

// get returns the cached value for key if present and unexpired. The hit
// promotes the entry to the front of the LRU list.
func (c *cache) get(key string, now time.Time) (any, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	el, ok := c.items[key]
	if !ok {
		c.misses++
		return nil, false
	}
	entry := el.Value.(*cacheEntry)
	if !entry.expiresAt.IsZero() && now.After(entry.expiresAt) {
		c.removeElement(el)
		c.misses++
		return nil, false
	}
	c.lru.MoveToFront(el)
	c.hits++
	return entry.value, true
}

// put stores a value at key, evicting the LRU tail if over capacity.
func (c *cache) put(key, queryName string, value any, ttl time.Duration, now time.Time) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if el, ok := c.items[key]; ok {
		entry := el.Value.(*cacheEntry)
		entry.value = value
		entry.queryName = queryName
		entry.expiresAt = expiryAt(now, ttl)
		c.lru.MoveToFront(el)
		return
	}
	entry := &cacheEntry{
		key:       key,
		queryName: queryName,
		value:     value,
		expiresAt: expiryAt(now, ttl),
	}
	c.items[key] = c.lru.PushFront(entry)
	if c.lru.Len() > c.capacity {
		c.removeElement(c.lru.Back())
		c.evicts++
	}
}

// invalidateQueries removes every entry whose queryName matches one of the
// provided names. Returns the number of entries dropped.
func (c *cache) invalidateQueries(names []string) int {
	if len(names) == 0 {
		return 0
	}
	want := make(map[string]struct{}, len(names))
	for _, n := range names {
		want[n] = struct{}{}
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	var dropped int
	for el := c.lru.Front(); el != nil; {
		next := el.Next()
		entry := el.Value.(*cacheEntry)
		if _, hit := want[entry.queryName]; hit {
			c.removeElement(el)
			dropped++
		}
		el = next
	}
	return dropped
}

// stats returns a snapshot of cache counters for observability/tests.
type CacheStats struct {
	Size     int
	Capacity int
	Hits     uint64
	Misses   uint64
	Evicts   uint64
}

// Stats returns a snapshot of the global query cache for observability.
func (c *cache) stats() CacheStats {
	c.mu.Lock()
	defer c.mu.Unlock()
	return CacheStats{
		Size:     c.lru.Len(),
		Capacity: c.capacity,
		Hits:     c.hits,
		Misses:   c.misses,
		Evicts:   c.evicts,
	}
}

// Stats returns the global query-cache counters. Exposed for /healthz-style
// observability and tests.
func Stats() CacheStats { return queryCache.stats() }

// flush wipes every entry. Used by tests; not part of the runtime hot path.
func (c *cache) flush() {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.items = make(map[string]*list.Element, c.capacity)
	c.lru.Init()
}

// FlushCache empties the global query cache. Tests use this to isolate runs.
func FlushCache() { queryCache.flush() }

// removeElement detaches an LRU element from both the list and the items map.
// Caller must hold c.mu.
func (c *cache) removeElement(el *list.Element) {
	entry := el.Value.(*cacheEntry)
	delete(c.items, entry.key)
	c.lru.Remove(el)
}

func expiryAt(now time.Time, ttl time.Duration) time.Time {
	switch {
	case ttl == 0:
		return now.Add(60 * time.Second)
	case ttl < 0:
		return time.Time{}
	default:
		return now.Add(ttl)
	}
}

// cacheKeyFor builds the canonical cache key for a query invocation. The
// key includes query name + tenant + args hash so tenants never collide and
// distinct argument sets never share a cache slot.
func cacheKeyFor(ctx *Ctx, queryName string, args any) (string, error) {
	tenant := "-"
	if ctx != nil && ctx.Tenant != nil {
		tenant = strconv.FormatInt(int64(ctx.Tenant.OrgID), 10)
	}
	argsHash, err := hashArgs(args)
	if err != nil {
		return "", err
	}
	return queryName + "|" + tenant + "|" + argsHash, nil
}

// hashArgs returns a stable SHA-256 of the JSON-encoded args. Argument
// shapes are author-defined plain Go structs, so json.Marshal is the
// canonical serialisation. The hash deduplicates equivalent calls (e.g.
// same Filters with same paginate cursor).
func hashArgs(args any) (string, error) {
	buf, err := json.Marshal(args)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(buf)
	return hex.EncodeToString(sum[:]), nil
}
