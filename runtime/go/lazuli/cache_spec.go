package lazuli

import "time"

var queryCache = newCache(1024)

type CacheSpec struct {
	TTL time.Duration
	Key string
}
