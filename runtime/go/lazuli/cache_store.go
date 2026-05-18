package lazuli

import (
	"sync"
	"sync/atomic"
	"time"

	lru "github.com/hashicorp/golang-lru/v2"
	"github.com/hashicorp/golang-lru/v2/expirable"
)

type cache struct {
	capacity int
	mu       sync.Mutex
	order    *lru.Cache[string, *cacheEntry]
	stores   map[time.Duration]*expirable.LRU[string, *cacheEntry]
	hits     atomic.Uint64
	misses   atomic.Uint64
	evicts   atomic.Uint64
}

type cacheEntry struct {
	queryName string
	value     any
	expiresAt time.Time
	ttl       time.Duration
}

func newCacheStores() map[time.Duration]*expirable.LRU[string, *cacheEntry] {
	return make(map[time.Duration]*expirable.LRU[string, *cacheEntry])
}

func (c *cache) storeLocked(ttl time.Duration) *expirable.LRU[string, *cacheEntry] {
	store := c.stores[ttl]
	if store == nil {
		store = expirable.NewLRU[string, *cacheEntry](c.capacity, nil, ttl)
		c.stores[ttl] = store
	}
	return store
}

func (c *cache) removeKeyLocked(key string, ttl time.Duration) {
	c.order.Remove(key)
	if store := c.stores[ttl]; store != nil {
		store.Remove(key)
	}
}
