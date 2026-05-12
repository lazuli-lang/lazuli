# Bucket Cycle: Auth (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=auth` pipeline.
Implementation deferred to a separate run with `mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-10.

## Contexto

The canonical fixture authors a full `auth` block inside
`feature customer_auth` (`examples/full-capsule/full-capsule.lzi:504-523`)
covering identity, password (+ algorithm + rate_limit), oauth, mfa
(totp), and sessions. The typed IR shape already exists in
`crates/lazuli_ir/src/lib.rs:1797-1853` (`Auth`, `AuthIdentity`,
`AuthPassword`, `AuthSessions`, `AuthMfa`, `AuthOAuthProvider`). What is
missing is **lowering**: `parse_feature_skeleton`
(`crates/lazuli_syntax/src/parser.rs:1137-1183`) silently drops the
whole block (`:1168-1173`), so `Feature.auth` is permanently `None`,
`lazuli inspect` cannot project it, and doctor cannot cross-check
between auth slots and the surrounding feature.

The lowering route was decided in
`docs/proposals/auth-lowering-scope.md` (canonical input for this run):
**Route A** — extend the canonical-indent slice in `lazuli_syntax`
to recognise `auth` alongside `agent`. Scope is the **6 children already
authored** in the fixture (`identity`, `password` + `algorithm` +
`rate_limit`, `oauth google`, `mfa totp`, `sessions`); the 10 other
roadmap §1.8 constructs are speculative and dropped from this design.

The closed-cycle criterion (4 doctor diagnostics, `--expand=auth`
projection, LSP hover/completion, golden evals on login/mfa/oauth) is
the acceptance gate. This proposal specifies the design for every
stage of that gate so the implementation run is mechanical.

## Baseline (Stages 1-2 inventory)

Copied verbatim from the pipeline run that produced
`auth-lowering-scope.md`.

| Layer | Status | Anchor |
|---|---|---|
| Surface syntax (`.lzi`) | authored, 6 children | `examples/full-capsule/full-capsule.lzi:504-523` |
| IR (`crates/lazuli_ir`) | typed, complete for 6 children | `crates/lazuli_ir/src/lib.rs:1797-1853` |
| Parser slice | silently skipped | `crates/lazuli_syntax/src/parser.rs:1168-1173` |
| LSP (file-local text walk) | shape-only diagnostics (`auth-password-algorithm`, `auth-password-rate-limit`, `auth-session-ttl`) | `crates/lazuli_lsp/src/lib.rs:9420-9516` |
| Doctor cross-feature | none (no IR to read) | n/a |
| Inspect projection | none | n/a |
| Runtime (Lazuli Go) | only `populateDevSession` (X-Lazuli-* headers) | `runtime/go/lazuli/session.go:1-60` |
| Highlighting | none for `auth`/`identity`/`mfa`/`sessions`/`oauth`/`algorithm`/`ttl`/`refresh`/`enroll`/`verify`/`adapter` slots | `editors/vscode/syntaxes/lazuli.tmLanguage.json` |
| Closed-catalog enforcement | partial — `@cap.Hashed(algorithm:…)` accepts `argon2id`/`bcrypt` (`crates/lazuli_lsp/src/lib.rs:2753-2760`); no enforcement on `auth password algorithm` | mismatched |

**Cross-cutting fact**: `@cap.Hashed(algorithm:argon2id)` on
`CustomerSession.refresh_token_hash`
(`examples/full-capsule/full-capsule.lzi:451`) is the typed counterpart
of `auth password algorithm argon2id`
(`examples/full-capsule/full-capsule.lzi:508`). Today nothing checks
that the two axes agree.

## Linguagem (Stage 3)

Surface is canonical — already authored, already audited. Stage 3 is
**documentation + closed-catalog tightening**, not invention.

### Formal grammar (EBNF, draft for `docs/grammar.lzi.md`)

```ebnf
auth_block        = "auth" NEWLINE INDENT
                    auth_identity
                    [ auth_password ]
                    { auth_oauth }
                    [ auth_mfa ]
                    [ auth_sessions ]
                    DEDENT ;

auth_identity     = "identity" qualified_field NEWLINE ;
                    (* Resource.field reference, e.g. Customer.email *)

auth_password     = "password" NEWLINE INDENT
                    auth_pw_algorithm
                    auth_pw_hash
                    auth_pw_verify
                    [ auth_pw_rate_limit ]
                    DEDENT ;

auth_pw_algorithm = "algorithm" identifier NEWLINE ;     (* closed catalog *)
auth_pw_hash      = "hash"      fn_ref NEWLINE ;         (* @fn.* *)
auth_pw_verify    = "verify"    fn_ref NEWLINE ;         (* @fn.* *)
auth_pw_rate_limit= "rate_limit" string NEWLINE ;        (* "N per <window> [per <key>]" *)

auth_oauth        = "oauth" identifier NEWLINE INDENT    (* provider id; closed *)
                    "adapter" adapter_ref NEWLINE        (* @adapter.* *)
                    DEDENT ;

auth_mfa          = "mfa" identifier NEWLINE INDENT      (* method; closed *)
                    auth_mfa_enroll
                    auth_mfa_verify
                    [ auth_mfa_adapter ]
                    DEDENT ;

auth_mfa_enroll   = "enroll" fn_ref NEWLINE ;            (* @fn.* *)
auth_mfa_verify   = "verify" ( fn_ref | validator_ref ) NEWLINE ;
auth_mfa_adapter  = "adapter" adapter_ref NEWLINE ;

auth_sessions     = "sessions" NEWLINE INDENT
                    "resource" resource_name NEWLINE     (* same-feature resource *)
                    "ttl" string NEWLINE                 (* "<N> <unit>" *)
                    [ "refresh" boolean NEWLINE ]
                    DEDENT ;
```

### Slot inventory (required/optional + type + closed catalog)

| Slot | Required | Type | Closed catalog | Fixture anchor |
|---|---|---|---|---|
| `auth identity <Resource.field>` | yes (the whole `auth` block requires it; IR field is non-optional — `crates/lazuli_ir/src/lib.rs:1798`) | `FieldRef` | no | `full-capsule.lzi:505` |
| `auth password` | optional | block | n/a | `full-capsule.lzi:507` |
| `auth password algorithm <name>` | required when `password` declared | identifier | **closed**: `argon2id`, `bcrypt` (matches `crates/lazuli_lsp/src/lib.rs:2753`) | `full-capsule.lzi:508` |
| `auth password hash @fn.<name>` | required when `password` declared | `@fn.*` ref | no | `full-capsule.lzi:509` |
| `auth password verify @fn.<name>` | required when `password` declared | `@fn.*` ref | no | `full-capsule.lzi:510` |
| `auth password rate_limit "<rule>"` | optional (warned today; **upgrade to required for `auth password` in strict profile**) | string | no — adapter-parsed | `full-capsule.lzi:511` |
| `auth oauth <provider>` | optional, repeatable | identifier | **closed v0**: `google`, `github`, `microsoft`, `apple`. Other providers require pilot evidence. | `full-capsule.lzi:513` |
| `auth oauth <provider> adapter @adapter.<x>` | required when `oauth <provider>` declared | `@adapter.*` ref | no | `full-capsule.lzi:514` |
| `auth mfa <method>` | optional | identifier | **closed v0**: `totp` only. `webauthn`/`recovery_codes`/`sms` deferred to pilot. | `full-capsule.lzi:516` |
| `auth mfa <method> enroll @fn.<x>` | required when `mfa` declared | `@fn.*` ref | no | `full-capsule.lzi:517` |
| `auth mfa <method> verify <@fn.\|@validator.>` | required when `mfa` declared | `@fn.*` or `@validator.*` | no | `full-capsule.lzi:518` |
| `auth mfa <method> adapter @adapter.<x>` | optional | `@adapter.*` ref | no | not in fixture; IR carries it (`crates/lazuli_ir/src/lib.rs:1844`) |
| `auth sessions` | optional | block | n/a | `full-capsule.lzi:520` |
| `auth sessions resource <Resource>` | required when `sessions` declared | resource name (same feature) | no | `full-capsule.lzi:521` |
| `auth sessions ttl "<rule>"` | required when `sessions` declared (warned today; upgrade to required) | string | no — adapter-parsed | `full-capsule.lzi:522` |
| `auth sessions refresh <bool>` | optional, default `false` | boolean | yes (`true`/`false`) | `full-capsule.lzi:523` |

### Closed-catalog rationale

- `algorithm ∈ {argon2id, bcrypt}` already enforced for `@cap.Hashed`
  (`crates/lazuli_lsp/src/lib.rs:2753-2760`). The auth-block axis must
  share the catalog — divergence is exactly the
  `auth_password_algorithm_hash_mismatch` bug.
- `oauth <provider> ∈ {google, github, microsoft, apple}` covers the
  four providers the Lazuli Go adapter has shipped for in `runtime/go/lazuli`
  surface today (only `google` is in fixture; the other three are
  named explicitly to let LSP completion offer them without inventing
  shape). Any non-listed provider triggers a `warning` (not error):
  pilot-driven extension.
- `mfa <method> ∈ {totp}` — fixture only authors `totp`. Other methods
  documented as deferred so authors don't try `webauthn` and get
  silent skipping.

## IR (Stage 4)

The IR struct already covers the 6 children. Confirmed walk:

| Surface slot | IR field | Status |
|---|---|---|
| `identity <Resource.field>` | `Auth.identity: AuthIdentity { field: FieldRef }` | present (`lazuli_ir/src/lib.rs:1798`, `:1813-1815`) |
| `password algorithm <X>` | **gap** — `AuthPassword` carries `hash`, `verify`, `rate_limit` but **no `algorithm` field** | `lazuli_ir/src/lib.rs:1817-1826` |
| `password hash @fn.X` | `AuthPassword.hash: String` | `:1820` |
| `password verify @fn.X` | `AuthPassword.verify: String` | `:1821` |
| `password rate_limit "..."` | `AuthPassword.rate_limit: Option<String>` | `:1825` |
| `oauth <provider> adapter @adapter.X` | `AuthOAuthProvider { provider, adapter }` | `:1848-1853` |
| `mfa <method> enroll @fn.X` | **gap** — `AuthMfa` has `method` + `adapter` but **no `enroll` / `verify` fields** | `:1839-1844` |
| `mfa <method> verify @fn.X` | **gap** — same as above | `:1839-1844` |
| `sessions resource X` | `AuthSessions.resource: QualifiedName` | `:1831` |
| `sessions ttl "..."` | `AuthSessions.ttl: String` | `:1833` |
| `sessions refresh <bool>` | `AuthSessions.refresh: bool` | `:1835` |

### IR gaps to close before lowering

Two additive fields on existing structs. No new struct.

1. **`AuthPassword.algorithm: String`** (required). Without it the
   cross-check vs `@cap.Hashed(algorithm:…)` has no IR axis to read.
   Position: between `AuthPassword.verify` and
   `AuthPassword.rate_limit` so JSON order stays readable.

2. **`AuthMfa.enroll: String`** (required) and
   **`AuthMfa.verify: String`** (required). Both are `@fn.*` or
   `@validator.*` qualified refs. Today's IR has only `method` +
   `adapter`, which is insufficient — the fixture authors both
   `enroll @fn.enroll_customer_totp` and
   `verify @validator.verify_customer_totp`
   (`full-capsule.lzi:517-518`) and an MFA flow needs both endpoints.

Both gaps are mechanical struct extensions; no schema-breaking changes
because no on-disk JSON consumer reads `Auth` yet (the field is always
`None` due to the parser skip).

### Inspect JSON shape (`lazuli inspect --format=json --expand=auth`)

New top-level `--expand=auth` flag in `ExpandSet`
(`crates/lazuli_cli/src/main.rs:98-118`). Projection:

```json
{
  "features": [
    {
      "name": "customer_auth",
      "auth": {
        "identity": { "field": "Customer.email" },
        "password": {
          "algorithm": "argon2id",
          "hash": "@fn.hash_customer_password",
          "verify": "@fn.verify_customer_password",
          "rate_limit": "5 per 10 minutes"
        },
        "sessions": {
          "resource": "CustomerSession",
          "ttl": "7 days",
          "refresh": false
        },
        "mfa": {
          "method": "totp",
          "enroll": "@fn.enroll_customer_totp",
          "verify": "@validator.verify_customer_totp"
        },
        "oauth": [
          { "provider": "google", "adapter": "@adapter.google_oauth" }
        ]
      }
    }
  ]
}
```

Normalisation rules:

- `identity.field` is the verbatim `Resource.field` written in source.
- All `@fn.*`, `@validator.*`, `@adapter.*` refs preserve the leading
  `@` sigil (matches the convention in
  `crates/lazuli_ir/src/lib.rs:1819-1820`).
- `oauth` is an array even with a single provider; consumers iterate
  unconditionally.
- `refresh` defaults to `false`; serialised explicitly when present.
- Without `--expand=auth` the `auth` key is **omitted entirely** from
  the inspect output (mirrors the `tools` and `expose` convention,
  `crates/lazuli_cli/src/main.rs:4774`, `:4801`).

### Cross-refs the analyzer must register (no implementation here)

Lower into typed IR, then the expand pass walks four edges:

| Edge | Source field | Target | Resolution scope |
|---|---|---|---|
| `auth.identity` | `Auth.identity.field` | `Resource.field` declared in the same feature | feature-local; record use in `Feature.dependencies` |
| `auth.password.algorithm` | `Auth.password.algorithm` | `@cap.Hashed(algorithm:…)` on a hash-shaped field of the session resource | same feature; via `Auth.sessions.resource` → `Resource` → fields tagged `@cap.Hashed` |
| `auth.oauth.<p>.adapter` | `AuthOAuthProvider.adapter` | `extension adapter <name>` (`full-capsule.lzi:550`) or `registry.lzi` `integrations` slot | feature `extensions` first, fall through to `registry.integrations` |
| `auth.mfa.<m>.verify` | `Auth.mfa.verify` | `@validator.<name>` declared in `feature.extensions` | feature-local |
| `auth.sessions.resource` | `Auth.sessions.resource` | `Resource` declaration in the same feature | feature-local; cross-check that resource has a field carrying `@cap.Hashed` and an `expires_at: DateTime` field consistent with `ttl` |

The cross-ref shape mirrors the existing agent/tool expansion in
`crates/lazuli_cli/src/main.rs:1060` (per-agent dispatch graph
projection).

## Codegen (Stage 5)

Three new generated files under `dist/go/<feature>/`. Output is
skeletal — the Lazuli Go runtime supplies the runtime body — and follows the existing
`dist/go/customer/customer.gen.go` style.

### `dist/go/customer_auth/auth.gen.go`

```go
// path: dist/go/customer_auth/auth.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer_auth

import (
    "github.com/lazuli/runtime/go/lazuli"
    "github.com/lazuli/runtime/go/lazuli/auth"
)

// PasswordContract is the lowered `auth password` block from
// feature customer_auth, identity Customer.email.
var PasswordContract = auth.PasswordContract{
    Identity:   lazuli.FieldRef{Resource: "Customer", Field: "email"},
    Algorithm:  auth.AlgoArgon2id,
    HashFn:     "@fn.hash_customer_password",
    VerifyFn:   "@fn.verify_customer_password",
    RateLimit:  "5 per 10 minutes",
}

// Login handles `command login` once the password contract resolves.
// Returns a session token + sets the session cookie via auth.Sessions.
func Login(ctx *lazuli.Ctx, in LoginInput) (LoginOutput, error) {
    return auth.LoginPassword(ctx, PasswordContract, in.Email, in.Password)
}

// Logout invalidates the current session.
func Logout(ctx *lazuli.Ctx) error {
    return auth.LogoutSession(ctx, SessionsContract)
}
```

### `dist/go/customer_auth/session.gen.go`

```go
// path: dist/go/customer_auth/session.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer_auth

import (
    "time"

    "github.com/lazuli/runtime/go/lazuli/auth"
)

// SessionsContract is the lowered `auth sessions` block.
var SessionsContract = auth.SessionsContract{
    Resource: "CustomerSession",       // resolves to CustomerSessionResource
    TTL:      7 * 24 * time.Hour,      // "7 days" parsed by Lazuli
    Refresh:  false,
}

// IssueSession persists a CustomerSession row and returns the cookie value.
func IssueSession(ctx *lazuli.Ctx, customerID lazuli.ID, provider AuthProvider) (string, error) {
    return auth.IssueSession(ctx, SessionsContract, customerID, provider)
}

// ResolveSession is called from the HTTP middleware to populate Ctx.User.
func ResolveSession(ctx *lazuli.Ctx, cookie string) error {
    return auth.ResolveSession(ctx, SessionsContract, cookie)
}
```

### `dist/go/customer_auth/mfa.gen.go`

```go
// path: dist/go/customer_auth/mfa.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer_auth

import "github.com/lazuli/runtime/go/lazuli/auth"

// MfaContract is the lowered `auth mfa totp` block.
var MfaContract = auth.MfaContract{
    Method:   auth.MfaMethodTOTP,
    EnrollFn: "@fn.enroll_customer_totp",       // resolves at boot
    VerifyFn: "@validator.verify_customer_totp",
}

// EnrollMFA dispatches to the registered enrol fn and returns the secret/QR.
func EnrollMFA(ctx *lazuli.Ctx, customerID lazuli.ID) (auth.MfaEnrolment, error) {
    return auth.EnrollMFA(ctx, MfaContract, customerID)
}

// VerifyMFA is the validator hook called by `command enable_mfa`.
// The caller is responsible for loading the persisted TOTP secret
// from the identity resource (e.g. `Customer.mfa_secret`) — the runtime
// helper does not touch the DB. This split keeps `runtime/go/lazuli/auth/`
// free of pgx coupling; the secret is threaded explicitly.
func VerifyMFA(ctx *lazuli.Ctx, customerID lazuli.ID, secret, code string) error {
    return auth.VerifyMFA(ctx, MfaContract, customerID, secret, code)
}
```

OAuth dispatch lives in a per-provider file:

### `dist/go/customer_auth/oauth_google.gen.go`

```go
// path: dist/go/customer_auth/oauth_google.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer_auth

import "github.com/lazuli/runtime/go/lazuli/auth"

var OAuthGoogle = auth.OAuthContract{
    Provider:    "google",
    AdapterRef:  "@adapter.google_oauth",
}

func OAuthGoogleRedirect(ctx *lazuli.Ctx) (string, error) {
    return auth.OAuthRedirect(ctx, OAuthGoogle)
}

func OAuthGoogleCallback(ctx *lazuli.Ctx, code, state string) (string, error) {
    return auth.OAuthCallback(ctx, OAuthGoogle, SessionsContract, code, state)
}
```

The web flow has two requests (`/login/google` → `/login/google/callback`) so
the state token must persist across them via a same-site cookie. The runtime
exposes `auth.StashOAuthState(ctx, provider, state)` and
`auth.LoadOAuthState(ctx, provider)` so transports can thread the cookie value
through ctx before calling `OAuthCallback`. `OAuthRedirect` itself mints state
inline (single-process flows / unit tests); transports that own state minting
can call `auth.GenerateOAuthState()` + `auth.BuildOAuthConfig(contract)` +
`cfg.AuthCodeURL(state, …)` directly and bypass `OAuthRedirect`.

### Types reused from `runtime/go/lazuli`

- `lazuli.Ctx` (`runtime/go/lazuli/ctx.go`) — request context, actor,
  tenant.
- `lazuli.ID` (`runtime/go/lazuli/types.go`) — typed id for
  customer/session.
- `lazuli.Error` (`runtime/go/lazuli/error.go`) — typed error envelope
  with `Code` + `Status` + `Message`, mapped to `expose client` shape.
- `lazuli.FieldRef` — already used by validators / commands.
- `withTx(ctx, fn)` helper from `runtime/go/lazuli/db.go` (used by
  `Command.Handle` at `runtime/go/lazuli/handle.go:27`) wraps
  `IssueSession` and `VerifyMFA`.

## Runtime (Stage 6)

Four new capability files under `runtime/go/lazuli/auth/`. Boundary
discipline: **the language references these by capability, never by
concrete provider**. Adapters (Google OAuth client, Twilio SMS,
WebAuthn server) sit in `@runtime/...` or `@plugin/...` packages and
plug in via `@adapter.*` resolution.

### `runtime/go/lazuli/auth/password.go`

- **Capability**: hash + verify password bytes.
- **Lifecycle**: stateless. Called from generated `Login` and from the
  registered `@fn.hash_customer_password` body when authors choose to
  delegate (default `customer_password.go.tmpl`).
- **Config**: reads `auth.PasswordContract.Algorithm`. Argon2id
  parameters (`m=64MiB`, `t=3`, `p=4`) hard-coded in v0; promotion to
  `auth password algorithm argon2id(memory:64MiB, time:3, parallelism:4)`
  is a pilot-gated language extension, **not** v0.
- **Dependency**: `golang.org/x/crypto/argon2`,
  `golang.org/x/crypto/bcrypt`.
- **Typed errors**:
  - `ErrPasswordMismatch` → mapped to `expose client` status 401, code
    `auth.password_mismatch`.
  - `ErrPasswordRateLimited` → 429, `auth.rate_limited`.

### `runtime/go/lazuli/auth/session.go`

- **Capability**: issue / resolve / invalidate sessions backed by the
  declared `auth sessions resource`.
- **Lifecycle**: per-request. `IssueSession` writes a row to the
  resource's table (via the generated `CustomerSessionResource.Create`
  helper), sets a `__lazuli_session` HTTP-only secure cookie with the
  hashed token. `ResolveSession` is wired as middleware in `main.go`,
  replacing the dev-mode `populateDevSession`
  (`runtime/go/lazuli/session.go:26-53`).
- **Config**: reads `auth.SessionsContract.TTL` and `.Refresh`. Cookie
  name / SameSite policy from `app.lzi` `urls` / `cors`. Resource
  table & columns from the codegen of the declared resource.
- **Typed errors**:
  - `ErrSessionExpired` → 401, `auth.session_expired`.
  - `ErrSessionNotFound` → 401, `auth.session_unknown`.

### `runtime/go/lazuli/auth/oauth.go`

- **Capability**: per-provider redirect + callback dispatch. The
  language declares the provider id + adapter ref; the runtime resolves
  the adapter to a concrete OAuth2 client at boot.
- **Lifecycle**: stateless dispatcher. Adapters
  (`@runtime/google_oauth`, etc.) own `golang.org/x/oauth2.Config`
  instances built from `registry.lzi` `integrations` config (client
  id, secret, redirect uri).
- **Config**: looks up `auth.OAuthContract.AdapterRef` in the boot-time
  adapter registry (`runtime/go/lazuli/register.go`).
- **Typed errors**:
  - `ErrOAuthStateMismatch` → 400, `auth.oauth_state_invalid`.
  - `ErrOAuthAdapterUnregistered` → 500,
    `auth.oauth_adapter_unbound` (compile-time prevented by the
    `auth_oauth_adapter_unbound` doctor diagnostic; this is the
    runtime safety net).

### `runtime/go/lazuli/auth/mfa.go`

- **Capability**: enrol + verify per-method MFA. v0 ships TOTP only.
- **Lifecycle**: stateless dispatcher; `Enroll` calls the registered
  `@fn.*`; `Verify` calls the registered `@validator.*`.
- **Config**: `auth.MfaContract.Method` selects the dispatcher.
  Adapter ref (optional) resolves to TOTP issuer/digits/period.
- **Dependency**: `github.com/pquerna/otp/totp`.
- **Typed errors**:
  - `ErrMfaCodeInvalid` → 400, `auth.mfa_code_invalid`.
  - `ErrMfaNotEnrolled` → 409, `auth.mfa_not_enrolled`.
  - `ErrMfaMethodUnsupported` → 500, `auth.mfa_method_unsupported`
    (compile-time prevented by closed-catalog enforcement).

## Evals/Testes (Stage 7)

### Golden eval — login (password)

`tests/golden/auth/login_password.jsonl` (single line, formatted for
readability):

```jsonl
{
  "name": "login_password_happy",
  "input": { "email": "alice@example.com", "password": "Tr0ub4dor&3" },
  "preconditions": {
    "Customer": [{ "id": 1, "email": "alice@example.com" }],
    "@fn.hash_customer_password": "stored:argon2id$..."
  },
  "expect": {
    "command": "customer_auth.login",
    "status": 200,
    "emits": ["customer_logged_in"],
    "creates_resource": "CustomerSession",
    "sets_cookie": "__lazuli_session"
  }
}
```

### Golden eval — TOTP enable + verify

`tests/golden/auth/mfa_totp.jsonl`. Two-step: enrol returns a
secret/qr; subsequent `enable_mfa` command with the rotating code
succeeds via `@validator.verify_customer_totp`. Asserts
`CustomerMfaConfig` row created (`full-capsule.lzi:457-459`).

### Golden eval — OAuth Google flow

`tests/golden/auth/oauth_google.jsonl`. Three-step: redirect emits a
location with state cookie; callback with matching state + provider
`code` issues a session bound to `provider: google`
(`full-capsule.lzi:453, 479`); event `customer_logged_in` emitted with
`provider: google`.

### Go sync test — session expiry

`runtime/go/lazuli/auth/auth_test.go` using `testing/synctest`:

- Issue session with `TTL: 7 * 24 * time.Hour`.
- Advance synthetic clock past TTL.
- `ResolveSession` returns `ErrSessionExpired`.
- Refresh disabled → no rotation observed.

### Doctor fixture — algorithm mismatch

`crates/lazuli_cli/tests/fixtures/auth/algorithm_mismatch.lzi`
(minimal `.lzi`):

```lzi
feature x_auth
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
  auth
    identity Session.id
    password
      algorithm bcrypt
      hash @fn.h
      verify @fn.v
    sessions
      resource Session
      ttl "1 day"
```

Asserts that doctor emits **exactly one**
`auth_password_algorithm_hash_mismatch` diagnostic at the
`algorithm bcrypt` line.

### LSP test — algorithm hover + completion

`crates/lazuli_lsp/tests/auth.rs`:

- Hover on `algorithm` keyword shows closed catalog
  `argon2id | bcrypt`.
- Completion at column after `algorithm ` offers exactly those two
  identifiers.
- Hover on `algorithm bcrypt` while a sibling resource declares
  `@cap.Hashed(algorithm:argon2id)` includes a warning hint mirroring
  the doctor diagnostic.

## Doctor/LSP (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `auth_password_algorithm_hash_mismatch` | error | "auth.password.algorithm `<X>` must match `@cap.Hashed(algorithm:<X>)` on the session resource's hash field (found `<Y>` on `<Resource>.<field>`)." | `Auth.password.algorithm != algorithm axis of @cap.Hashed on session resource's `@cap.Hashed`-tagged field | `algorithm_mismatch.lzi` above |
| `auth_sessions_resource_unknown` | error | "auth.sessions.resource `<X>` does not name a resource declared in feature `<feature>`." | `Auth.sessions.resource` does not resolve to any `Resource` declaration in the same `Feature` | minimal `.lzi` with `sessions resource Bogus` and no `resource Bogus` |
| `auth_identity_field_unknown` | error | "auth.identity `<Resource>.<field>` does not resolve: `<reason>`." Reason ∈ { `resource not found`, `field not found`, `field is not identity-shaped (missing @semantic.Email / @semantic.Phone / unique)` } | `Auth.identity.field` fails resolution via `Feature.domain` | three minimal fixtures (one per reason) |
| `auth_oauth_adapter_unbound` | error | "auth.oauth.`<provider>`.adapter `<@adapter.X>` is not declared in `extensions` of feature `<feature>` or `integrations` in `registry.lzi`." | adapter ref resolves nowhere | minimal `.lzi` with `oauth google adapter @adapter.bogus` |

All four codes register under `is_security_enforcement_code`
(`crates/lazuli_lsp/src/lib.rs:9527`) so the strict + production
profiles upgrade severity to ERROR uniformly (already error here; the
profile gate is for future warning-level relatives).

### Diagnostic anchors (where to add)

- `auth_password_algorithm_hash_mismatch` — runs in
  `crates/lazuli_cli/src/doctor.rs` cross-feature pass once IR carries
  `Auth.password.algorithm`. Needs read access to the session
  resource's fields → already in `Feature.resources` after lowering.
- `auth_sessions_resource_unknown` — same pass; trivially a name
  lookup in `Feature.resources`.
- `auth_identity_field_unknown` — same pass; uses the
  `FieldRef`-resolution helper that today serves `resource <X> on_delete`
  cross-checks.
- `auth_oauth_adapter_unbound` — needs both the feature
  `extensions adapter <name>` list (already in legacy IR) and the
  `registry.integrations` map (already typed via
  `crates/lazuli_cli/src/app_manifest.rs`).

### LSP hovers (new entries)

Add to `KEYWORD_HOVER` in `crates/lazuli_lsp/src/lib.rs`:

| Keyword | Hover summary |
|---|---|
| `auth` | "Authentication block: groups identity, password, sessions, MFA, and OAuth subcontracts for a feature." |
| `identity` | "`identity <Resource>.<field>` — names the resource field used as the canonical login identifier." |
| `password` | "Password subcontract: hash + verify + algorithm (+ rate_limit)." |
| `algorithm` | "Password hash algorithm. v0: `argon2id` (recommended) \| `bcrypt` (legacy migration)." |
| `oauth` | "OAuth subcontract: `oauth <provider>` with `adapter @adapter.<x>`. v0 providers: `google`, `github`, `microsoft`, `apple`." |
| `mfa` | "MFA subcontract: `mfa <method>` with `enroll` + `verify`. v0 method: `totp`." |
| `sessions` | "Sessions subcontract: backing resource + ttl + refresh policy." |
| `ttl` | "Session/token time-to-live as a duration string parsed by the adapter (e.g., `\"7 days\"`)." |
| `refresh` | "Whether the session adapter issues refresh tokens. Default `false`." |
| `enroll` | "Enrolment function reference (`@fn.*`) returning method-specific enrolment data." |
| `verify` | "Verification reference (`@fn.*` or `@validator.*`) returning success/failure." |
| `adapter` | "Adapter slot: `@adapter.<local>` resolved against `extensions adapter <name>` or `registry.integrations`." |

Closed-catalog completions to add:

- `algorithm ` → `argon2id`, `bcrypt`.
- `oauth ` → `google`, `github`, `microsoft`, `apple`.
- `mfa ` → `totp`.
- `refresh ` → `true`, `false`.

### Namespaces (`is_allowed_reference_namespace`)

No new namespace required. All references reuse existing slots: `@fn`,
`@validator`, `@adapter` (all listed in
`crates/lazuli_lsp/src/lib.rs:2114-2135`). Catalog list stays stable.

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

Add `auth | identity | password | algorithm | oauth | mfa | sessions |
ttl | refresh | enroll | hash | verify | adapter` to the keyword
scope. The catalog literals (`argon2id`, `bcrypt`, `totp`, `google`,
`github`, `microsoft`, `apple`, `true`, `false`) hit the existing
identifier scope without explicit listing; the
`auth_password_algorithm_hash_mismatch` story relies on doctor, not
highlighting.

## Critério de "ciclo fechado"

- [ ] Fixture `feature customer_auth` exercises the 6 children (already
      true — `full-capsule.lzi:504-523`).
- [ ] `lazuli check examples/full-capsule` accepts the syntax after
      lowering lands (no regression).
- [ ] `lazuli inspect --format=json --expand=auth examples/full-capsule`
      shows the IR shape described in Stage 4 for `customer_auth`.
- [ ] `lazuli doctor` emits the 4 named diagnostics on the matching
      fixtures.
- [ ] `lazuli generate` produces `dist/go/customer_auth/{auth,session,
      mfa,oauth_google}.gen.go` that compile under
      `runtime/go/lazuli/auth`.
- [ ] Lazuli Go exposes login / logout / mfa enrol+verify / oauth
      redirect+callback end-to-end (runtime-team deliverable).
- [ ] Golden evals + the `testing/synctest` Go test for expiry pass.
- [ ] LSP hovers + completion cover the 11 keywords + 4 closed
      catalogs from Stage 8.

## Próximo passo

Human approval of this proposal + a separate `mode=implement` run
that lands Route A: extend `parse_feature_skeleton`
(`crates/lazuli_syntax/src/parser.rs:1147-1173`), add `parse_auth`
sibling to `parse_agent`, add `auth: Option<Auth>` to
`FeatureSkeleton` (`crates/lazuli_syntax/src/ast.rs:218`), extend
`AuthPassword` with `algorithm` and `AuthMfa` with `enroll`/`verify`
(`crates/lazuli_ir/src/lib.rs:1817-1844`), wire lowering, add
`ExpandSet.auth` (`crates/lazuli_cli/src/main.rs:98-118`), and ship
the four doctor diagnostics + LSP entries. Pointer:
`docs/proposals/auth-lowering-scope.md` §"Closed-cycle criterion"
remains the acceptance gate.

## Rows sugeridas para `docs/next-checklist.md`

Three additions, formatted to match the existing table:

```
| 26 | Auth bucket cycle Route A — canonical-indent slice covers `auth` | planned | Extend `parse_feature_skeleton` to recognise `auth` alongside `agent`; add `parse_auth`; wire `auth: Option<Auth>` through `FeatureSkeleton`. IR extensions: `AuthPassword.algorithm`, `AuthMfa.enroll`/`verify`. New `--expand=auth` projection. See `docs/proposals/bucket-auth-cycle.md` §Linguagem/§IR. |
| 27 | Auth bucket cycle — 4 doctor diagnostics + LSP coverage | planned | `auth_password_algorithm_hash_mismatch`, `auth_sessions_resource_unknown`, `auth_identity_field_unknown`, `auth_oauth_adapter_unbound`; LSP hovers for 11 keywords + closed-catalog completions for `algorithm`/`oauth`/`mfa`/`refresh`. Depends on row 26. See `docs/proposals/bucket-auth-cycle.md` §Doctor/LSP. |
| 28 | Auth bucket cycle — golden evals + Go expiry test | planned | `login_password`, `mfa_totp`, `oauth_google` JSONL golden evals + `runtime/go/lazuli/auth/auth_test.go` synctest expiry. The runtime team owns the runtime `auth` package (`password.go`, `session.go`, `oauth.go`, `mfa.go`). Depends on row 26. See `docs/proposals/bucket-auth-cycle.md` §Evals/Testes/§Runtime. |
```
