package auth

import (
	"errors"
	"time"

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
	// TTL is the session lifetime parsed by Lazuli from the DSL
	// duration string.
	TTL time.Duration
	// Refresh enables refresh-token rotation. Default `false`.
	Refresh bool
}

// Typed errors returned by the session capability. Mapped to
// `expose client` HTTP status codes:
//
//	ErrSessionExpired  → 401 auth.session_expired
//	ErrSessionNotFound → 401 auth.session_unknown
var (
	ErrSessionExpired  = errors.New("auth: session expired")
	ErrSessionNotFound = errors.New("auth: session not found")
)

// IssueSession persists a new session row and returns the cookie
// value the transport layer must set on the response.
func IssueSession(ctx *lazuli.Ctx, contract SessionsContract, identityID lazuli.ID, provider string) (string, error) {
	_ = ctx
	_ = contract
	_ = identityID
	_ = provider
	return "", errors.New("auth: IssueSession not yet implemented")
}

// ResolveSession is the HTTP middleware hook that populates Ctx.User
// (and Ctx.Tenant when the session row carries one) from a cookie
// value. Replaces the dev-mode `populateDevSession` once the codegen
// emits a `SessionsContract`.
func ResolveSession(ctx *lazuli.Ctx, contract SessionsContract, cookie string) error {
	_ = ctx
	_ = contract
	_ = cookie
	return errors.New("auth: ResolveSession not yet implemented")
}

// InvalidateSession deletes the persisted session row and asks the
// transport layer to clear the cookie.
func InvalidateSession(ctx *lazuli.Ctx, contract SessionsContract, cookie string) error {
	_ = ctx
	_ = contract
	_ = cookie
	return errors.New("auth: InvalidateSession not yet implemented")
}
