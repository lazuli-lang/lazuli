package auth

import (
	"context"
	"sync"
	"time"

	"lazuli.dev/runtime/lazuli"
)

// Session is the resolved server-side state for an auth session.
type Session struct {
	// UserID is the Lazuli user identity bound to the session.
	UserID lazuli.ID
	// Attrs carries provider-neutral session metadata.
	Attrs SessionAttrs
	// ExpiresAt is the time after which the session is no longer valid.
	ExpiresAt time.Time
}

// SessionStore is the provider-neutral storage contract for auth sessions.
//
// Implementations must be safe for concurrent use. Tokens are opaque to
// callers; stores may hash them before persistence.
type SessionStore interface {
	// Create stores a new session and returns the opaque token plus expiry time.
	Create(ctx context.Context, userID lazuli.ID, ttl time.Duration, attrs SessionAttrs) (token string, expiresAt time.Time, err error)
	// Resolve returns the unexpired session bound to token.
	Resolve(ctx context.Context, token string) (Session, error)
	// Invalidate removes token if present.
	Invalidate(ctx context.Context, token string) error
	// CleanupExpired removes expired sessions and returns the number deleted.
	CleanupExpired(ctx context.Context) (int, error)
}

// MemorySessionStore is an in-process SessionStore for development and tests.
//
// The zero value is ready to use. Sessions are keyed by SHA-256 token hash, not
// by the raw bearer token.
type MemorySessionStore struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu       sync.Mutex
	sessions map[string]memorySession
}

type memorySession struct {
	userID    lazuli.ID
	attrs     SessionAttrs
	expiresAt time.Time
}

var _ SessionStore = (*MemorySessionStore)(nil)

// NewMemorySessionStore returns an empty in-process session store.
func NewMemorySessionStore() *MemorySessionStore {
	return &MemorySessionStore{
		sessions: make(map[string]memorySession),
	}
}

// Create implements SessionStore.
func (s *MemorySessionStore) Create(
	ctx context.Context,
	userID lazuli.ID,
	ttl time.Duration,
	attrs SessionAttrs,
) (string, time.Time, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return "", time.Time{}, err
	}

	token, tokenHash, err := newSessionToken()
	if err != nil {
		return "", time.Time{}, err
	}
	expiresAt := s.now().Add(sessionTTL(ttl))

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	s.sessions[tokenHash] = memorySession{
		userID:    userID,
		attrs:     cloneSessionAttrs(attrs),
		expiresAt: expiresAt,
	}
	return token, expiresAt, nil
}

// Resolve implements SessionStore.
func (s *MemorySessionStore) Resolve(ctx context.Context, token string) (Session, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return Session{}, err
	}

	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return Session{}, err
	}
	now := s.now()

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	session, ok := s.sessions[tokenHash]
	if !ok {
		return Session{}, ErrSessionNotFound
	}
	if !session.expiresAt.After(now) {
		delete(s.sessions, tokenHash)
		return Session{}, ErrSessionExpired
	}

	return Session{
		UserID:    session.userID,
		Attrs:     cloneSessionAttrs(session.attrs),
		ExpiresAt: session.expiresAt,
	}, nil
}

// Invalidate implements SessionStore.
func (s *MemorySessionStore) Invalidate(ctx context.Context, token string) error {
	if err := sessionStoreContextErr(ctx); err != nil {
		return err
	}

	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	delete(s.sessions, tokenHash)
	return nil
}

// CleanupExpired implements SessionStore.
func (s *MemorySessionStore) CleanupExpired(ctx context.Context) (int, error) {
	if err := sessionStoreContextErr(ctx); err != nil {
		return 0, err
	}
	now := s.now()

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	var deleted int
	for tokenHash, session := range s.sessions {
		if !session.expiresAt.After(now) {
			delete(s.sessions, tokenHash)
			deleted++
		}
	}
	return deleted, nil
}

func (s *MemorySessionStore) ensureLocked() {
	if s.sessions == nil {
		s.sessions = make(map[string]memorySession)
	}
}

func (s *MemorySessionStore) now() time.Time {
	if s != nil && s.Clock != nil {
		return s.Clock().UTC()
	}
	return time.Now().UTC()
}

func sessionStoreContextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}

func cloneSessionAttrs(attrs SessionAttrs) SessionAttrs {
	if len(attrs) == 0 {
		return SessionAttrs{}
	}

	cloned := make(SessionAttrs, len(attrs))
	for key, value := range attrs {
		cloned[key] = cloneSessionAttrValue(value)
	}
	return cloned
}

func cloneSessionAttrValue(value any) any {
	switch v := value.(type) {
	case []byte:
		return append([]byte(nil), v...)
	case []string:
		return append([]string(nil), v...)
	case []any:
		cloned := make([]any, len(v))
		for i, item := range v {
			cloned[i] = cloneSessionAttrValue(item)
		}
		return cloned
	case map[string]any:
		return cloneSessionAttrs(v)
	default:
		return v
	}
}
