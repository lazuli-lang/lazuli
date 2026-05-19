package lazuli

import (
	"context"
	"net/http"
	"time"
)

// Ctx is the request execution context that flows through every command,
// query, job, and webhook. The runtime populates it from the inbound HTTP
// request, the job trigger, the webhook envelope, etc.
//
// Generated code typically receives Ctx and reads `ctx.User`, `ctx.Tenant`,
// `ctx.Now`, and so on.
type Ctx struct {
	context.Context

	// Actor that initiated the call. Populated from session/JWT/HMAC by the
	// transport layer.
	Actor Actor

	// User is the authenticated user when the actor is `@actor.user`. Nil for
	// system/anonymous actors.
	User *User

	// Tenant is the active tenant scope. Nil only for explicit
	// `scope global` operations.
	Tenant *Tenant

	// RequestID is propagated from the inbound transport for tracing.
	RequestID string

	// TraceID for OpenTelemetry / distributed tracing.
	TraceID string

	// Now is the resolved current time (allows test override).
	Now time.Time

	// SessionID is the row id of the active `*UserSession` populated by
	// the transport layer's session-cookie middleware (`auth.ResolveSession`).
	// Zero when the actor is anonymous, system, or used a non-session
	// auth path (JWT, HMAC, plain header). Closes WAR-RUNTIME-CTX-01:
	// the `logout` handler can now revoke just THIS session instead of
	// every session of the actor (the previous "log out all devices"
	// hammer).
	SessionID ID

	// SessionToken is the raw cookie value the transport extracted
	// before swapping it for a User row. Exposed so handlers can
	// re-hash + look up the session row themselves when SessionID
	// isn't enough (e.g. step-up auth flows that need to invalidate
	// + reissue). Empty when SessionID is zero.
	SessionToken string

	// responseWriter is the per-request `http.ResponseWriter` populated
	// by the HTTP boundary so handlers can set cookies through
	// `Ctx.SetSessionCookie` / `Ctx.SetCookie` / `Ctx.DeleteCookie`
	// without taking a writer argument. Nil for non-HTTP contexts (jobs,
	// webhooks invoked through fixtures) — the helpers then become
	// no-ops so out-of-band code can call them safely.
	responseWriter http.ResponseWriter
}

// withResponseWriter is the runtime-internal hook the HTTP boundary
// uses to wire the per-request `http.ResponseWriter` into the Ctx.
// Exposed lowercase to keep the surface tight: external handlers see
// only the typed cookie helpers.
func (c *Ctx) withResponseWriter(w http.ResponseWriter) {
	c.responseWriter = w
}

// SetCookie writes a cookie on the active response. No-op when the
// Ctx isn't attached to an HTTP request (job / fixture). Handlers use
// this to issue session cookies without taking a writer parameter.
//
//	ctx.SetCookie("lazuli_session", token, lazuli.CookieOpts{TTL: 7 * 24 * time.Hour})
func (c *Ctx) SetCookie(name, value string, opts CookieOpts) {
	if c == nil || c.responseWriter == nil {
		return
	}
	SetCookie(c.responseWriter, name, value, opts)
}

// DeleteCookie clears a cookie on the active response. No-op when the
// Ctx isn't attached to an HTTP request. Pairs with `SetCookie` —
// handlers use this on logout.
//
//	ctx.DeleteCookie("lazuli_session")
func (c *Ctx) DeleteCookie(name string) {
	if c == nil || c.responseWriter == nil {
		return
	}
	DeleteCookie(c.responseWriter, name)
}

// SetSessionCookie issues the canonical `lazuli_session` cookie with
// the runtime defaults (HttpOnly + SameSite=Lax + Secure when TLS).
// Used by login handlers after `auth.IssueSession` returns the opaque
// token. Pass `ttl=0` to use `SessionCookieTTL` (7 days).
func (c *Ctx) SetSessionCookie(token string, ttl time.Duration) {
	if ttl <= 0 {
		ttl = SessionCookieTTL
	}
	c.SetCookie(ProductionSessionCookieName, token, CookieOpts{
		TTL:      ttl,
		Path:     "/",
		AllowJS:  false,
		Secure:   sessionCookieSecureDefault(),
		SameSite: http.SameSiteLaxMode,
	})
}

// ClearSessionCookie deletes the canonical session cookie. Used by
// logout handlers after `auth.InvalidateSession`.
func (c *Ctx) ClearSessionCookie() {
	c.DeleteCookie(ProductionSessionCookieName)
}

// sessionCookieSecureDefault returns whether the canonical session
// cookie should set the `Secure` attribute. Defaults to `true` so
// production deployments stay safe; tests and local dev flip it to
// `false` via `SetSessionCookieSecure(false)` if the dev server isn't
// behind HTTPS.
var sessionCookieSecureFlag = false

func sessionCookieSecureDefault() bool { return sessionCookieSecureFlag }

// SetSessionCookieSecure toggles the `Secure` attribute on the
// canonical session cookie. Boot wiring calls this with `true` in
// production and `false` for local dev (when the API runs on http://).
func SetSessionCookieSecure(secure bool) { sessionCookieSecureFlag = secure }

// Actor names the kind of caller. Mirrors the DSL `@actor.*` namespace.
type Actor string

const (
	ActorUser      Actor = "user"
	ActorSystem    Actor = "system"
	ActorAnonymous Actor = "anonymous"
)

// User is the authenticated user identity.
type User struct {
	ID    ID
	OrgID ID
	Email string
	Roles []string
}

// Tenant is the active tenant scope.
type Tenant struct {
	OrgID ID
}
