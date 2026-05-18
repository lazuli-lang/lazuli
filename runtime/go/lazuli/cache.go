package lazuli

import (
	"time"

	lru "github.com/hashicorp/golang-lru/v2"
)

func newCache(capacity int) *cache {
	if capacity <= 0 {
		capacity = 1024
	}
	order, err := lru.New[string, *cacheEntry](capacity)
	if err != nil {
		panic(err)
	}
	return &cache{capacity: capacity, order: order, stores: newCacheStores()}
}

func (c *cache) get(key string, now time.Time) (any, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	entry, ok := c.order.Get(key)
	if !ok {
		c.misses.Add(1)
		return nil, false
	}
	store := c.stores[entry.ttl]
	if store == nil {
		c.order.Remove(key)
		c.misses.Add(1)
		return nil, false
	}
	entry, ok = store.Get(key)
	if !ok {
		c.order.Remove(key)
		c.misses.Add(1)
		return nil, false
	}
	if !entry.expiresAt.IsZero() && now.After(entry.expiresAt) {
		c.removeKeyLocked(key, entry.ttl)
		c.misses.Add(1)
		return nil, false
	}
	c.hits.Add(1)
	return entry.value, true
}

func (c *cache) put(key, queryName string, value any, ttl time.Duration, now time.Time) {
	c.mu.Lock()
	defer c.mu.Unlock()
	expiresAt := expiryAt(now, ttl)
	ttl = normalizeTTL(ttl)
	if old, ok := c.order.Peek(key); ok && old.ttl != ttl {
		c.removeKeyLocked(key, old.ttl)
	}
	if _, ok := c.order.Peek(key); !ok && c.order.Len() >= c.capacity {
		oldKey, old, ok := c.order.GetOldest()
		if ok {
			c.removeKeyLocked(oldKey, old.ttl)
		}
		c.evicts.Add(1)
	}
	entry := &cacheEntry{queryName: queryName, value: value, expiresAt: expiresAt, ttl: ttl}
	c.order.Add(key, entry)
	c.storeLocked(ttl).Add(key, entry)
}
