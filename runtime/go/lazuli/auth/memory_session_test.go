package auth

import (
	"context"
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestMemorySessionStoreCreateResolveInvalidate(t *testing.T) {
	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	store := NewMemorySessionStore()
	store.Clock = func() time.Time { return now }

	roles := []string{"admin"}
	meta := map[string]any{"tier": "pro"}
	attrs := SessionAttrs{
		"provider": "password",
		"roles":    roles,
		"meta":     meta,
	}

	token, expiresAt, err := store.Create(context.Background(), lazuli.ID(42), time.Hour, attrs)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}
	if token == "" {
		t.Fatal("Create() token is empty")
	}
	if want := now.Add(time.Hour); !expiresAt.Equal(want) {
		t.Fatalf("Create() expiresAt = %v, want %v", expiresAt, want)
	}

	attrs["provider"] = "oauth"
	roles[0] = "viewer"
	meta["tier"] = "free"

	session, err := store.Resolve(context.Background(), token)
	if err != nil {
		t.Fatalf("Resolve() error = %v", err)
	}
	if session.UserID != lazuli.ID(42) {
		t.Fatalf("Resolve() UserID = %d, want 42", session.UserID)
	}
	if !session.ExpiresAt.Equal(expiresAt) {
		t.Fatalf("Resolve() ExpiresAt = %v, want %v", session.ExpiresAt, expiresAt)
	}
	if got := session.Attrs["provider"]; got != "password" {
		t.Fatalf("Resolve() provider attr = %v, want password", got)
	}
	if got := session.Attrs["roles"].([]string)[0]; got != "admin" {
		t.Fatalf("Resolve() roles attr = %v, want admin", got)
	}
	if got := session.Attrs["meta"].(map[string]any)["tier"]; got != "pro" {
		t.Fatalf("Resolve() meta attr = %v, want pro", got)
	}

	session.Attrs["provider"] = "mutated"
	session.Attrs["roles"].([]string)[0] = "mutated"
	session.Attrs["meta"].(map[string]any)["tier"] = "mutated"

	session, err = store.Resolve(context.Background(), token)
	if err != nil {
		t.Fatalf("Resolve() after attr mutation error = %v", err)
	}
	if got := session.Attrs["provider"]; got != "password" {
		t.Fatalf("Resolve() after attr mutation provider = %v, want password", got)
	}
	if got := session.Attrs["roles"].([]string)[0]; got != "admin" {
		t.Fatalf("Resolve() after attr mutation roles = %v, want admin", got)
	}
	if got := session.Attrs["meta"].(map[string]any)["tier"]; got != "pro" {
		t.Fatalf("Resolve() after attr mutation meta = %v, want pro", got)
	}

	if err := store.Invalidate(context.Background(), token); err != nil {
		t.Fatalf("Invalidate() error = %v", err)
	}
	if _, err := store.Resolve(context.Background(), token); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("Resolve() after invalidate error = %v, want ErrSessionNotFound", err)
	}
}

func TestMemorySessionStoreResolveExpired(t *testing.T) {
	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	store := NewMemorySessionStore()
	store.Clock = func() time.Time { return now }

	token, _, err := store.Create(context.Background(), lazuli.ID(7), time.Second, nil)
	if err != nil {
		t.Fatalf("Create() error = %v", err)
	}

	now = now.Add(2 * time.Second)
	if _, err := store.Resolve(context.Background(), token); !errors.Is(err, ErrSessionExpired) {
		t.Fatalf("Resolve() expired error = %v, want ErrSessionExpired", err)
	}
	if _, err := store.Resolve(context.Background(), token); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("Resolve() expired after prune error = %v, want ErrSessionNotFound", err)
	}
}

func TestMemorySessionStoreCleanupExpired(t *testing.T) {
	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	store := &MemorySessionStore{}
	store.Clock = func() time.Time { return now }

	expiredToken, _, err := store.Create(context.Background(), lazuli.ID(1), time.Second, nil)
	if err != nil {
		t.Fatalf("Create() expired session error = %v", err)
	}
	activeToken, _, err := store.Create(context.Background(), lazuli.ID(2), time.Hour, nil)
	if err != nil {
		t.Fatalf("Create() active session error = %v", err)
	}

	now = now.Add(2 * time.Second)
	deleted, err := store.CleanupExpired(context.Background())
	if err != nil {
		t.Fatalf("CleanupExpired() error = %v", err)
	}
	if deleted != 1 {
		t.Fatalf("CleanupExpired() deleted = %d, want 1", deleted)
	}
	if _, err := store.Resolve(context.Background(), expiredToken); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("Resolve() cleaned token error = %v, want ErrSessionNotFound", err)
	}
	if _, err := store.Resolve(context.Background(), activeToken); err != nil {
		t.Fatalf("Resolve() active token error = %v", err)
	}

	now = now.Add(time.Hour)
	deleted, err = store.CleanupExpired(context.Background())
	if err != nil {
		t.Fatalf("CleanupExpired() second error = %v", err)
	}
	if deleted != 1 {
		t.Fatalf("CleanupExpired() second deleted = %d, want 1", deleted)
	}
}

func TestMemorySessionStoreRejectsInvalidToken(t *testing.T) {
	store := NewMemorySessionStore()

	if _, err := store.Resolve(context.Background(), "not a session token"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("Resolve() invalid token error = %v, want ErrTokenInvalid", err)
	}
	if err := store.Invalidate(context.Background(), "not a session token"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("Invalidate() invalid token error = %v, want ErrTokenInvalid", err)
	}
}

func TestMemorySessionStoreReturnsContextErrors(t *testing.T) {
	store := NewMemorySessionStore()
	token, _, err := store.Create(context.Background(), lazuli.ID(1), time.Hour, nil)
	if err != nil {
		t.Fatalf("Create() setup error = %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, _, err := store.Create(ctx, lazuli.ID(2), time.Hour, nil); !errors.Is(err, context.Canceled) {
		t.Fatalf("Create() canceled error = %v, want context.Canceled", err)
	}
	if _, err := store.Resolve(ctx, token); !errors.Is(err, context.Canceled) {
		t.Fatalf("Resolve() canceled error = %v, want context.Canceled", err)
	}
	if err := store.Invalidate(ctx, token); !errors.Is(err, context.Canceled) {
		t.Fatalf("Invalidate() canceled error = %v, want context.Canceled", err)
	}
	if _, err := store.CleanupExpired(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("CleanupExpired() canceled error = %v, want context.Canceled", err)
	}
}
