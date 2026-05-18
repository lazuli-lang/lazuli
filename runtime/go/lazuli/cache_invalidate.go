package lazuli

func (c *cache) invalidateQueries(names []string) int {
	if len(names) == 0 {
		return 0
	}
	want := make(map[string]struct{}, len(names))
	for _, name := range names {
		want[name] = struct{}{}
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	var dropped int
	for _, key := range c.order.Keys() {
		entry, ok := c.order.Peek(key)
		if !ok {
			continue
		}
		store := c.stores[entry.ttl]
		if !ok || store == nil {
			continue
		}
		entry, ok = store.Peek(key)
		if !ok {
			continue
		}
		if _, hit := want[entry.queryName]; hit {
			c.removeKeyLocked(key, entry.ttl)
			dropped++
		}
	}
	return dropped
}
