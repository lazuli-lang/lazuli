# Wire-Thin Audit: `runtime/go/lazuli/auth/`

| | |
|---|---|
| **Date** | 2026-05-13 |
| **Auditor** | TechLead (LAZ-2) |
| **Source commit** | `3988e42` — `docs: L1 proposal for auth session tenant-column codegen shim` |
| **Branch audited** | `origin/main` |

## Method

For each non-test `.go` file:

1. **Effective LOC** — lines excluding blanks and comment-only lines (`grep -v '^\s*$' | grep -v '^\s*//'`).
2. **External imports** — import paths matching `github.com/*`, `golang.org/x/*`, `gopkg.in/*`, `cloud.google.com/*`.
3. **CLAUDE.md test** — `> 100 effective LOC AND zero external imports` when a well-known library exists for the concern → `rewrite-as-wire`.

## Audit Table

| File | Eff. LOC | Ext. imports | Verdict | Target library | Est. cell size |
|---|---|---|---|---|---|
| `audit.go` | 114 | 1 (`jackc/pgx/v5`) | `wire` | — | — |
| `email_verification.go` | 166 | 2 (`jackc/pgx/v5`, `pgconn`) | `wire` | — | — |
| `jwt.go` | 170 | **0** | **`rewrite-as-wire`** | `github.com/golang-jwt/jwt/v5` | ~120 LOC |
| `mfa.go` | 66 | 1 (`pquerna/otp/totp`) | `wire` | — | — |
| `oauth.go` | 108 | 2 (`golang.org/x/oauth2`, `oauth2/google`) | `wire` | — | — |
| `oauth_google.go` | 55 | 1 (`golang.org/x/oauth2`) | `wire` | — | — |
| `password.go` | 111 | 2 (`golang.org/x/crypto/argon2`, `bcrypt`) | `wire` | — | — |
| `password_reset.go` | 145 | 2 (`jackc/pgx/v5`, `pgconn`) | `wire` | — | — |
| `session.go` | 193 | 2 (`jackc/pgx/v5`, `pgconn`) | `questionable` | see note | — |

### Notes

**`jwt.go` (rewrite-as-wire):** 170 effective LOC that hand-rolls HS256 JWT sign/verify using only stdlib (`crypto/hmac`, `crypto/sha256`, `encoding/base64`, `encoding/json`). The implementation is functionally correct (HS256 only, exp validation, custom claims merge) but home-grown. `github.com/golang-jwt/jwt/v5` covers HS256/RS256/ES256, handles `nbf`, `iss`, `aud` validation, and has broad ecosystem adoption. Replacing jwt.go with a wire eliminates ~130 LOC of crypto hand-rolling and closes the door to subtle variations in constant-time comparison and claim handling. This is the canonical LAZ-4 target.

**`session.go` (questionable):** 193 effective LOC, 2 external imports (pgx). The DB wiring (`IssueSession`, `ResolveSession`, `InvalidateSession`) is clean and correct. However, the file carries a 70-LOC custom TTL duration parser (`parseSessionDuration` / `parseSessionNumberUnit` / `parseSessionNumberAndUnit`) that handles strings like `"7 days"`, `"2 weeks"`, `"1h"`. There is no well-known canonical Go library for this exact pattern (go-humanize parses *output*, not input), so the CLAUDE.md `rewrite-as-wire` test does not fire. The concern is that this parser is untested edge-case surface that will be duplicated if other buckets need DSL-string durations. Filed as a `questionable` verdict: not a blocker for the Hostpoint port but should be extracted into a shared `lazuli.ParseDuration` helper before a second bucket needs it.

## Punch List — Follow-up Codex Cells

### Cell LAZ-4 (respin): Rewire `jwt.go` via `golang-jwt/jwt/v5`

**File:** `runtime/go/lazuli/auth/jwt.go`
**Target library:** `github.com/golang-jwt/jwt/v5`

**Spec for Codex worker:**

Rewrite `runtime/go/lazuli/auth/jwt.go` to be a thin wrapper over `github.com/golang-jwt/jwt/v5`. Preserve the existing public API exactly:

- `type Claims struct` — keep field names and json tags; map to `jwt.RegisteredClaims` + `jwt.MapClaims` for Custom.
- `SignJWT(secret []byte, claims Claims) (string, error)` — use `jwt.NewWithClaims(jwt.SigningMethodHS256, ...)`.
- `VerifyJWT(secret []byte, token string) (Claims, error)` — use `jwt.ParseWithClaims`; translate `jwt.ErrTokenExpired` → `ErrJWTExpired`, signature errors → `ErrJWTSignature`, all other parse errors → `ErrJWTInvalid`.
- Keep `ErrJWTInvalid`, `ErrJWTExpired`, `ErrJWTSignature` sentinel errors unchanged.
- Remove all hand-rolled HMAC/base64/json logic — that becomes the library's responsibility.
- Add `github.com/golang-jwt/jwt/v5` to `runtime/go/lazuli/go.mod` if not already present.
- Ensure `go test ./lazuli/auth/...` passes unchanged (the existing test suite covers the public API).

Single file output. Do not touch `jwt_test.go`. Target ~60–80 LOC (from 170).

### Cell (future): Extract `lazuli.ParseDuration` helper

**File:** `runtime/go/lazuli/session.go` (duration parser only — not a full rewrite)
**Priority:** low — not blocking Hostpoint port

**Spec for Codex worker:**

Extract `parseSessionDuration`, `parseSessionNumberUnit`, `parseSessionNumberAndUnit` from `session.go` into a shared function `lazuli.ParseDuration(s string) (time.Duration, bool)` in `runtime/go/lazuli/duration.go`. Wire `session.go`'s `sessionTTL` to call the shared helper. Add a test file `runtime/go/lazuli/duration_test.go` covering the common cases (`"7 days"`, `"2w"`, `"24h"`, `"1hour"`, `""`). No change to `session.go`'s public API.

Two files output (`duration.go`, `duration_test.go`) + edit to `session.go`. Blocked until a second bucket needs the parser (defer until then).

## Summary

**Wire-clean: 8 of 9 files (88.9%).** The `auth/` bucket is in good shape. Seven files (`audit.go`, `email_verification.go`, `mfa.go`, `oauth.go`, `oauth_google.go`, `password.go`, `password_reset.go`) are clean wires over pgx, golang.org/x/crypto, golang.org/x/oauth2, and pquerna/otp. One file is `questionable` (`session.go`) due to an inline duration parser that is not yet duplicated and does not trigger the CLAUDE.md rewrite-as-wire test. One file is `rewrite-as-wire` (`jwt.go`).

**Top 3 risks for the Hostpoint Phase 1 Auth port:**

1. **`jwt.go` hand-rolls HS256.** The Hostpoint port will consume `SignJWT`/`VerifyJWT`. If it also needs RS256 (service-to-service tokens are common in multi-tenant products), the current implementation cannot be extended — it must be rewritten first. The LAZ-4 rewire **should land before the Hostpoint port begins**, not after. Risk: medium-high.

2. **`session.go` TTL parser.** If Hostpoint uses DSL-string TTLs (`"30 days"`) rather than `time.Duration` constants, the custom parser becomes a shared dependency. Any TTL string format Hostpoint needs that `parseSessionDuration` doesn't handle will silently fall back to 24h. Risk: low until Hostpoint DSL is audited, then medium.

3. **`email_verification.go` and `password_reset.go` inline SQL with `fmt.Sprintf` quoting.** Both use a hand-rolled `quoteEmailVerificationIdent` / `quotePasswordResetIdent` that panics on non-alphanumeric identifiers. This is safe for compiler-generated resource names, but if Hostpoint introduces a resource name with a dot or dash (common in multi-tenant schemas), the runtime will panic rather than return an error. Risk: low given the compiler enforces identifier names, but worth documenting in the port checklist.
