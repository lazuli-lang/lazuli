package auth

import (
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

type mockPasswordResetDB struct {
	users  map[string]mockPasswordResetUser
	tokens map[string]mockPasswordResetTokenRow
}

type mockPasswordResetUser struct {
	id           lazuli.ID
	passwordHash string
}

type mockPasswordResetTokenRow struct {
	userID    lazuli.ID
	expiresAt time.Time
	usedAt    *time.Time
}

func withMockPasswordResetDB(t *testing.T) *mockPasswordResetDB {
	t.Helper()
	db := &mockPasswordResetDB{
		users:  make(map[string]mockPasswordResetUser),
		tokens: make(map[string]mockPasswordResetTokenRow),
	}
	prev := passwordResetDBProvider
	passwordResetDBProvider = func() passwordResetDB { return db }
	t.Cleanup(func() { passwordResetDBProvider = prev })
	return db
}

func (db *mockPasswordResetDB) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	switch {
	case strings.HasPrefix(sql, `INSERT INTO "password_reset"`):
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
		db.tokens[tokenHash] = mockPasswordResetTokenRow{userID: lazuli.ID(userID), expiresAt: expiresAt}
		return pgconn.NewCommandTag("INSERT 0 1"), nil
	case strings.HasPrefix(sql, `UPDATE "user" SET password_hash`):
		if len(args) != 2 {
			return pgconn.CommandTag{}, fmt.Errorf("update user arg count = %d", len(args))
		}
		passwordHash, ok := args[0].(string)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("update password_hash type = %T", args[0])
		}
		userID, ok := args[1].(int64)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("update user id type = %T", args[1])
		}
		for email, user := range db.users {
			if user.id == lazuli.ID(userID) {
				user.passwordHash = passwordHash
				db.users[email] = user
				return pgconn.NewCommandTag("UPDATE 1"), nil
			}
		}
		return pgconn.NewCommandTag("UPDATE 0"), nil
	case strings.HasPrefix(sql, `UPDATE "password_reset" SET used_at`):
		if len(args) != 2 {
			return pgconn.CommandTag{}, fmt.Errorf("update token arg count = %d", len(args))
		}
		usedAt, ok := args[0].(time.Time)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("used_at type = %T", args[0])
		}
		tokenHash, ok := args[1].(string)
		if !ok {
			return pgconn.CommandTag{}, fmt.Errorf("update token_hash type = %T", args[1])
		}
		row, ok := db.tokens[tokenHash]
		if !ok {
			return pgconn.NewCommandTag("UPDATE 0"), nil
		}
		row.usedAt = &usedAt
		db.tokens[tokenHash] = row
		return pgconn.NewCommandTag("UPDATE 1"), nil
	default:
		return pgconn.CommandTag{}, fmt.Errorf("unexpected exec SQL: %s", sql)
	}
}

func (db *mockPasswordResetDB) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	switch {
	case strings.HasPrefix(sql, `SELECT id FROM "user"`):
		if len(args) != 1 {
			return mockPasswordResetRow{err: fmt.Errorf("lookup arg count = %d", len(args))}
		}
		email, ok := args[0].(string)
		if !ok {
			return mockPasswordResetRow{err: fmt.Errorf("lookup email type = %T", args[0])}
		}
		user, ok := db.users[email]
		if !ok {
			return mockPasswordResetRow{err: pgx.ErrNoRows}
		}
		return mockPasswordResetRow{userID: user.id}
	case strings.HasPrefix(sql, `SELECT user_id, expires_at, used_at FROM "password_reset"`):
		if len(args) != 1 {
			return mockPasswordResetRow{err: fmt.Errorf("token arg count = %d", len(args))}
		}
		tokenHash, ok := args[0].(string)
		if !ok {
			return mockPasswordResetRow{err: fmt.Errorf("token_hash type = %T", args[0])}
		}
		row, ok := db.tokens[tokenHash]
		if !ok {
			return mockPasswordResetRow{err: pgx.ErrNoRows}
		}
		return mockPasswordResetRow{userID: row.userID, expiresAt: row.expiresAt, usedAt: row.usedAt}
	default:
		return mockPasswordResetRow{err: fmt.Errorf("unexpected query SQL: %s", sql)}
	}
}

type mockPasswordResetRow struct {
	userID    lazuli.ID
	expiresAt time.Time
	usedAt    *time.Time
	err       error
}

func (r mockPasswordResetRow) Scan(dest ...any) error {
	if r.err != nil {
		return r.err
	}
	switch len(dest) {
	case 1:
		userID, ok := dest[0].(*int64)
		if !ok {
			return fmt.Errorf("scan user_id dest type = %T", dest[0])
		}
		*userID = int64(r.userID)
	case 3:
		userID, ok := dest[0].(*int64)
		if !ok {
			return fmt.Errorf("scan token user_id dest type = %T", dest[0])
		}
		expiresAt, ok := dest[1].(*time.Time)
		if !ok {
			return fmt.Errorf("scan expires_at dest type = %T", dest[1])
		}
		usedAt, ok := dest[2].(**time.Time)
		if !ok {
			return fmt.Errorf("scan used_at dest type = %T", dest[2])
		}
		*userID = int64(r.userID)
		*expiresAt = r.expiresAt
		*usedAt = r.usedAt
	default:
		return fmt.Errorf("scan dest count = %d", len(dest))
	}
	return nil
}

func TestPasswordResetRequestAndConfirm(t *testing.T) {
	db := withMockPasswordResetDB(t)
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	contract := testPasswordResetContract()
	db.users["alice@example.com"] = mockPasswordResetUser{id: lazuli.ID(42), passwordHash: "old"}

	token, err := RequestPasswordReset(ctx, contract, "alice@example.com")
	if err != nil {
		t.Fatalf("RequestPasswordReset: %v", err)
	}
	raw, err := base64.RawURLEncoding.DecodeString(string(token))
	if err != nil {
		t.Fatalf("token must be raw URL-safe base64: %v", err)
	}
	if len(raw) != 32 {
		t.Fatalf("token entropy bytes = %d, want 32", len(raw))
	}
	if len(db.tokens) != 1 {
		t.Fatalf("stored tokens = %d, want 1", len(db.tokens))
	}

	if err := ConfirmPasswordReset(ctx, contract, token, "new-secret"); err != nil {
		t.Fatalf("ConfirmPasswordReset: %v", err)
	}
	user := db.users["alice@example.com"]
	if user.passwordHash == "" || user.passwordHash == "old" {
		t.Fatalf("password hash was not updated: %q", user.passwordHash)
	}
	if err := VerifyPassword(ctx, PasswordContract{}, "new-secret", user.passwordHash); err != nil {
		t.Fatalf("updated password hash does not verify: %v", err)
	}
	for _, row := range db.tokens {
		if row.usedAt == nil || !row.usedAt.Equal(now) {
			t.Fatalf("used_at = %v, want %v", row.usedAt, now)
		}
	}
}

func TestPasswordResetRequestUnknownIdentityIsIdempotent(t *testing.T) {
	db := withMockPasswordResetDB(t)
	ctx := &lazuli.Ctx{Context: context.Background()}

	token, err := RequestPasswordReset(ctx, testPasswordResetContract(), "missing@example.com")
	if err != nil {
		t.Fatalf("RequestPasswordReset unknown identity: %v", err)
	}
	if token == "" {
		t.Fatalf("unknown identity should still receive token-shaped value")
	}
	if len(db.tokens) != 0 {
		t.Fatalf("stored tokens = %d, want 0", len(db.tokens))
	}
}

func TestPasswordResetConfirmRejectsInvalidExpiredAndUsedTokens(t *testing.T) {
	db := withMockPasswordResetDB(t)
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	contract := testPasswordResetContract()
	db.users["alice@example.com"] = mockPasswordResetUser{id: lazuli.ID(42), passwordHash: "old"}

	if err := ConfirmPasswordReset(ctx, contract, PasswordResetToken("not a token"), "new"); !errors.Is(err, ErrPasswordResetTokenInvalid) {
		t.Fatalf("invalid token error = %v, want ErrPasswordResetTokenInvalid", err)
	}

	expired, expiredHash, err := newPasswordResetToken()
	if err != nil {
		t.Fatalf("newPasswordResetToken expired: %v", err)
	}
	db.tokens[expiredHash] = mockPasswordResetTokenRow{userID: lazuli.ID(42), expiresAt: now.Add(-time.Second)}
	if err := ConfirmPasswordReset(ctx, contract, expired, "new"); !errors.Is(err, ErrPasswordResetTokenExpired) {
		t.Fatalf("expired token error = %v, want ErrPasswordResetTokenExpired", err)
	}

	used, usedHash, err := newPasswordResetToken()
	if err != nil {
		t.Fatalf("newPasswordResetToken used: %v", err)
	}
	usedAt := now.Add(-time.Minute)
	db.tokens[usedHash] = mockPasswordResetTokenRow{userID: lazuli.ID(42), expiresAt: now.Add(time.Hour), usedAt: &usedAt}
	if err := ConfirmPasswordReset(ctx, contract, used, "new"); !errors.Is(err, ErrPasswordResetTokenInvalid) {
		t.Fatalf("used token error = %v, want ErrPasswordResetTokenInvalid", err)
	}
}

func testPasswordResetContract() PasswordResetContract {
	return PasswordResetContract{
		Resource: "password_reset",
		TTL:      time.Hour,
		Identity: FieldRef{
			Resource: "user",
			Field:    "email",
		},
	}
}
