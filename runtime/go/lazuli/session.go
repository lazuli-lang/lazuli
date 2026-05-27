package lazuli

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"
)

// populateDevSession fills the request Ctx from `X-Lazuli-*` headers. This
// is the **dev-mode auth** for the spike — real cookie/JWT/HMAC sessions
// arrive in a future cut and replace this function without changing the
// `Ctx` shape downstream.
//
// Recognised headers:
//
//	X-Lazuli-Actor    "user" | "system" | "anonymous" (default)
//	X-Lazuli-User-ID  numeric user id (sets Actor=user when present)
//	X-Lazuli-Org-ID   numeric org id (populates Tenant)
//	X-Lazuli-Email    user email
//	X-Lazuli-Roles    CSV of role names ("admin,sales")
//
// When `X-Lazuli-User-ID` is present the actor is forced to `user` and the
// `Ctx.User` struct is built. When only `X-Lazuli-Actor: system` is sent,
// the request runs as the system actor (no user). Without any header the
// request is anonymous.
func populateDevSession(r *http.Request, ctx *Ctx) {
	if os.Getenv("LAZULI_ENV") == "production" {
		if r.Header.Get("X-Lazuli-Actor") != "" ||
			r.Header.Get("X-Lazuli-User-ID") != "" ||
			r.Header.Get("X-Lazuli-Org-ID") != "" ||
			r.Header.Get("X-Lazuli-Roles") != "" {
			slog.Warn("lazuli: rejecting dev-session header in production",
				"remote_addr", r.RemoteAddr)
		}
		return
	}

	if actor := r.Header.Get("X-Lazuli-Actor"); actor == "system" {
		ctx.Actor = ActorSystem
		return
	}

	userIDStr := r.Header.Get("X-Lazuli-User-ID")
	if userIDStr == "" {
		return
	}
	userID, err := strconv.ParseInt(userIDStr, 10, 64)
	if err != nil {
		return
	}

	orgID, _ := strconv.ParseInt(r.Header.Get("X-Lazuli-Org-ID"), 10, 64)

	ctx.Actor = ActorUser
	ctx.User = &User{
		ID:    userID,
		OrgID: orgID,
		Email: r.Header.Get("X-Lazuli-Email"),
		Roles: parseRolesCSV(r.Header.Get("X-Lazuli-Roles")),
	}
	if orgID > 0 {
		ctx.Tenant = &Tenant{OrgID: orgID}
	}
}

// ProductionSessionCookieName is the canonical cookie name the production
// session middleware looks for. Mirrored by `lazuli.SetCookie(...)`
// callers (e.g. the the canonical pilot login handler).
const ProductionSessionCookieName = "lazuli_session"

// ProductionRefreshCookieName is the canonical cookie name carrying
// the long-lived refresh token. Issued by the auth refresh handler
// after `auth.RotateSession` returns a new refresh value; read by the
// production middleware to populate `ctx.RefreshToken`.
const ProductionRefreshCookieName = "lazuli_refresh"

// SessionResolver is the runtime-side adapter that turns an opaque
// session token into a populated actor. Implemented by the `auth`
// package and registered at boot from the per-feature `auth.gen.go`
// init block.
//
// The interface is intentionally thin (no contract object, no
// pgx) so the runtime can stay decoupled from the `auth` package —
// otherwise we'd reintroduce an import cycle (`lazuli` → `auth` →
// `lazuli`).
type SessionResolver interface {
	// Resolve looks up the session row matching `token`. On success it
	// returns `found=true` plus the identity (`userID`, `orgID`) and
	// session row id. On not-found / expired / malformed input it
	// returns `found=false, err=nil` so the middleware can keep the
	// request anonymous (policies decide whether to deny).
	//
	// Genuine infrastructure errors (DB down, bad SQL) surface as `err`
	// so the middleware can log them; the request still proceeds
	// anonymous.
	Resolve(ctx context.Context, token string) (userID, orgID ID, sessionID ID, found bool, err error)
}

// SessionExpiryResolver is optionally implemented by rotation-aware
// resolvers. It distinguishes an expired access token with a stored refresh
// hash from an anonymous request so the HTTP boundary can emit token_expired.
type SessionExpiryResolver interface {
	ResolveAccessWithExpiry(ctx context.Context, token string) (expiredAt time.Time, refreshable bool, err error)
}

// SessionRolesResolver is optionally implemented by resolvers that can
// also bear the actor's role assignments. When present, the production
// middleware populates `ctx.User.Roles` so policy `@role.*` atoms match
// without an extra hop. When absent, `ctx.User.Roles` stays empty (the
// dev-mode header path `X-Lazuli-Roles` still works).
type SessionRolesResolver interface {
	ResolveRoles(ctx context.Context, userID ID) (roles []string, err error)
}

var sessionResolver SessionResolver

// RegisterSessionResolver installs the production session resolver.
// Called from per-feature `auth.gen.go` init() blocks; calling more
// than once is allowed (last wins) so re-registration during tests
// works.
func RegisterSessionResolver(r SessionResolver) {
	sessionResolver = r
}

// ClearRefreshToken zeros the refresh token in ctx so subsequent
// handlers (logging, audit, non-refresh paths) cannot leak it.
// SECURITY (M-4): the refresh handler MUST call this immediately
// after reading the token. Defense: even if a logger somehow
// grabs ctx, the field is empty.
func (c *Ctx) ClearRefreshToken() {
	if c == nil {
		return
	}
	c.RefreshToken = ""
}

// populateProductionSession reads `lazuli_session` cookie (preferred)
// or `Authorization: Bearer <token>` header and populates Ctx via the
// registered SessionResolver. When no resolver is registered or no
// token is present, the function is a no-op so the dev-mode header
// path remains the only auth axis (test scaffolding stays green).
//
// Cookie wins over Authorization so browser-based clients that mirror
// the token to localStorage don't fight the cookie source-of-truth.
func populateProductionSession(r *http.Request, ctx *Ctx) {
	// Refresh token is extracted unconditionally so the refresh
	// command handler can rotate a session whose access token already
	// expired (the access path below will skip the resolver hop when
	// the access token is missing or invalid).
	if rt := extractRefreshToken(r); rt != "" {
		ctx.RefreshToken = rt
	}
	resolver := sessionResolver
	if resolver == nil {
		return
	}
	token := extractSessionToken(r)
	if token == "" {
		return
	}
	if expResolver, ok := resolver.(SessionExpiryResolver); ok {
		expiredAt, refreshable, err := expResolver.ResolveAccessWithExpiry(r.Context(), token)
		if err != nil {
			slog.Warn("lazuli: session expiry resolver error", "error", err)
			return
		}
		if refreshable {
			ctx.SessionExpiredAt = &expiredAt
			return
		}
	}
	userID, orgID, sessionID, found, err := resolver.Resolve(r.Context(), token)
	if err != nil {
		// Genuine DB faults bubble up so ops can spot them in logs;
		// the request still proceeds anonymous so policy can decide.
		slog.Warn("lazuli: session resolver error", "error", err)
		return
	}
	if !found {
		return
	}
	ctx.Actor = ActorUser
	ctx.User = &User{ID: userID, OrgID: orgID}
	if rolesResolver, ok := resolver.(SessionRolesResolver); ok && userID > 0 {
		if roles, rolesErr := rolesResolver.ResolveRoles(r.Context(), userID); rolesErr == nil {
			ctx.User.Roles = roles
		} else {
			slog.Warn("lazuli: session roles resolver error", "error", rolesErr)
		}
	}
	if orgID > 0 {
		ctx.Tenant = &Tenant{OrgID: orgID}
	}
	ctx.SessionID = sessionID
	ctx.SessionToken = token
}

func sessionExpiredError(ctx *Ctx) error {
	if ctx == nil || ctx.SessionExpiredAt == nil || ctx.Actor != ActorAnonymous {
		return nil
	}
	return &Error{Status: http.StatusUnauthorized, Code: CodeTokenExpired,
		Message: "session expired", MessageKey: CodeTokenExpired}
}

// extractSessionToken returns the opaque session token from the
// request, preferring the `lazuli_session` cookie over the
// `Authorization: Bearer <token>` header. Returns "" when neither
// path supplies a value.
func extractSessionToken(r *http.Request) string {
	if cookie, err := r.Cookie(ProductionSessionCookieName); err == nil && cookie.Value != "" {
		return cookie.Value
	}
	authz := r.Header.Get("Authorization")
	if authz == "" {
		return ""
	}
	const prefix = "Bearer "
	if len(authz) <= len(prefix) || !strings.EqualFold(authz[:len(prefix)], prefix) {
		return ""
	}
	return strings.TrimSpace(authz[len(prefix):])
}

// extractRefreshToken returns the long-lived refresh token from the
// request. Prefers the `lazuli_refresh` cookie (browser flow); falls
// back to `X-Lazuli-Refresh` (bearer/mobile flow where the SDK keeps
// the refresh token outside HttpOnly storage). Returns "" when neither
// path supplies a value.
func extractRefreshToken(r *http.Request) string {
	if cookie, err := r.Cookie(ProductionRefreshCookieName); err == nil && cookie.Value != "" {
		return cookie.Value
	}
	return strings.TrimSpace(r.Header.Get("X-Lazuli-Refresh"))
}

// SessionCookieTTL is the default TTL used by `Ctx.SetSessionCookie`
// when callers don't pass an explicit TTL. Mirrors the canonical
// `7 days` shipped by the `auth.sessions` declaration in the the canonical pilot
// account feature.
const SessionCookieTTL = 7 * 24 * time.Hour

// parseRolesCSV splits "admin, sales,  ops" into ["admin", "sales", "ops"],
// dropping empty entries and trimming whitespace.
func parseRolesCSV(s string) []string {
	if s == "" {
		return nil
	}
	parts := strings.Split(s, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if t := strings.TrimSpace(p); t != "" {
			out = append(out, t)
		}
	}
	return out
}
