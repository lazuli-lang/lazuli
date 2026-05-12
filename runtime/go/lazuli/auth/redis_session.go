package auth

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"time"

	"github.com/redis/go-redis/v9"

	"lazuli.dev/runtime/lazuli"
)

const (
	redisSessionDefaultKeyPrefix = "auth:session:"
	redisSessionExpiredKeyPart   = "expired:"
	redisSessionExpiredMarkerTTL = 24 * time.Hour
)

var errRedisSessionClientNotConfigured = errors.New("auth: redis client is not configured")

// RedisSessionStore is a Redis-backed SessionStore.
//
// Sessions are keyed by SHA-256 token hash, not by the raw bearer token. Redis
// TTLs enforce physical expiry for session payloads. A small expiry marker lets
// the first resolve after Redis prunes a payload still return ErrSessionExpired
// instead of treating a formerly valid token as unknown.
type RedisSessionStore struct {
	Client *redis.Client

	// KeyPrefix prefixes Redis keys. Defaults to "auth:session:".
	KeyPrefix string
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time
}

type redisSessionRecord struct {
	UserID    lazuli.ID    `json:"user_id"`
	Attrs     SessionAttrs `json:"attrs,omitempty"`
	ExpiresAt time.Time    `json:"expires_at"`
}

var _ SessionStore = (*RedisSessionStore)(nil)

// NewRedisSessionStore returns a Redis-backed session store using go-redis.
func NewRedisSessionStore(addr, password string, db int) *RedisSessionStore {
	return &RedisSessionStore{
		Client: redis.NewClient(&redis.Options{Addr: addr, Password: password, DB: db}),
	}
}

// Create implements SessionStore.
func (s *RedisSessionStore) Create(
	ctx context.Context,
	userID lazuli.ID,
	ttl time.Duration,
	attrs SessionAttrs,
) (string, time.Time, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return "", time.Time{}, err
	}

	client, err := s.client()
	if err != nil {
		return "", time.Time{}, err
	}
	token, tokenHash, err := newSessionToken()
	if err != nil {
		return "", time.Time{}, err
	}

	expiresIn := sessionTTL(ttl)
	expiresAt := s.now().Add(expiresIn)
	data, err := json.Marshal(redisSessionRecord{
		UserID:    userID,
		Attrs:     cloneSessionAttrs(attrs),
		ExpiresAt: expiresAt,
	})
	if err != nil {
		return "", time.Time{}, err
	}
	rctx := redisSessionContext(ctx)
	pipe := client.Pipeline()
	pipe.Set(rctx, s.sessionKey(tokenHash), data, expiresIn)
	pipe.Set(rctx, s.expiredKey(tokenHash), expiresAt.Format(time.RFC3339Nano), expiresIn+redisSessionExpiredMarkerTTL)
	if _, err := pipe.Exec(rctx); err != nil {
		return "", time.Time{}, err
	}
	return token, expiresAt, nil
}

// Resolve implements SessionStore.
func (s *RedisSessionStore) Resolve(ctx context.Context, token string) (Session, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return Session{}, err
	}

	client, err := s.client()
	if err != nil {
		return Session{}, err
	}
	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return Session{}, err
	}

	rctx := redisSessionContext(ctx)
	data, err := client.Get(rctx, s.sessionKey(tokenHash)).Bytes()
	if errors.Is(err, redis.Nil) {
		return s.resolveMissing(rctx, client, tokenHash)
	}
	if err != nil {
		return Session{}, err
	}

	record, err := decodeRedisSessionRecord(data)
	if err != nil {
		return Session{}, err
	}
	if !record.ExpiresAt.After(s.now()) {
		if err := s.deleteSessionKeys(rctx, client, tokenHash); err != nil {
			return Session{}, err
		}
		return Session{}, ErrSessionExpired
	}

	return Session{
		UserID:    record.UserID,
		Attrs:     cloneSessionAttrs(record.Attrs),
		ExpiresAt: record.ExpiresAt,
	}, nil
}

// Invalidate implements SessionStore.
func (s *RedisSessionStore) Invalidate(ctx context.Context, token string) error {
	if err := sessionStoreContextErr(ctx); err != nil {
		return err
	}

	client, err := s.client()
	if err != nil {
		return err
	}
	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return err
	}
	return s.deleteSessionKeys(redisSessionContext(ctx), client, tokenHash)
}

// CleanupExpired implements SessionStore.
func (s *RedisSessionStore) CleanupExpired(ctx context.Context) (int, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return 0, err
	}

	client, err := s.client()
	if err != nil {
		return 0, err
	}

	rctx := redisSessionContext(ctx)
	now := s.now()
	iter := client.Scan(rctx, 0, redisSessionGlobEscape(s.keyPrefix())+"*", 100).Iterator()
	var deleted int
	for iter.Next(rctx) {
		key := iter.Val()
		if strings.HasPrefix(key, s.expiredKeyPrefix()) {
			continue
		}
		data, err := client.Get(rctx, key).Bytes()
		if errors.Is(err, redis.Nil) {
			continue
		}
		if err != nil {
			return deleted, err
		}

		record, err := decodeRedisSessionRecord(data)
		if err != nil {
			return deleted, err
		}
		if record.ExpiresAt.After(now) {
			continue
		}

		tokenHash := strings.TrimPrefix(key, s.keyPrefix())
		n, err := client.Del(rctx, key, s.expiredKey(tokenHash)).Result()
		if err != nil {
			return deleted, err
		}
		if n > 0 {
			deleted++
		}
	}
	if err := iter.Err(); err != nil {
		return deleted, err
	}
	return deleted, nil
}

func (s *RedisSessionStore) client() (*redis.Client, error) {
	if s == nil || s.Client == nil {
		return nil, errRedisSessionClientNotConfigured
	}
	return s.Client, nil
}

func (s *RedisSessionStore) sessionKey(tokenHash string) string {
	return s.keyPrefix() + tokenHash
}

func (s *RedisSessionStore) expiredKey(tokenHash string) string {
	return s.expiredKeyPrefix() + tokenHash
}

func (s *RedisSessionStore) expiredKeyPrefix() string {
	return s.keyPrefix() + redisSessionExpiredKeyPart
}

func (s *RedisSessionStore) keyPrefix() string {
	if s != nil && s.KeyPrefix != "" {
		return s.KeyPrefix
	}
	return redisSessionDefaultKeyPrefix
}

func (s *RedisSessionStore) now() time.Time {
	if s != nil && s.Clock != nil {
		return s.Clock().UTC()
	}
	return time.Now().UTC()
}

func (s *RedisSessionStore) resolveMissing(ctx context.Context, client *redis.Client, tokenHash string) (Session, error) {
	if _, err := client.Get(ctx, s.expiredKey(tokenHash)).Result(); errors.Is(err, redis.Nil) {
		return Session{}, ErrSessionNotFound
	} else if err != nil {
		return Session{}, err
	}
	if err := client.Del(ctx, s.expiredKey(tokenHash)).Err(); err != nil {
		return Session{}, err
	}
	return Session{}, ErrSessionExpired
}

func (s *RedisSessionStore) deleteSessionKeys(ctx context.Context, client *redis.Client, tokenHash string) error {
	return client.Del(ctx, s.sessionKey(tokenHash), s.expiredKey(tokenHash)).Err()
}

func decodeRedisSessionRecord(data []byte) (redisSessionRecord, error) {
	var record redisSessionRecord
	if err := json.Unmarshal(data, &record); err != nil {
		return redisSessionRecord{}, err
	}
	return record, nil
}

func redisSessionContext(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	return ctx
}

func redisSessionGlobEscape(s string) string {
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
