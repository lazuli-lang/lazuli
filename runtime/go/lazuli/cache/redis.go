package cache

import (
	"context"
	"errors"
	"strings"
	"time"

	"github.com/redis/go-redis/v9"
)

const (
	redisDefaultTTL = time.Minute
	redisTagPrefix  = "tag:"
)

var errRedisClientNotConfigured = errors.New("lazuli/cache: redis client is not configured")

// RedisBackend is the Redis-backed Cache adapter.
type RedisBackend struct {
	Client *redis.Client
}

var _ Backend = (*RedisBackend)(nil)

func NewRedisBackend(addr, password string, db int) *RedisBackend {
	return &RedisBackend{
		Client: redis.NewClient(&redis.Options{Addr: addr, Password: password, DB: db}),
	}
}

func (b *RedisBackend) Get(ctx context.Context, key string) ([]byte, bool, error) {
	client, err := b.client()
	if err != nil {
		return nil, false, err
	}
	value, err := client.Get(ctx, key).Bytes()
	if errors.Is(err, redis.Nil) {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	return value, true, nil
}

func (b *RedisBackend) Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error {
	client, err := b.client()
	if err != nil {
		return err
	}
	pipe := client.Pipeline()
	pipe.Set(ctx, key, value, redisExpiration(ttl))
	for _, tag := range tags {
		if tag == "" {
			continue
		}
		pipe.SAdd(ctx, redisTagKey(tag), key)
	}
	_, err = pipe.Exec(ctx)
	return err
}

func (b *RedisBackend) Set(ctx context.Context, key string, value []byte, ttl time.Duration) error {
	return b.Put(ctx, key, value, ttl, nil)
}

func (b *RedisBackend) Delete(ctx context.Context, key string) error {
	client, err := b.client()
	if err != nil {
		return err
	}
	return client.Del(ctx, key).Err()
}

func (b *RedisBackend) DeleteByTag(ctx context.Context, tag string) error {
	_, err := b.InvalidateTags(ctx, []string{tag})
	return err
}

func (b *RedisBackend) DeleteByPrefix(ctx context.Context, prefix string) error {
	_, err := b.deleteByPrefix(ctx, prefix)
	return err
}

func (b *RedisBackend) InvalidateQueries(ctx context.Context, names []string) (int, error) {
	var deleted int
	for _, name := range names {
		if name == "" {
			continue
		}
		n, err := b.deleteByPrefix(ctx, name+"|")
		if err != nil {
			return deleted, err
		}
		deleted += n
	}
	return deleted, nil
}

func (b *RedisBackend) InvalidateTags(ctx context.Context, labels []string) (int, error) {
	client, err := b.client()
	if err != nil {
		return 0, err
	}

	var deleted int
	for _, label := range labels {
		if label == "" {
			continue
		}
		tagKey := redisTagKey(label)
		keys, err := client.SMembers(ctx, tagKey).Result()
		if err != nil {
			return deleted, err
		}
		if len(keys) == 0 {
			if err := client.Del(ctx, tagKey).Err(); err != nil {
				return deleted, err
			}
			continue
		}

		pipe := client.Pipeline()
		cacheDel := pipe.Del(ctx, keys...)
		pipe.Del(ctx, tagKey)
		if _, err := pipe.Exec(ctx); err != nil {
			return deleted, err
		}
		deleted += int(cacheDel.Val())
	}
	return deleted, nil
}

func (b *RedisBackend) Stats(ctx context.Context) (QueryStats, error) {
	client, err := b.client()
	if err != nil {
		return QueryStats{}, err
	}

	var entries uint64
	iter := client.Scan(ctx, 0, "*", 100).Iterator()
	for iter.Next(ctx) {
		if !strings.HasPrefix(iter.Val(), redisTagPrefix) {
			entries++
		}
	}
	if err := iter.Err(); err != nil {
		return QueryStats{}, err
	}
	return QueryStats{Entries: entries}, nil
}

func (b *RedisBackend) deleteByPrefix(ctx context.Context, prefix string) (int, error) {
	client, err := b.client()
	if err != nil {
		return 0, err
	}

	pattern := redisGlobEscape(prefix) + "*"
	iter := client.Scan(ctx, 0, pattern, 100).Iterator()
	batch := make([]string, 0, 100)
	var deleted int
	for iter.Next(ctx) {
		batch = append(batch, iter.Val())
		if len(batch) == cap(batch) {
			n, err := deleteRedisKeys(ctx, client, batch)
			if err != nil {
				return deleted, err
			}
			deleted += n
			batch = batch[:0]
		}
	}
	if err := iter.Err(); err != nil {
		return deleted, err
	}
	if len(batch) > 0 {
		n, err := deleteRedisKeys(ctx, client, batch)
		if err != nil {
			return deleted, err
		}
		deleted += n
	}
	return deleted, nil
}

func (b *RedisBackend) client() (*redis.Client, error) {
	if b == nil || b.Client == nil {
		return nil, errRedisClientNotConfigured
	}
	return b.Client, nil
}

func deleteRedisKeys(ctx context.Context, client *redis.Client, keys []string) (int, error) {
	if len(keys) == 0 {
		return 0, nil
	}
	deleted, err := client.Del(ctx, keys...).Result()
	return int(deleted), err
}

func redisExpiration(ttl time.Duration) time.Duration {
	switch {
	case ttl < 0:
		return 0
	case ttl == 0:
		return redisDefaultTTL
	default:
		return ttl
	}
}

func redisTagKey(label string) string {
	return redisTagPrefix + label
}

func redisGlobEscape(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		switch r {
		case '\\', '*', '?', '[', ']':
			b.WriteByte('\\')
		}
		b.WriteRune(r)
	}
	return b.String()
}
