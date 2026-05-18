package lazuli

import "time"

const defaultCacheTTL = 60 * time.Second

func normalizeTTL(ttl time.Duration) time.Duration {
	if ttl == 0 {
		return defaultCacheTTL
	}
	if ttl < 0 {
		return 0
	}
	return ttl
}

func expiryAt(now time.Time, ttl time.Duration) time.Time {
	switch {
	case ttl == 0:
		return now.Add(defaultCacheTTL)
	case ttl < 0:
		return time.Time{}
	default:
		return now.Add(ttl)
	}
}
