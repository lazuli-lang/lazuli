package auth

import (
	"errors"
	"time"

	"lazuli.dev/runtime/lazuli"
)

var errPasswordLoginLookupMissing = errors.New("auth: password login lookup missing")

// AuthSession is the stable login response shape generated auth commands
// can target. SessionToken is omitted from JSON because transports should
// set it as CookieName rather than echoing it in the response body.
type AuthSession struct {
	UserID       lazuli.ID    `json:"user_id"`
	ExpiresAt    time.Time    `json:"expires_at"`
	CookieName   string       `json:"cookie_name"`
	SessionToken string       `json:"-"`
	Attrs        SessionAttrs `json:"attrs,omitempty"`
}

// PasswordLoginInput is the generic input shape for password-backed login.
// Generated commands map their concrete identity field (for example email)
// into Identity before calling LoginPassword.
type PasswordLoginInput struct {
	Identity string `json:"identity"`
	Password string `json:"password"`
}

// PasswordLoginSubject is the persisted auth data needed to validate a
// password and issue a session.
type PasswordLoginSubject struct {
	UserID       lazuli.ID
	PasswordHash string
	Attrs        SessionAttrs
}

// PasswordLoginLookup loads the subject for an identity. Implementations
// should return ErrPasswordMismatch for unknown identities so callers do not
// reveal whether an account exists.
type PasswordLoginLookup func(ctx *lazuli.Ctx, identity string) (PasswordLoginSubject, error)

// LoginPassword validates a plaintext password with PasswordContract, issues
// a session with SessionsContract, and returns an AuthSession DTO for the
// generated command/transport layer.
func LoginPassword(
	ctx *lazuli.Ctx,
	passwords PasswordContract,
	sessions SessionsContract,
	input PasswordLoginInput,
	lookup PasswordLoginLookup,
) (AuthSession, error) {
	if lookup == nil {
		return AuthSession{}, errPasswordLoginLookupMissing
	}
	subject, err := lookup(ctx, input.Identity)
	if err != nil {
		return AuthSession{}, err
	}
	if err := VerifyPassword(ctx, passwords, input.Password, subject.PasswordHash); err != nil {
		return AuthSession{}, err
	}

	attrs := passwordLoginAttrs(subject.Attrs)
	token, expiresAt, err := IssueSession(ctx, sessions, subject.UserID, attrs)
	if err != nil {
		return AuthSession{}, err
	}
	return AuthSession{
		UserID:       subject.UserID,
		ExpiresAt:    expiresAt,
		CookieName:   CookieName,
		SessionToken: token,
		Attrs:        attrs,
	}, nil
}

func passwordLoginAttrs(attrs SessionAttrs) SessionAttrs {
	cloned := cloneSessionAttrs(attrs)
	if cloned == nil {
		cloned = SessionAttrs{}
	}
	if _, ok := cloned["provider"]; !ok {
		cloned["provider"] = "password"
	}
	return cloned
}
