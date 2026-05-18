package lazuli

type CacheStats struct {
	Size     int
	Capacity int
	Hits     uint64
	Misses   uint64
	Evicts   uint64
}

func (c *cache) stats() CacheStats {
	c.mu.Lock()
	defer c.mu.Unlock()
	return CacheStats{
		Size:     c.order.Len(),
		Capacity: c.capacity,
		Hits:     c.hits.Load(),
		Misses:   c.misses.Load(),
		Evicts:   c.evicts.Load(),
	}
}

func Stats() CacheStats { return queryCache.stats() }

func (c *cache) flush() {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.order.Purge()
	for _, store := range c.stores {
		store.Purge()
	}
}

func FlushCache() { queryCache.flush() }
