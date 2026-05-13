package auth

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

// EmailVerificationToken is the opaque value stored in the user's
// verification email link.
type EmailVerificationToken string

// EmailVerificationContract is the lowered `auth.email_verification`
// shape. Mirrors PasswordContract style.
type EmailVerificationContract struct {
	Resource string        // e.g., "EmailVerification" (a resource table)
	TTL      time.Duration // default 24h
	Identity FieldRef      // typically Customer.email
}

var (
	ErrEmailVerifyTokenInvalid = errors.New("auth: email verification token invalid")
	ErrEmailVerifyTokenExpired = errors.New("auth: email verification token expired")
)

type emailVerificationDB interface {
	Exec(context.Context, string, ...any) (pgconn.CommandTag, error)
	QueryRow(context.Context, string, ...any) pgx.Row
}

var emailVerificationDBProvider = func() emailVerificationDB {
	return lazuli.DB()
}

// IssueEmailVerificationToken creates a token + DB row. Returns the token to
// email out. Token hash stored in DB; plaintext only in email.
func IssueEmailVerificationToken(ctx *lazuli.Ctx, contract EmailVerificationContract, userID lazuli.ID) (EmailVerificationToken, error) {
	token, tokenHash, err := newEmailVerificationToken()
	if err != nil {
		return "", err
	}

	db := emailVerificationDBProvider()
	execCtx := ctxOrBackground(ctx)
	now := emailVerificationNow(ctx)
	expiresAt := now.Add(emailVerificationTTL(contract.TTL))

	identityValue, err := loadEmailVerificationIdentity(execCtx, db, contract, userID)
	if err != nil {
		return "", err
	}

	sql := fmt.Sprintf(
		"INSERT INTO %s (user_id, identity_value, token_hash, expires_at, created_at) VALUES ($1, $2, $3, $4, $5)",
		quoteEmailVerificationIdent(contract.Resource),
	)
	if _, err := db.Exec(execCtx, sql, userID, identityValue, tokenHash, expiresAt, now); err != nil {
		return "", err
	}
	return token, nil
}

// VerifyEmailToken consumes the token + marks the user email verified. Returns
// userID on success, ErrEmailVerifyTokenInvalid/ErrEmailVerifyTokenExpired on
// failure.
func VerifyEmailToken(ctx *lazuli.Ctx, contract EmailVerificationContract, token EmailVerificationToken) (lazuli.ID, error) {
	tokenHash, err := hashEmailVerificationToken(token)
	if err != nil {
		return 0, err
	}

	db := emailVerificationDBProvider()
	execCtx := ctxOrBackground(ctx)
	now := emailVerificationNow(ctx)
	tokenTable := quoteEmailVerificationIdent(contract.Resource)
	identityTable := quoteEmailVerificationIdent(contract.Identity.Resource)
	verifiedAtColumn := quoteEmailVerificationIdent(contract.Identity.Field + "_verified_at")

	sql := fmt.Sprintf(
		`WITH consumed AS (
			UPDATE %s
			SET consumed_at = $2
			WHERE token_hash = $1
				AND consumed_at IS NULL
				AND expires_at > $2
			RETURNING user_id
		), marked AS (
			UPDATE %s
			SET %s = $2
			WHERE id = (SELECT user_id FROM consumed)
			RETURNING id
		)
		SELECT id FROM marked`,
		tokenTable,
		identityTable,
		verifiedAtColumn,
	)

	var userID lazuli.ID
	err = db.QueryRow(execCtx, sql, tokenHash, now).Scan(&userID)
	if err == nil {
		return userID, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return 0, err
	}

	expired, lookupErr := emailVerificationTokenExpired(execCtx, db, tokenTable, tokenHash, now)
	if lookupErr != nil {
		return 0, lookupErr
	}
	if expired {
		return 0, ErrEmailVerifyTokenExpired
	}
	return 0, ErrEmailVerifyTokenInvalid
}

func loadEmailVerificationIdentity(ctx context.Context, db emailVerificationDB, contract EmailVerificationContract, userID lazuli.ID) (string, error) {
	sql := fmt.Sprintf(
		"SELECT %s FROM %s WHERE id = $1 LIMIT 1",
		quoteEmailVerificationIdent(contract.Identity.Field),
		quoteEmailVerificationIdent(contract.Identity.Resource),
	)
	var identityValue string
	if err := db.QueryRow(ctx, sql, userID).Scan(&identityValue); err != nil {
		return "", err
	}
	return identityValue, nil
}

func emailVerificationTokenExpired(ctx context.Context, db emailVerificationDB, tokenTable, tokenHash string, now time.Time) (bool, error) {
	sql := fmt.Sprintf("SELECT expires_at FROM %s WHERE token_hash = $1 AND consumed_at IS NULL LIMIT 1", tokenTable)
	var expiresAt time.Time
	err := db.QueryRow(ctx, sql, tokenHash).Scan(&expiresAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	return !expiresAt.After(now), nil
}

func newEmailVerificationToken() (EmailVerificationToken, string, error) {
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return "", "", err
	}
	token := EmailVerificationToken(base64.RawURLEncoding.EncodeToString(buf))
	tokenHash, err := hashEmailVerificationToken(token)
	if err != nil {
		return "", "", err
	}
	return token, tokenHash, nil
}

func hashEmailVerificationToken(token EmailVerificationToken) (string, error) {
	raw, err := base64.RawURLEncoding.DecodeString(string(token))
	if err != nil || len(raw) != 32 {
		return "", ErrEmailVerifyTokenInvalid
	}
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:]), nil
}

func emailVerificationTTL(ttl time.Duration) time.Duration {
	if ttl > 0 {
		return ttl
	}
	return 24 * time.Hour
}

func emailVerificationNow(ctx *lazuli.Ctx) time.Time {
	if ctx != nil && !ctx.Now.IsZero() {
		return ctx.Now
	}
	return time.Now()
}

func quoteEmailVerificationIdent(name string) string {
	for _, c := range name {
		ok := (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') || c == '_'
		if !ok {
			panic("lazuli/auth: refusing to quote suspicious email verification identifier: " + name)
		}
	}
	return `"` + name + `"`
}
