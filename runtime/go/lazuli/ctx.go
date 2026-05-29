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

	// RefreshToken is the raw refresh-token cookie value the transport
	// extracted from `lazuli_refresh` (cookie) or `Authorization: Bearer`
	// (mobile/bearer mode). Populated unconditionally — independent of
	// whether the access token validated — so the refresh handler can
	// rotate a session whose access token already expired.
	RefreshToken string

	// SessionExpiredAt is set when a shape-valid access token maps to
	// a refresh-capable session row whose access TTL elapsed.
	SessionExpiredAt *time.Time

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

// SetSessionCookie issues the session cookie with the runtime defaults
// (name `lazuli_session`, HttpOnly + SameSite=Lax + Secure by default).
// Used by login handlers after `auth.IssueSession` returns the opaque
// token. Pass `ttl=0` to use `SessionCookieTTL` (7 days).
//
// When the feature's `auth.sessions.cookie` block declared transport
// attributes (lowered into [SessionCookieConfig] by boot wiring), the
// declared axes override the defaults below; absent axes keep these
// literals. A `cookie`-less declaration leaves the config zero-valued, so
// the stamped cookie is byte-identical to the pre-`cookie` runtime.
func (c *Ctx) SetSessionCookie(token string, ttl time.Duration) {
	if ttl <= 0 {
		ttl = SessionCookieTTL
	}
	c.SetCookie(SessionCookieName(), token, sessionCookieConfig.opts(ttl))
}

// ClearSessionCookie deletes the session cookie. Used by logout handlers
// after `auth.InvalidateSession`. Honors the declared cookie name so the
// cleared cookie matches the one that was set.
func (c *Ctx) ClearSessionCookie() {
	c.DeleteCookie(SessionCookieName())
}

// SetRefreshCookie issues the canonical `lazuli_refresh` cookie carrying
// the long-lived refresh token. Inherits the same declared transport
// attributes (SameSite / Secure / HttpOnly / Domain / Path) as the
// session cookie; the cookie NAME stays `lazuli_refresh` (the refresh
// cookie is a distinct canonical cookie — `name` overrides the session
// cookie only). Used by the refresh handler after `auth.RotateSession`
// returns a new refresh token.
func (c *Ctx) SetRefreshCookie(token string, ttl time.Duration) {
	if ttl <= 0 {
		ttl = SessionCookieTTL
	}
	c.SetCookie(ProductionRefreshCookieName, token, sessionCookieConfig.opts(ttl))
}

// ClearRefreshCookie deletes the canonical refresh cookie. Used after
// a refresh failure (theft detection forces logout).
func (c *Ctx) ClearRefreshCookie() {
	c.DeleteCookie(ProductionRefreshCookieName)
}

// SessionCookieConfig carries the transport attributes lowered from a
// feature's `auth.sessions.cookie` block. Each axis is a pointer so the
// zero value (every field nil) means "nothing declared" — the runtime
// then keeps its hardcoded literals and behaves exactly as it did before
// the `cookie` child existed. Boot wiring (emitted by codegen from the IR
// `SessionCookie`) calls [ConfigureSessionCookie] with the populated
// axes.
//
// This is wire, not logic: `opts` just overlays declared axes onto the
// same [CookieOpts] sink the SET helpers already used. `HTTPOnly` is the
// author-facing axis; the sink is keyed on the inverse `AllowJS`, so
// `HTTPOnly=&true` lowers to `AllowJS:false`.
type SessionCookieConfig struct {
	// Name overrides the session cookie name (`lazuli_session`). Nil keeps
	// the canonical name. The refresh cookie is unaffected.
	Name *string
	// SameSite overrides the SameSite policy. Nil keeps SameSite=Lax.
	SameSite *http.SameSite
	// Secure overrides the Secure flag. Nil keeps the
	// `SetSessionCookieSecure` default (true in prod).
	Secure *bool
	// HTTPOnly overrides the HttpOnly flag. Nil keeps HttpOnly=true.
	HTTPOnly *bool
	// Domain overrides the cookie Domain. Nil keeps host-only (empty).
	Domain *string
	// Path overrides the cookie Path. Nil keeps `/`.
	Path *string
}

// sessionCookieConfig is the process-wide declared cookie config. Zero
// value = nothing declared = legacy behavior. Set once at boot by
// generated wiring, mirroring `sessionCookieSecureFlag`.
var sessionCookieConfig SessionCookieConfig

// ConfigureSessionCookie installs the declared `auth.sessions.cookie`
// transport attributes. Generated boot wiring calls this from the lowered
// IR `SessionCookie`; absent axes pass nil and keep the runtime default.
// Calling it with the zero value is a no-op (legacy behavior).
func ConfigureSessionCookie(cfg SessionCookieConfig) { sessionCookieConfig = cfg }

// SessionCookieName returns the declared session cookie name, or the
// canonical `lazuli_session` when none was declared. The production
// session middleware reads through this so SET and READ stay symmetric.
func SessionCookieName() string {
	if sessionCookieConfig.Name != nil && *sessionCookieConfig.Name != "" {
		return *sessionCookieConfig.Name
	}
	return ProductionSessionCookieName
}

// opts builds the CookieOpts for a session/refresh cookie at the given
// TTL, starting from the runtime defaults and overlaying any declared
// axis. The defaults are exactly the literals the SET helpers hardcoded
// before the `cookie` child landed (Path `/`, HttpOnly, Secure-by-default,
// SameSite=Lax), so a zero-valued config reproduces legacy behavior.
func (c SessionCookieConfig) opts(ttl time.Duration) CookieOpts {
	opts := CookieOpts{
		TTL:      ttl,
		Path:     "/",
		AllowJS:  false,
		Secure:   sessionCookieSecureDefault(),
		SameSite: http.SameSiteLaxMode,
	}
	if c.SameSite != nil {
		opts.SameSite = *c.SameSite
	}
	if c.Secure != nil {
		opts.Secure = *c.Secure
	}
	if c.HTTPOnly != nil {
		opts.AllowJS = !*c.HTTPOnly
	}
	if c.Domain != nil {
		opts.Domain = *c.Domain
	}
	if c.Path != nil && *c.Path != "" {
		opts.Path = *c.Path
	}
	return opts
}

// sessionCookieSecureDefault returns whether the canonical session
// cookie should set the `Secure` attribute. Defaults to `true` so
// production deployments stay safe; tests and local dev flip it to
// `false` via `SetSessionCookieSecure(false)` if the dev server isn't
// behind HTTPS.
//
// SECURITY (SEC-H1): session cookies are HTTPS-only by default.
// Tests and local dev flip via SetSessionCookieSecure(false) before
// the test/server runs. Closes HIGH-1 from
// lazuli-cyber-owasp-2026-05-20.md.
var sessionCookieSecureFlag = true

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
