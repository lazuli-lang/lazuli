package auth

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestLoginPasswordIssuesAuthSession(t *testing.T) {
	db := withMockSessionDB(t)
	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	passwords := PasswordContract{Algorithm: AlgoBcrypt}
	sessions := SessionsContract{Resource: "Session", TTL: 2 * time.Hour}
	hash := mustHashLoginPassword(t, ctx, passwords, "correct-password")

	var gotIdentity string
	lookup := func(ctx *lazuli.Ctx, identity string) (PasswordLoginSubject, error) {
		gotIdentity = identity
		return PasswordLoginSubject{
			UserID:       lazuli.ID(42),
			PasswordHash: hash,
			Attrs: SessionAttrs{
				"scope": "customer",
			},
		}, nil
	}

	session, err := LoginPassword(ctx, passwords, sessions, PasswordLoginInput{
		Identity: "alice@example.com",
		Password: "correct-password",
	}, lookup)
	if err != nil {
		t.Fatalf("LoginPassword: %v", err)
	}
	if gotIdentity != "alice@example.com" {
		t.Fatalf("lookup identity = %q, want alice@example.com", gotIdentity)
	}
	if session.UserID != lazuli.ID(42) {
		t.Fatalf("UserID = %d, want 42", session.UserID)
	}
	if session.CookieName != CookieName {
		t.Fatalf("CookieName = %q, want %q", session.CookieName, CookieName)
	}
	if session.SessionToken == "" {
		t.Fatalf("SessionToken must be set")
	}
	if want := now.Add(2 * time.Hour); !session.ExpiresAt.Equal(want) {
		t.Fatalf("ExpiresAt = %v, want %v", session.ExpiresAt, want)
	}
	if session.Attrs["provider"] != "password" {
		t.Fatalf("Attrs[provider] = %#v, want password", session.Attrs["provider"])
	}
	if session.Attrs["scope"] != "customer" {
		t.Fatalf("Attrs[scope] = %#v, want customer", session.Attrs["scope"])
	}

	userID, _, err := ResolveSession(ctx, sessions, session.SessionToken)
	if err != nil {
		t.Fatalf("ResolveSession issued token: %v", err)
	}
	if userID != lazuli.ID(42) {
		t.Fatalf("resolved userID = %d, want 42", userID)
	}
	if len(db.rows) != 1 {
		t.Fatalf("stored sessions = %d, want 1", len(db.rows))
	}

	encoded, err := json.Marshal(session)
	if err != nil {
		t.Fatalf("Marshal AuthSession: %v", err)
	}
	var body map[string]any
	if err := json.Unmarshal(encoded, &body); err != nil {
		t.Fatalf("Unmarshal AuthSession JSON: %v", err)
	}
	if _, ok := body["session_token"]; ok {
		t.Fatalf("AuthSession JSON must not expose session_token: %s", encoded)
	}
	if body["cookie_name"] != CookieName {
		t.Fatalf("cookie_name = %#v, want %q", body["cookie_name"], CookieName)
	}
}

func TestLoginPasswordRejectsWrongPasswordWithoutSession(t *testing.T) {
	db := withMockSessionDB(t)
	ctx := &lazuli.Ctx{Context: context.Background()}
	passwords := PasswordContract{Algorithm: AlgoBcrypt}
	sessions := SessionsContract{Resource: "Session", TTL: time.Hour}
	hash := mustHashLoginPassword(t, ctx, passwords, "correct-password")

	lookup := func(ctx *lazuli.Ctx, identity string) (PasswordLoginSubject, error) {
		return PasswordLoginSubject{
			UserID:       lazuli.ID(7),
			PasswordHash: hash,
		}, nil
	}

	_, err := LoginPassword(ctx, passwords, sessions, PasswordLoginInput{
		Identity: "alice@example.com",
		Password: "wrong-password",
	}, lookup)
	if !errors.Is(err, ErrPasswordMismatch) {
		t.Fatalf("LoginPassword wrong password error = %v, want ErrPasswordMismatch", err)
	}
	if len(db.rows) != 0 {
		t.Fatalf("stored sessions = %d, want 0", len(db.rows))
	}
}

func TestLoginPasswordPropagatesUnknownIdentityAsPasswordMismatch(t *testing.T) {
	db := withMockSessionDB(t)
	ctx := &lazuli.Ctx{Context: context.Background()}
	passwords := PasswordContract{Algorithm: AlgoBcrypt}
	sessions := SessionsContract{Resource: "Session", TTL: time.Hour}

	lookup := func(ctx *lazuli.Ctx, identity string) (PasswordLoginSubject, error) {
		return PasswordLoginSubject{}, ErrPasswordMismatch
	}

	_, err := LoginPassword(ctx, passwords, sessions, PasswordLoginInput{
		Identity: "nobody@example.com",
		Password: "whatever",
	}, lookup)
	if !errors.Is(err, ErrPasswordMismatch) {
		t.Fatalf("LoginPassword unknown identity error = %v, want ErrPasswordMismatch", err)
	}
	if len(db.rows) != 0 {
		t.Fatalf("stored sessions = %d, want 0", len(db.rows))
	}
}

func mustHashLoginPassword(
	t *testing.T,
	ctx *lazuli.Ctx,
	contract PasswordContract,
	plaintext string,
) string {
	t.Helper()
	hash, err := HashPassword(ctx, contract, plaintext)
	if err != nil {
		t.Fatalf("HashPassword: %v", err)
	}
	return hash
}
