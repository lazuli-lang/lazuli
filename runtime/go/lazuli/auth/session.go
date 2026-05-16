package auth

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

// CookieName is the canonical name of the session cookie set by
// IssueSession and read by ResolveSession middleware. Same-site
// policy, secure flag, and domain come from `app.lzi` `urls` + `cors`.
const CookieName = "__lazuli_session"

// SessionsContract is the lowered `auth sessions` block. The codegen
// emits a `var SessionsContract = auth.SessionsContract{...}` per
// feature.
type SessionsContract struct {
	// Resource names the same-feature resource that backs persisted
	// sessions (e.g. `CustomerSession`).
	Resource string
	// TTL is the session lifetime. Generated code currently emits a
	// time.Duration; tests and adapters may pass the original DSL string
	// (e.g. "7 days"). Unparseable or non-positive values fall back to 24h.
	TTL any
	// Refresh enables refresh-token rotation. Default `false`.
	Refresh bool
}

// SessionAttrs carries optional session metadata reserved for generated
// callers. The current v0 table contract stores only user_id/token/expires_at,
// so runtime helpers return an empty map on resolve.
type SessionAttrs = map[string]any

// Typed errors returned by the session capability. Mapped to
// `expose client` HTTP status codes:
//
//	ErrSessionExpired  → 401 auth.session_expired
//	ErrSessionNotFound → 401 auth.session_unknown
//	ErrTokenInvalid    → 400 auth.token_invalid
var (
	ErrSessionExpired  = errors.New("auth: session expired")
	ErrSessionNotFound = errors.New("auth: session not found")
	ErrTokenInvalid    = errors.New("auth: token invalid")
)

type sessionDB interface {
	Exec(context.Context, string, ...any) (pgconn.CommandTag, error)
	QueryRow(context.Context, string, ...any) pgx.Row
}

var sessionDBProvider = func() sessionDB {
	return lazuli.DB()
}

// IssueSession persists a new session row and returns the cookie
// value the transport layer must set on the response.
func IssueSession(ctx *lazuli.Ctx, contract SessionsContract, userID lazuli.ID, attrs SessionAttrs) (string, time.Time, error) {
	_ = attrs

	token, tokenHash, err := newSessionToken()
	if err != nil {
		return "", time.Time{}, err
	}
	expiresAt := sessionNow(ctx).Add(sessionTTL(contract.TTL))
	// Column name `"user"` is intentionally quoted (SQL reserved word).
	// Lazuli emits each `field: Resource required` as a column named
	// `<field>` (no `_id` suffix); UserSession has a `user: User
	// required` field → `"user"` column. WAR-RUNTIME-SESSION-COL-01:
	// if a future codegen pivots to `<field>_id` columns, mirror here.
	sql := fmt.Sprintf(
		`INSERT INTO %s ("user", token_hash, expires_at) VALUES ($1, $2, $3)`,
		quoteSessionIdent(contract.Resource),
	)
	if _, err := sessionDBProvider().Exec(ctxOrBackground(ctx), sql, userID, tokenHash, expiresAt); err != nil {
		return "", time.Time{}, err
	}
	return token, expiresAt, nil
}

// ResolveSession is the HTTP middleware hook that populates Ctx.User
// (and Ctx.Tenant when the session row carries one) from a cookie
// value. Replaces the dev-mode `populateDevSession` once the codegen
// emits a `SessionsContract`.
func ResolveSession(ctx *lazuli.Ctx, contract SessionsContract, token string) (lazuli.ID, SessionAttrs, error) {
	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return 0, nil, err
	}

	sql := fmt.Sprintf(
		`SELECT "user", expires_at FROM %s WHERE token_hash = $1 LIMIT 1`,
		quoteSessionIdent(contract.Resource),
	)
	var userID lazuli.ID
	var expiresAt time.Time
	err = sessionDBProvider().QueryRow(ctxOrBackground(ctx), sql, tokenHash).Scan(&userID, &expiresAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return 0, nil, ErrSessionNotFound
	}
	if err != nil {
		return 0, nil, err
	}
	if !expiresAt.After(sessionNow(ctx)) {
		return 0, nil, ErrSessionExpired
	}
	return userID, SessionAttrs{}, nil
}

// InvalidateSession deletes the persisted session row and asks the
// transport layer to clear the cookie.
func InvalidateSession(ctx *lazuli.Ctx, contract SessionsContract, token string) error {
	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return err
	}
	sql := fmt.Sprintf(
		"DELETE FROM %s WHERE token_hash = $1",
		quoteSessionIdent(contract.Resource),
	)
	_, err = sessionDBProvider().Exec(ctxOrBackground(ctx), sql, tokenHash)
	return err
}

// MintSessionToken generates the (token, tokenHash, expiresAt) triple
// without writing to the database. Codegen callers use this, then build
// their own per-resource INSERT with the correct tenant columns.
func MintSessionToken(ctx *lazuli.Ctx, ttl any) (token, tokenHash string, expiresAt time.Time, err error) {
	token, tokenHash, err = newSessionToken()
	if err != nil {
		return
	}
	expiresAt = sessionNow(ctx).Add(sessionTTL(ttl))
	return
}

// HashSessionToken exposes the internal hash function for codegen callers
// that receive a raw token and need to compute the stored hash.
func HashSessionToken(token string) (string, error) { return hashSessionToken(token) }

// MapSessionResolveError maps pgx sentinel errors to typed auth errors.
// Codegen callers use this after their own SELECT roundtrip.
func MapSessionResolveError(err error) error {
	if errors.Is(err, pgx.ErrNoRows) {
		return ErrSessionNotFound
	}
	return err
}

// SessionDB exposes the configured sessionDB handle to codegen callers.
// Uses _ for ctx; reserved for future tenant-scoped connection pools.
func SessionDB(_ *lazuli.Ctx) sessionDB { return sessionDBProvider() }

func newSessionToken() (string, string, error) {
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return "", "", err
	}
	token := base64.RawURLEncoding.EncodeToString(buf)
	tokenHash, err := hashSessionToken(token)
	if err != nil {
		return "", "", err
	}
	return token, tokenHash, nil
}

func hashSessionToken(token string) (string, error) {
	raw, err := base64.RawURLEncoding.DecodeString(token)
	if err != nil || len(raw) != 32 {
		return "", ErrTokenInvalid
	}
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:]), nil
}

func sessionTTL(raw any) time.Duration {
	const fallback = 24 * time.Hour
	switch v := raw.(type) {
	case time.Duration:
		if v > 0 {
			return v
		}
	case string:
		if d, ok := parseSessionDuration(v); ok && d > 0 {
			return d
		}
	case lazuli.Duration:
		if d, ok := parseSessionDuration(string(v)); ok && d > 0 {
			return d
		}
	}
	return fallback
}

func parseSessionDuration(raw string) (time.Duration, bool) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return 0, false
	}
	if d, err := time.ParseDuration(trimmed); err == nil {
		return d, true
	}

	compact := strings.ReplaceAll(trimmed, " ", "")
	if d, ok := parseSessionNumberUnit(compact); ok {
		return d, true
	}

	parts := strings.Fields(trimmed)
	if len(parts) != 2 {
		return 0, false
	}
	return parseSessionNumberAndUnit(parts[0], parts[1])
}

func parseSessionNumberUnit(compact string) (time.Duration, bool) {
	splitAt := -1
	for i, c := range compact {
		if c < '0' || c > '9' {
			splitAt = i
			break
		}
	}
	if splitAt <= 0 {
		return 0, false
	}
	return parseSessionNumberAndUnit(compact[:splitAt], compact[splitAt:])
}

func parseSessionNumberAndUnit(n, unit string) (time.Duration, bool) {
	value, err := strconv.ParseInt(n, 10, 64)
	if err != nil || value <= 0 {
		return 0, false
	}
	switch strings.ToLower(strings.TrimSpace(unit)) {
	case "ms", "millisecond", "milliseconds":
		return time.Duration(value) * time.Millisecond, true
	case "s", "sec", "secs", "second", "seconds":
		return time.Duration(value) * time.Second, true
	case "m", "min", "mins", "minute", "minutes":
		return time.Duration(value) * time.Minute, true
	case "h", "hr", "hrs", "hour", "hours":
		return time.Duration(value) * time.Hour, true
	case "d", "day", "days":
		return time.Duration(value) * 24 * time.Hour, true
	case "w", "week", "weeks":
		return time.Duration(value) * 7 * 24 * time.Hour, true
	default:
		return 0, false
	}
}

func sessionNow(ctx *lazuli.Ctx) time.Time {
	if ctx != nil && !ctx.Now.IsZero() {
		return ctx.Now
	}
	return time.Now()
}

func quoteSessionIdent(name string) string {
	for _, c := range name {
		ok := (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') || c == '_'
		if !ok {
			panic("lazuli/auth: refusing to quote suspicious session resource: " + name)
		}
	}
	return `"` + sessionResourceTable(name) + `"`
}

// sessionResourceTable lower-snake-cases a PascalCase resource name so
// the auth runtime references the migrated table name. `UserSession`
// becomes `user_session`. See WAR-RUNTIME-MIGRATION-04 for the
// parallel fix in handle.go.
func sessionResourceTable(name string) string {
	var out []rune
	for i, r := range name {
		isUpper := r >= 'A' && r <= 'Z'
		if isUpper && i > 0 {
			out = append(out, '_')
		}
		if isUpper {
			out = append(out, r+32)
		} else {
			out = append(out, r)
		}
	}
	return string(out)
}
