package auth

import (
	"context"
	"errors"
	"net/http"
	"strconv"
	"strings"
	"time"

	"lazuli.dev/runtime/lazuli"
)

// SessionCookieOptions controls the Lazuli session cookie attributes.
//
// Name defaults to CookieName and Path defaults to "/". Session cookies are
// always HttpOnly.
type SessionCookieOptions struct {
	Name     string
	Path     string
	Domain   string
	Secure   bool
	SameSite http.SameSite
}

// SessionMiddlewareOptions configures SessionMiddleware.
type SessionMiddlewareOptions struct {
	Cookie SessionCookieOptions
}

type sessionContextKey struct{}
type sessionErrorContextKey struct{}

// ReadSessionCookie reads the Lazuli session cookie from r.
func ReadSessionCookie(r *http.Request, opts SessionCookieOptions) (string, error) {
	if r == nil {
		return "", lazuli.ErrCookieMissing
	}
	return lazuli.GetCookie(r, sessionCookieName(opts))
}

// WriteSessionCookie writes the Lazuli session cookie using expiresAt as the
// browser-visible expiry. Non-positive expiries clear the cookie.
func WriteSessionCookie(w http.ResponseWriter, token string, expiresAt time.Time, opts SessionCookieOptions) {
	if w == nil {
		return
	}
	if expiresAt.IsZero() || !expiresAt.After(time.Now()) {
		ClearSessionCookie(w, opts)
		return
	}

	http.SetCookie(w, &http.Cookie{
		Name:     sessionCookieName(opts),
		Value:    token,
		Path:     sessionCookiePath(opts),
		Domain:   opts.Domain,
		Expires:  expiresAt,
		MaxAge:   sessionCookieMaxAge(expiresAt),
		HttpOnly: true,
		Secure:   opts.Secure,
		SameSite: sessionCookieSameSite(opts),
	})
}

// ClearSessionCookie asks the browser to drop the Lazuli session cookie.
func ClearSessionCookie(w http.ResponseWriter, opts SessionCookieOptions) {
	if w == nil {
		return
	}
	http.SetCookie(w, &http.Cookie{
		Name:     sessionCookieName(opts),
		Value:    "",
		Path:     sessionCookiePath(opts),
		Domain:   opts.Domain,
		MaxAge:   -1,
		Expires:  time.Unix(0, 0),
		HttpOnly: true,
		Secure:   opts.Secure,
		SameSite: sessionCookieSameSite(opts),
	})
}

// SessionMiddleware resolves the Lazuli session cookie through store and
// attaches the resolved Session to the request context.
//
// Missing cookies pass through unchanged. Invalid, unknown, or expired session
// cookies are cleared and the resolve error is attached to the request context
// for downstream handlers that want to distinguish anonymous requests from bad
// credentials.
func SessionMiddleware(store SessionStore, opts SessionMiddlewareOptions) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		if store == nil {
			return next
		}

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			token, err := ReadSessionCookie(r, opts.Cookie)
			if errors.Is(err, lazuli.ErrCookieMissing) {
				next.ServeHTTP(w, r)
				return
			}
			if err != nil {
				next.ServeHTTP(w, r.WithContext(ContextWithSessionError(r.Context(), err)))
				return
			}

			session, err := store.Resolve(r.Context(), token)
			if err != nil {
				if shouldClearSessionCookie(err) {
					ClearSessionCookie(w, opts.Cookie)
				}
				next.ServeHTTP(w, r.WithContext(ContextWithSessionError(r.Context(), err)))
				return
			}

			next.ServeHTTP(w, r.WithContext(ContextWithSession(r.Context(), session)))
		})
	}
}

// ContextWithSession returns a context carrying a resolved auth session.
func ContextWithSession(ctx context.Context, session Session) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	return context.WithValue(ctx, sessionContextKey{}, session)
}

// SessionFromContext returns the resolved auth session attached by
// SessionMiddleware.
func SessionFromContext(ctx context.Context) (Session, bool) {
	if ctx == nil {
		return Session{}, false
	}
	session, ok := ctx.Value(sessionContextKey{}).(Session)
	return session, ok
}

// ContextWithSessionError returns a context carrying a session resolve error.
func ContextWithSessionError(ctx context.Context, err error) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	if err == nil {
		return ctx
	}
	return context.WithValue(ctx, sessionErrorContextKey{}, err)
}

// SessionErrorFromContext returns a session resolve error attached by
// SessionMiddleware.
func SessionErrorFromContext(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	err, _ := ctx.Value(sessionErrorContextKey{}).(error)
	return err
}

// ResolveSessionIntoCtx resolves token through store, attaches the resolved
// Session to the Go context, and projects generic session identity onto ctx.
func ResolveSessionIntoCtx(ctx *lazuli.Ctx, store SessionStore, token string) (Session, error) {
	if store == nil {
		return Session{}, errors.New("auth: session store missing")
	}

	session, err := store.Resolve(ctxOrBackground(ctx), token)
	if err != nil {
		return Session{}, err
	}
	if ctx != nil {
		ctx.Context = ContextWithSession(ctxOrBackground(ctx), session)
		ApplySessionToCtx(ctx, session)
	}
	return session, nil
}

// ApplySessionToCtx projects generic session identity onto a Lazuli request
// context without depending on an app-specific session resource type.
func ApplySessionToCtx(ctx *lazuli.Ctx, session Session) {
	if ctx == nil {
		return
	}

	user := &lazuli.User{ID: session.UserID}
	if email, ok := session.Attrs["email"].(string); ok {
		user.Email = email
	}
	if roles, ok := sessionRoles(session.Attrs["roles"]); ok {
		user.Roles = roles
	}
	if orgID, ok := sessionOrgID(session.Attrs); ok {
		user.OrgID = orgID
		ctx.Tenant = &lazuli.Tenant{OrgID: orgID}
	}

	ctx.Actor = lazuli.ActorUser
	ctx.User = user
}

func sessionCookieName(opts SessionCookieOptions) string {
	if opts.Name == "" {
		return CookieName
	}
	return opts.Name
}

func sessionCookiePath(opts SessionCookieOptions) string {
	if opts.Path == "" {
		return "/"
	}
	return opts.Path
}

func sessionCookieSameSite(opts SessionCookieOptions) http.SameSite {
	if opts.SameSite == 0 {
		return http.SameSiteStrictMode
	}
	return opts.SameSite
}

func sessionCookieMaxAge(expiresAt time.Time) int {
	ttl := time.Until(expiresAt)
	if ttl <= 0 {
		return -1
	}
	seconds := int(ttl.Seconds())
	if seconds < 1 {
		return 1
	}
	return seconds
}

func shouldClearSessionCookie(err error) bool {
	return errors.Is(err, ErrSessionExpired) ||
		errors.Is(err, ErrSessionNotFound) ||
		errors.Is(err, ErrTokenInvalid)
}

func sessionOrgID(attrs SessionAttrs) (lazuli.ID, bool) {
	for _, key := range []string{"org_id", "tenant_org_id"} {
		if id, ok := sessionID(attrs[key]); ok {
			return id, true
		}
	}
	return 0, false
}

func sessionID(value any) (lazuli.ID, bool) {
	switch v := value.(type) {
	case int:
		return lazuli.ID(v), v != 0
	case int64:
		return lazuli.ID(v), v != 0
	case float64:
		if v == float64(int64(v)) && v != 0 {
			return lazuli.ID(v), true
		}
	case string:
		parsed, err := strconv.ParseInt(strings.TrimSpace(v), 10, 64)
		return lazuli.ID(parsed), err == nil && parsed != 0
	}
	return 0, false
}

func sessionRoles(value any) ([]string, bool) {
	switch v := value.(type) {
	case []string:
		return append([]string(nil), v...), true
	case []any:
		roles := make([]string, 0, len(v))
		for _, item := range v {
			role, ok := item.(string)
			if !ok {
				return nil, false
			}
			role = strings.TrimSpace(role)
			if role != "" {
				roles = append(roles, role)
			}
		}
		return roles, true
	case string:
		if v == "" {
			return nil, true
		}
		parts := strings.Split(v, ",")
		roles := make([]string, 0, len(parts))
		for _, part := range parts {
			if role := strings.TrimSpace(part); role != "" {
				roles = append(roles, role)
			}
		}
		return roles, true
	default:
		return nil, false
	}
}
