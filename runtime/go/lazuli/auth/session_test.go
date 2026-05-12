package auth

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

type mockSessionDB struct {
	rows map[string]mockStoredSession
}

type mockStoredSession struct {
	userID    lazuli.ID
	expiresAt time.Time
}

func withMockSessionDB(t *testing.T) *mockSessionDB {
	t.Helper()
	db := &mockSessionDB{rows: make(map[string]mockStoredSession)}
	prev := sessionDBProvider
	sessionDBProvider = func() sessionDB { return db }
	t.Cleanup(func() { sessionDBProvider = prev })
	return db
}

func (db *mockSessionDB) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	switch {
	case strings.HasPrefix(sql, `INSERT INTO "Session"`):
		if len(args) != 3 {
			return pgconn.CommandTag{}, fmt.Errorf("insert arg count = %d", len(args))
		}
		userID, ok := args[0].(int64)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("insert user_id type = %T", args[0])
		}
		tokenHash, ok := args[1].(string)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("insert token_hash type = %T", args[1])
		}
		expiresAt, ok := args[2].(time.Time)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("insert expires_at type = %T", args[2])
		}
		db.rows[tokenHash] = mockStoredSession{userID: lazuli.ID(userID), expiresAt: expiresAt}
		return pgconn.NewCommandTag("INSERT 0 1"), nil
	case strings.HasPrefix(sql, `DELETE FROM "Session"`):
		if len(args) != 1 {
			return pgconn.CommandTag{}, fmt.Errorf("delete arg count = %d", len(args))
		}
		tokenHash, ok := args[0].(string)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("delete token_hash type = %T", args[0])
		}
		affected := int64(0)
		if _, ok := db.rows[tokenHash]; ok {
			affected = 1
		}
		delete(db.rows, tokenHash)
		return pgconn.NewCommandTag(fmt.Sprintf("DELETE %d", affected)), nil
	default:
		return pgconn.CommandTag{}, fmt.Errorf("unexpected exec SQL: %s", sql)
	}
}

func (db *mockSessionDB) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	if !strings.HasPrefix(sql, `SELECT user_id, expires_at FROM "Session"`) {
		return mockSessionRow{err: fmt.Errorf("unexpected query SQL: %s", sql)}
	}
	if len(args) != 1 {
		return mockSessionRow{err: fmt.Errorf("query arg count = %d", len(args))}
	}
	tokenHash, ok := args[0].(string)
	if !ok {
		return mockSessionRow{err: fmt.Errorf("query token_hash type = %T", args[0])}
	}
	row, ok := db.rows[tokenHash]
	if !ok {
		return mockSessionRow{err: pgx.ErrNoRows}
	}
	return mockSessionRow{userID: row.userID, expiresAt: row.expiresAt}
}

type mockSessionRow struct {
	userID    lazuli.ID
	expiresAt time.Time
	err       error
}

func (r mockSessionRow) Scan(dest ...any) error {
	if r.err != nil {
		return r.err
	}
	if len(dest) != 2 {
		return fmt.Errorf("scan dest count = %d", len(dest))
	}
	userID, ok := dest[0].(*int64)
	if !ok {
		return fmt.Errorf("scan user_id dest type = %T", dest[0])
	}
	expiresAt, ok := dest[1].(*time.Time)
	if !ok {
		return fmt.Errorf("scan expires_at dest type = %T", dest[1])
	}
	*userID = int64(r.userID)
	*expiresAt = r.expiresAt
	return nil
}

func TestIssueResolveInvalidateSessionRoundTrip(t *testing.T) {
	db := withMockSessionDB(t)
	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	contract := SessionsContract{Resource: "Session", TTL: "7 days"}

	token, expiresAt, err := IssueSession(ctx, contract, lazuli.ID(42), SessionAttrs{"provider": "password"})
	if err != nil {
		t.Fatalf("IssueSession: %v", err)
	}
	raw, err := base64.RawURLEncoding.DecodeString(token)
	if err != nil {
		t.Fatalf("token must be raw URL-safe base64: %v", err)
	}
	if len(raw) != 32 {
		t.Fatalf("token entropy bytes = %d, want 32", len(raw))
	}
	if want := now.Add(7 * 24 * time.Hour); !expiresAt.Equal(want) {
		t.Fatalf("expiresAt = %v, want %v", expiresAt, want)
	}

	sum := sha256.Sum256([]byte(token))
	tokenHash := hex.EncodeToString(sum[:])
	if _, ok := db.rows[tokenHash]; !ok {
		t.Fatalf("session row stored under SHA-256 token hash")
	}

	userID, attrs, err := ResolveSession(ctx, contract, token)
	if err != nil {
		t.Fatalf("ResolveSession: %v", err)
	}
	if userID != lazuli.ID(42) {
		t.Fatalf("userID = %d, want 42", userID)
	}
	if len(attrs) != 0 {
		t.Fatalf("attrs = %#v, want empty attrs for v0 table schema", attrs)
	}

	if err := InvalidateSession(ctx, contract, token); err != nil {
		t.Fatalf("InvalidateSession: %v", err)
	}
	if _, _, err := ResolveSession(ctx, contract, token); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("ResolveSession after invalidate = %v, want ErrSessionNotFound", err)
	}
}

func TestResolveSessionExpired(t *testing.T) {
	withMockSessionDB(t)
	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	issueCtx := &lazuli.Ctx{Context: context.Background(), Now: now}
	resolveCtx := &lazuli.Ctx{Context: context.Background(), Now: now.Add(2 * time.Hour)}
	contract := SessionsContract{Resource: "Session", TTL: "1 hour"}

	token, _, err := IssueSession(issueCtx, contract, lazuli.ID(7), nil)
	if err != nil {
		t.Fatalf("IssueSession: %v", err)
	}
	if _, _, err := ResolveSession(resolveCtx, contract, token); !errors.Is(err, ErrSessionExpired) {
		t.Fatalf("ResolveSession expired = %v, want ErrSessionExpired", err)
	}
}

func TestIssueSessionDefaultsInvalidTTLTo24Hours(t *testing.T) {
	withMockSessionDB(t)
	now := time.Date(2026, 5, 12, 10, 30, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	contract := SessionsContract{Resource: "Session", TTL: "soon"}

	_, expiresAt, err := IssueSession(ctx, contract, lazuli.ID(1), nil)
	if err != nil {
		t.Fatalf("IssueSession: %v", err)
	}
	if want := now.Add(24 * time.Hour); !expiresAt.Equal(want) {
		t.Fatalf("expiresAt = %v, want %v", expiresAt, want)
	}
}

func TestSessionRejectsInvalidToken(t *testing.T) {
	withMockSessionDB(t)
	ctx := &lazuli.Ctx{Context: context.Background()}
	contract := SessionsContract{Resource: "Session", TTL: time.Hour}

	if _, _, err := ResolveSession(ctx, contract, "not a session token"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("ResolveSession invalid token = %v, want ErrTokenInvalid", err)
	}
	if err := InvalidateSession(ctx, contract, "not a session token"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("InvalidateSession invalid token = %v, want ErrTokenInvalid", err)
	}
}
