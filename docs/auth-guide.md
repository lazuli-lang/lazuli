# Lazuli Auth Guide

## Declaring auth in DSL

Declare auth inside the feature that owns the identity and session resources. The canonical DSL form uses indented children, not `key: value` pairs:

```lzi
feature account
  domain
    resource User
      email: @semantic.Email @pii.contact required
      password_hash: @cap.Hashed(algorithm:argon2id) optional

    resource UserSession
      user: User required
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      parent_session_id: UserSession optional
      theft_detected_at: DateTime optional
      refresh_expires_at: DateTime optional
      expires_at: DateTime required

  auth
    identity User.email

    password
      algorithm argon2id
      hash @fn.hash_password
      verify @fn.verify_password
      rate_limit "5 per 10 minutes per ip"

    sessions
      resource UserSession
      access_ttl "15 minutes"
      rotation
        refresh_ttl "14d"
        grace "1m"
        theft_detection_action revoke_session_family

    oauth google
      adapter @adapter.google_oauth
```

`auth identity` points at the resource field used for login lookup. `auth password` lowers to a password contract; `argon2id` is the canonical v0 algorithm. `auth sessions` names the persisted session resource and either its legacy single-token TTL or its access/refresh rotation discipline. Each `auth oauth <provider>` lowers to one provider contract with an adapter reference resolved at boot.

## What gets emitted

- `dist/go/<feature>/auth.gen.go` carries the lowered auth contracts:
  - `auth.FieldRef` for `auth identity`.
  - `auth.PasswordContract` for `auth password`.
  - `auth.SessionsContract` for `auth sessions`.
  - `auth.OAuthContract` for each `auth oauth <provider>`.
  - `auth.MfaContract` when `mfa` is declared.
- `dist/go/migrations/*.sql` carries DDL for declared resources, including the session resource named by `auth sessions resource ...`.
- Generated Go imports `lazuli.dev/runtime/lazuli/auth`; session contracts also import `time` because generated TTLs lower to `time.Duration`.
- Auth codegen emits contracts, not provider secrets. OAuth `ClientID`, `ClientSecret`, redirect URL, and scopes are populated by runtime adapters or boot code.
- HTTP mounting belongs to generated commands/APIs or the app transport. Use canonical public routes such as `/auth/login`, `/auth/signup`, `/auth/logout`, and `/auth/oauth/google` when wiring the web layer to these contracts.

## Common patterns

### Protected endpoint

Gate a command or API with an authenticated policy, then let session middleware populate `Ctx.User` from the `__lazuli_session` cookie:

```lzi
command update_profile
  input
    display_name: Text
  policy @policy.authenticated
  updates User
    display_name = input.display_name
```

Runtime references:

- `runtime/go/lazuli/auth/middleware.go` reads the session cookie, resolves the session, and applies it to `lazuli.Ctx`.
- `runtime/go/lazuli/auth/session.go` issues, resolves, and invalidates persisted sessions.

### Password login

Use a public login command/API that maps its identity field into `auth.PasswordLoginInput.Identity`, then call `auth.LoginPassword` with the generated contracts:

```go
session, err := auth.LoginPassword(ctx, accountgen.AccountAuthPassword, accountgen.AccountAuthSessions, auth.PasswordLoginInput{
	Identity: input.Email,
	Password: input.Password,
}, lookupUserByEmail)
```

On success, the transport sets `session.SessionToken` as `session.CookieName` with `auth.WriteSessionCookie`. Unknown identities should return `auth.ErrPasswordMismatch` from the lookup function so login does not reveal whether an account exists.

### Role-based authz

Use a policy that requires an admin role:

```lzi
command delete_user
  input
    user_id: ID
  policy @policy.admin
```

The Go runtime authorization engine is `runtime/go/lazuli/authz/policy.go`. It evaluates explicit `authz.Rule` entries first, then falls back to `authz.RoleGraph` permissions and role inheritance.

### Route guards

Backend policy still gates commands, queries, and APIs. Route guards apply the
same policy vocabulary before a screen renders:

```lazuli
app AcmeCRM
  actor_query account.query.me
  route_guard
    default_policy @scope.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
```

`actor_query` resolves the current `LazuliActor | null`; route guard policies
use that actor to decide whether to render, redirect as unauthenticated, or
redirect as unauthorized. Views and audiences may override the app fallback with
their own `policy @policy.<name>` guard. The resolved view guard must be at
least as strict as every backend command/query the view `submit`s or `source`s,
so users do not see screens that only fail at API time.

### OAuth-only mode

Skip `auth password` and declare one or more OAuth providers:

```lzi
auth
  identity User.email

  sessions
    resource UserSession
    access_ttl "15 minutes"
    rotation

  oauth google
    adapter @adapter.google_oauth
```

Wire the transport to `auth.OAuthRedirect` for the consent redirect and `auth.OAuthCallback` for the callback. The current runtime includes provider helpers for Google, GitHub, Microsoft, and Apple under `runtime/go/lazuli/auth/oauth*.go`; built-in endpoint wiring is implemented for Google, while other providers use their adapter descriptors.

## Rotation discipline

Enable `auth.sessions.rotation` for production applications that keep users
signed in across browser restarts or protect sensitive operations. The legacy
single-token `ttl` shape is useful for tests and low-risk prototypes, but a
long-lived access token increases the blast radius of a stolen cookie.

Keep `access_ttl` short. `15 minutes` is the canonical default because it limits
the value of a stolen access token while keeping refresh traffic manageable. Set
`refresh_ttl` to the period users reasonably expect to stay signed in; the
framework default is `14d`, which is the web sweet spot for most applications.
Shorten it for admin consoles and regulated workflows. Lengthen it only when
the product explicitly prioritizes low-friction re-entry and has strong theft
response.

Use `theft_detection_action revoke_session_family` by default. It revokes the
`parent_session_id` chain for the compromised device while leaving the user's
other devices signed in. Switch to `revoke_user` for high-stakes applications
such as banking, clinical systems, or privileged operator tools where reuse of a
rotated refresh token should invalidate every session for that user.

Worked fixture slice from `examples/full-capsule/full-capsule.lzi`:

```lzi
    sessions
      resource CustomerSession
      ttl "7 days"
      access_ttl "15 minutes"
      rotation
        refresh_ttl "30 days"
        grace "30 seconds"
        theft_detection_action revoke_session_family
```

That fixture authors explicit values to exercise codegen. In an app that accepts
the framework defaults, the body can be as small as:

```lzi
    sessions
      resource UserSession
      access_ttl "15 minutes"
      rotation
```

The session resource must carry the rotation columns
`refresh_token_hash`, `parent_session_id`, `theft_detected_at`, and
`refresh_expires_at`; migration codegen adds equivalent columns when the
authored resource omits them. Protect `refresh_token_hash` with field policy so
only system actors can read or write it.

## Production checklist

- [ ] `LAZULI_DB` set.
- [ ] `LAZULI_AUTH_COOKIE_DOMAIN` or equivalent boot config set when cookies must work across subdomains.
- [ ] Access cookie/header and refresh cookie are separate; the refresh cookie is `HttpOnly` and scoped to the refresh endpoint.
- [ ] Session cookies use `Secure`, `HttpOnly`, explicit paths, and appropriate `SameSite` policy.
- [ ] Session cleanup cron deletes expired rows from the declared session resource.
- [ ] Audit log retention policy is declared and operationally enforced.
- [ ] OAuth redirect URLs are registered with each provider.
- [ ] OAuth client secrets are provided through env or a secret manager, not DSL source.
- [ ] Password lookup returns the same auth error for unknown identity and wrong password.

## Runtime references

- `runtime/go/lazuli/auth/login.go` - `AuthSession`, `PasswordLoginInput`, `LoginPassword`.
- `runtime/go/lazuli/auth/password.go` - `PasswordContract`, `HashPassword`, `VerifyPassword`, `AlgoArgon2id`.
- `runtime/go/lazuli/auth/middleware.go` - session cookie middleware and `Ctx` projection.
- `runtime/go/lazuli/auth/session.go` - `SessionsContract`, `IssueSession`, `ResolveSession`, `InvalidateSession`.
- `runtime/go/lazuli/auth/refresh.go` - refresh-token rotation, grace window, and theft action.
- `runtime/go/lazuli/auth/middleware_refresh.go` - expired-access-token signal for auto-refresh.
- `runtime/go/lazuli/auth/session_cleanup.go` - expired-session cleanup helper.
- `runtime/go/lazuli/auth/jwt.go` - JWT helpers for token-backed integrations.
- `runtime/go/lazuli/auth/oauth.go` and `runtime/go/lazuli/auth/oauth*.go` - OAuth contracts, state, PKCE, and provider helpers.
- `runtime/go/lazuli/authz/policy.go` - policy evaluation.
- `runtime/go/lazuli/authz/rbac.go` and `runtime/go/lazuli/authz/role_inheritance.go` - role permissions and inheritance.
- `crates/lazuli_codegen_go/src/emitter/auth.rs` - Go auth contract emitter.
- `runtime/ts/lazuli/src/client.ts` - `enableAutoRefresh`, single-flight refresh, and retry-once behavior.

## Smoke tests

Run the auth emitter and runtime tests:

```bash
cargo test -p lazuli_codegen_go auth
go test ./runtime/go/lazuli/auth ./runtime/go/lazuli/authz
```

Run the current Go codegen smoke suite when you need full generated-server coverage:

```bash
cargo test -p lazuli_codegen_go --features smoke_e2e
```
