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

type mockEmailVerificationDB struct {
	users  map[lazuli.ID]mockEmailVerificationUser
	tokens map[string]mockEmailVerificationStoredToken
}

type mockEmailVerificationUser struct {
	email           string
	emailVerifiedAt time.Time
}

type mockEmailVerificationStoredToken struct {
	userID        lazuli.ID
	identityValue string
	expiresAt     time.Time
	createdAt     time.Time
	consumedAt    time.Time
}

func withMockEmailVerificationDB(t *testing.T) *mockEmailVerificationDB {
	t.Helper()
	db := &mockEmailVerificationDB{
		users:  make(map[lazuli.ID]mockEmailVerificationUser),
		tokens: make(map[string]mockEmailVerificationStoredToken),
	}
	prev := emailVerificationDBProvider
	emailVerificationDBProvider = func() emailVerificationDB { return db }
	t.Cleanup(func() { emailVerificationDBProvider = prev })
	return db
}

func (db *mockEmailVerificationDB) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	if !strings.HasPrefix(sql, `INSERT INTO "email_verification"`) {
		return pgconn.CommandTag{}, fmt.Errorf("unexpected exec SQL: %s", sql)
	}
	if len(args) != 5 {
		return pgconn.CommandTag{}, fmt.Errorf("insert arg count = %d", len(args))
	}
	userID, ok := args[0].(int64)
	if !ok {
		return pgconn.CommandTag{}, fmt.Errorf("insert user_id type = %T", args[0])
	}
	identityValue, ok := args[1].(string)
	if !ok {
		return pgconn.CommandTag{}, fmt.Errorf("insert identity_value type = %T", args[1])
	}
	tokenHash, ok := args[2].(string)
	if !ok {
		return pgconn.CommandTag{}, fmt.Errorf("insert token_hash type = %T", args[2])
	}
	expiresAt, ok := args[3].(time.Time)
	if !ok {
		return pgconn.CommandTag{}, fmt.Errorf("insert expires_at type = %T", args[3])
	}
	createdAt, ok := args[4].(time.Time)
	if !ok {
		return pgconn.CommandTag{}, fmt.Errorf("insert created_at type = %T", args[4])
	}
	db.tokens[tokenHash] = mockEmailVerificationStoredToken{
		userID:        lazuli.ID(userID),
		identityValue: identityValue,
		expiresAt:     expiresAt,
		createdAt:     createdAt,
	}
	return pgconn.NewCommandTag("INSERT 0 1"), nil
}

func (db *mockEmailVerificationDB) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	switch {
	case strings.HasPrefix(sql, `SELECT "email" FROM "customer"`):
		if len(args) != 1 {
			return mockEmailVerificationRow{err: fmt.Errorf("identity arg count = %d", len(args))}
		}
		userID, ok := args[0].(int64)
		if !ok {
			return mockEmailVerificationRow{err: fmt.Errorf("identity user_id type = %T", args[0])}
		}
		user, ok := db.users[lazuli.ID(userID)]
		if !ok {
			return mockEmailVerificationRow{err: pgx.ErrNoRows}
		}
		return mockEmailVerificationRow{identityValue: user.email}
	case strings.HasPrefix(sql, "WITH consumed AS"):
		if len(args) != 2 {
			return mockEmailVerificationRow{err: fmt.Errorf("verify arg count = %d", len(args))}
		}
		tokenHash, ok := args[0].(string)
		if !ok {
			return mockEmailVerificationRow{err: fmt.Errorf("verify token_hash type = %T", args[0])}
		}
		now, ok := args[1].(time.Time)
		if !ok {
			return mockEmailVerificationRow{err: fmt.Errorf("verify now type = %T", args[1])}
		}
		stored, ok := db.tokens[tokenHash]
		if !ok || !stored.consumedAt.IsZero() || !stored.expiresAt.After(now) {
			return mockEmailVerificationRow{err: pgx.ErrNoRows}
		}
		user, ok := db.users[stored.userID]
		if !ok {
			return mockEmailVerificationRow{err: pgx.ErrNoRows}
		}
		stored.consumedAt = now
		db.tokens[tokenHash] = stored
		user.emailVerifiedAt = now
		db.users[stored.userID] = user
		return mockEmailVerificationRow{userID: stored.userID}
	case strings.HasPrefix(sql, `SELECT expires_at FROM "email_verification"`):
		if len(args) != 1 {
			return mockEmailVerificationRow{err: fmt.Errorf("expiry arg count = %d", len(args))}
		}
		tokenHash, ok := args[0].(string)
		if !ok {
			return mockEmailVerificationRow{err: fmt.Errorf("expiry token_hash type = %T", args[0])}
		}
		stored, ok := db.tokens[tokenHash]
		if !ok || !stored.consumedAt.IsZero() {
			return mockEmailVerificationRow{err: pgx.ErrNoRows}
		}
		return mockEmailVerificationRow{expiresAt: stored.expiresAt}
	default:
		return mockEmailVerificationRow{err: fmt.Errorf("unexpected query SQL: %s", sql)}
	}
}

type mockEmailVerificationRow struct {
	userID        lazuli.ID
	identityValue string
	expiresAt     time.Time
	err           error
}

func (r mockEmailVerificationRow) Scan(dest ...any) error {
	if r.err != nil {
		return r.err
	}
	if len(dest) != 1 {
		return fmt.Errorf("scan dest count = %d", len(dest))
	}
	switch out := dest[0].(type) {
	case *int64:
		*out = int64(r.userID)
	case *string:
		*out = r.identityValue
	case *time.Time:
		*out = r.expiresAt
	default:
		return fmt.Errorf("scan dest type = %T", dest[0])
	}
	return nil
}

func TestEmailVerificationIssueVerifyRoundTrip(t *testing.T) {
	db := withMockEmailVerificationDB(t)
	now := time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	contract := EmailVerificationContract{
		Resource: "email_verification",
		TTL:      2 * time.Hour,
		Identity: FieldRef{Resource: "customer", Field: "email"},
	}
	db.users[42] = mockEmailVerificationUser{email: "ada@example.test"}

	token, err := IssueEmailVerificationToken(ctx, contract, lazuli.ID(42))
	if err != nil {
		t.Fatalf("IssueEmailVerificationToken: %v", err)
	}
	raw, err := base64.RawURLEncoding.DecodeString(string(token))
	if err != nil {
		t.Fatalf("token must be raw URL-safe base64: %v", err)
	}
	if len(raw) != 32 {
		t.Fatalf("token entropy bytes = %d, want 32", len(raw))
	}

	sum := sha256.Sum256([]byte(token))
	tokenHash := hex.EncodeToString(sum[:])
	stored, ok := db.tokens[tokenHash]
	if !ok {
		t.Fatal("verification row stored under SHA-256 token hash")
	}
	if stored.identityValue != "ada@example.test" {
		t.Fatalf("identityValue = %q, want ada@example.test", stored.identityValue)
	}
	if want := now.Add(2 * time.Hour); !stored.expiresAt.Equal(want) {
		t.Fatalf("expiresAt = %v, want %v", stored.expiresAt, want)
	}

	userID, err := VerifyEmailToken(ctx, contract, token)
	if err != nil {
		t.Fatalf("VerifyEmailToken: %v", err)
	}
	if userID != lazuli.ID(42) {
		t.Fatalf("userID = %d, want 42", userID)
	}
	if got := db.users[42].emailVerifiedAt; !got.Equal(now) {
		t.Fatalf("emailVerifiedAt = %v, want %v", got, now)
	}
	if _, err := VerifyEmailToken(ctx, contract, token); !errors.Is(err, ErrEmailVerifyTokenInvalid) {
		t.Fatalf("VerifyEmailToken second use = %v, want ErrEmailVerifyTokenInvalid", err)
	}
}

func TestEmailVerificationRejectsExpiredToken(t *testing.T) {
	db := withMockEmailVerificationDB(t)
	now := time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC)
	issueCtx := &lazuli.Ctx{Context: context.Background(), Now: now}
	verifyCtx := &lazuli.Ctx{Context: context.Background(), Now: now.Add(2 * time.Hour)}
	contract := EmailVerificationContract{
		Resource: "email_verification",
		TTL:      time.Hour,
		Identity: FieldRef{Resource: "customer", Field: "email"},
	}
	db.users[7] = mockEmailVerificationUser{email: "grace@example.test"}

	token, err := IssueEmailVerificationToken(issueCtx, contract, lazuli.ID(7))
	if err != nil {
		t.Fatalf("IssueEmailVerificationToken: %v", err)
	}
	if _, err := VerifyEmailToken(verifyCtx, contract, token); !errors.Is(err, ErrEmailVerifyTokenExpired) {
		t.Fatalf("VerifyEmailToken expired = %v, want ErrEmailVerifyTokenExpired", err)
	}
	if !db.users[7].emailVerifiedAt.IsZero() {
		t.Fatal("expired token marked email verified")
	}
}

func TestEmailVerificationRejectsInvalidToken(t *testing.T) {
	withMockEmailVerificationDB(t)
	contract := EmailVerificationContract{
		Resource: "email_verification",
		Identity: FieldRef{Resource: "customer", Field: "email"},
	}

	if _, err := VerifyEmailToken(&lazuli.Ctx{Context: context.Background()}, contract, "not a token"); !errors.Is(err, ErrEmailVerifyTokenInvalid) {
		t.Fatalf("VerifyEmailToken invalid = %v, want ErrEmailVerifyTokenInvalid", err)
	}
}

func TestEmailVerificationDefaultsTTLTo24Hours(t *testing.T) {
	db := withMockEmailVerificationDB(t)
	now := time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	contract := EmailVerificationContract{
		Resource: "email_verification",
		Identity: FieldRef{Resource: "customer", Field: "email"},
	}
	db.users[1] = mockEmailVerificationUser{email: "linus@example.test"}

	token, err := IssueEmailVerificationToken(ctx, contract, lazuli.ID(1))
	if err != nil {
		t.Fatalf("IssueEmailVerificationToken: %v", err)
	}
	sum := sha256.Sum256([]byte(token))
	stored := db.tokens[hex.EncodeToString(sum[:])]
	if want := now.Add(24 * time.Hour); !stored.expiresAt.Equal(want) {
		t.Fatalf("expiresAt = %v, want %v", stored.expiresAt, want)
	}
}
