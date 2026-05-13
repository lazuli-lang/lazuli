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

// PasswordResetToken is the opaque one-time token sent to a user during the
// password reset flow. Only its SHA-256 hash is persisted.
type PasswordResetToken string

// PasswordResetContract is the lowered password-reset block. Resource names
// the generated reset-token table, and Identity points at the user resource
// field used to find the account, typically User.email.
type PasswordResetContract struct {
	Resource string
	TTL      time.Duration
	Identity FieldRef
}

var (
	ErrPasswordResetTokenInvalid = errors.New("auth: password reset token invalid")
	ErrPasswordResetTokenExpired = errors.New("auth: password reset token expired")
)

type passwordResetDB interface {
	Exec(context.Context, string, ...any) (pgconn.CommandTag, error)
	QueryRow(context.Context, string, ...any) pgx.Row
}

var passwordResetDBProvider = func() passwordResetDB {
	return lazuli.DB()
}

// RequestPasswordReset issues a one-time token and stores its hash when the
// identity exists. Unknown identities still receive a token-shaped value and a
// nil error so callers do not leak account existence.
func RequestPasswordReset(ctx *lazuli.Ctx, contract PasswordResetContract, email string) (PasswordResetToken, error) {
	token, tokenHash, err := newPasswordResetToken()
	if err != nil {
		return "", err
	}

	var userID lazuli.ID
	lookupSQL := fmt.Sprintf(
		"SELECT id FROM %s WHERE %s = $1 LIMIT 1",
		quotePasswordResetIdent(contract.Identity.Resource),
		quotePasswordResetIdent(contract.Identity.Field),
	)
	err = passwordResetDBProvider().QueryRow(ctxOrBackground(ctx), lookupSQL, email).Scan(&userID)
	if errors.Is(err, pgx.ErrNoRows) {
		return token, nil
	}
	if err != nil {
		return "", err
	}

	expiresAt := passwordResetNow(ctx).Add(passwordResetTTL(contract.TTL))
	insertSQL := fmt.Sprintf(
		"INSERT INTO %s (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
		quotePasswordResetIdent(contract.Resource),
	)
	if _, err := passwordResetDBProvider().Exec(ctxOrBackground(ctx), insertSQL, userID, tokenHash, expiresAt); err != nil {
		return "", err
	}
	return token, nil
}

// ConfirmPasswordReset validates a one-time reset token, hashes the new
// password with the runtime default password contract, updates the identity
// resource's password_hash column, and marks the reset token as used.
func ConfirmPasswordReset(ctx *lazuli.Ctx, contract PasswordResetContract, token PasswordResetToken, newPassword string) error {
	tokenHash, err := hashPasswordResetToken(token)
	if err != nil {
		return err
	}

	var userID lazuli.ID
	var expiresAt time.Time
	var usedAt *time.Time
	selectSQL := fmt.Sprintf(
		"SELECT user_id, expires_at, used_at FROM %s WHERE token_hash = $1 LIMIT 1",
		quotePasswordResetIdent(contract.Resource),
	)
	err = passwordResetDBProvider().QueryRow(ctxOrBackground(ctx), selectSQL, tokenHash).Scan(&userID, &expiresAt, &usedAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return ErrPasswordResetTokenInvalid
	}
	if err != nil {
		return err
	}
	if usedAt != nil {
		return ErrPasswordResetTokenInvalid
	}
	now := passwordResetNow(ctx)
	if !expiresAt.After(now) {
		return ErrPasswordResetTokenExpired
	}

	passwordHash, err := HashPassword(ctx, PasswordContract{Identity: contract.Identity}, newPassword)
	if err != nil {
		return err
	}
	updateUserSQL := fmt.Sprintf(
		"UPDATE %s SET password_hash = $1 WHERE id = $2",
		quotePasswordResetIdent(contract.Identity.Resource),
	)
	if _, err := passwordResetDBProvider().Exec(ctxOrBackground(ctx), updateUserSQL, passwordHash, userID); err != nil {
		return err
	}

	useTokenSQL := fmt.Sprintf(
		"UPDATE %s SET used_at = $1 WHERE token_hash = $2",
		quotePasswordResetIdent(contract.Resource),
	)
	_, err = passwordResetDBProvider().Exec(ctxOrBackground(ctx), useTokenSQL, now, tokenHash)
	return err
}

func newPasswordResetToken() (PasswordResetToken, string, error) {
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return "", "", err
	}
	token := PasswordResetToken(base64.RawURLEncoding.EncodeToString(buf))
	tokenHash, err := hashPasswordResetToken(token)
	if err != nil {
		return "", "", err
	}
	return token, tokenHash, nil
}

func hashPasswordResetToken(token PasswordResetToken) (string, error) {
	raw, err := base64.RawURLEncoding.DecodeString(string(token))
	if err != nil || len(raw) != 32 {
		return "", ErrPasswordResetTokenInvalid
	}
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:]), nil
}

func passwordResetTTL(ttl time.Duration) time.Duration {
	if ttl > 0 {
		return ttl
	}
	return time.Hour
}

func passwordResetNow(ctx *lazuli.Ctx) time.Time {
	if ctx != nil && !ctx.Now.IsZero() {
		return ctx.Now
	}
	return time.Now()
}

func quotePasswordResetIdent(name string) string {
	for _, c := range name {
		ok := (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') || c == '_'
		if !ok {
			panic("lazuli/auth: refusing to quote suspicious password reset identifier: " + name)
		}
	}
	return `"` + name + `"`
}
