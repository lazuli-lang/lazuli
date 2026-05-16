package lazuli

import (
	"context"
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
}

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
