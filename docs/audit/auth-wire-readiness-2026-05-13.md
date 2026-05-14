# Auth Bucket Wire-Thin Audit

- **Date:** 2026-05-13 (committed 2026-05-14)
- **Auditor:** TechLead
- **Source commit:** `3988e42` (main, pre-audit snapshot)
- **Bucket:** `runtime/go/lazuli/auth/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `audit.go` | 114 | `pgx/v5` | **wire** | — | — |
| `email_verification.go` | 166 | `pgx/v5`, `pgx/v5/pgconn` | **wire** | — | — |
| `jwt.go` | 170 | _none_ | **rewrite-as-wire** | `github.com/golang-jwt/jwt/v5` | S (≤ 60 LOC) |
| `mfa.go` | 66 | `pquerna/otp/totp` | **wire** | — | — |
| `oauth.go` | 108 | `golang.org/x/oauth2`, `golang.org/x/oauth2/google` | **wire** | — | — |
| `oauth_google.go` | 55 | `golang.org/x/oauth2` | **wire** | — | — |
| `password.go` | 111 | `golang.org/x/crypto/argon2`, `.../bcrypt` | **wire** | — | — |
| `password_reset.go` | 145 | `pgx/v5`, `pgx/v5/pgconn` | **wire** | — | — |
| `session.go` | 193 | `pgx/v5`, `pgx/v5/pgconn` | **questionable** | see note | — |

### session.go note

`session.go` correctly wires `pgx/v5` for all DB operations. The concern is `parseSessionDuration` (~55 eff. LOC across three helpers), which hand-parses DSL strings like `"7 days"` or `"2w"` into `time.Duration`. The `TTL any` field on `SessionsContract` must accept both `time.Duration` (generated code) and raw DSL strings (test/adapter paths), so no existing stdlib parser covers the full input space. This is intentional framework vocabulary bridge code — not a reimplementation of a known library — and the LOC is concentrated in a single switch table. Verdict: **questionable but acceptable** for v0; a future cell could extract it to a shared `lazuli.ParseDuration` util if reused elsewhere.

---

## Summary

**8/9 files (88.9%) are wire-clean.** One clear violation (`jwt.go`), one acceptable questionable case (`session.go`).

### Top 3 risks for the Hostpoint Phase 1 Auth port

1. **`jwt.go` custom implementation** — The inline HS256 signer/verifier is not battle-tested for edge cases (algorithm confusion, clock skew handling, future RS256 needs). The Hostpoint port likely requires asymmetric keys (RS256/ES256) for third-party token validation. Rewriting to `golang-jwt/jwt/v5` before the port avoids forking the JWT surface twice.

2. **`session.go` TTL parser coupling** — The `TTL any` field and `parseSessionDuration` are tightly coupled to the DSL string representation. If the Hostpoint auth codegen emits a different TTL format, this could silently fall back to 24h. Recommend adding a `lazuli.Duration` strong type and emitting it uniformly from codegen.

3. **`email_verification.go` inline SQL** — 166 LOC, all DB queries are hand-rolled `fmt.Sprintf` strings (no pgx batch, no named params). This is not a wire-thin violation (pgx is wired), but the SQL surface is large enough to diverge if the schema changes. A codegen shim that lowers `auth email-verification` to typed queries would reduce drift risk.

---

## Punch List (Codex cells)

### Cell AUTH-1: Rewrite jwt.go as wire of golang-jwt/jwt/v5

**Trigger:** `jwt.go` is 170 eff. LOC with zero external imports — hard violation of the wire-thin rule.

**Spec for Codex:**

```
File to replace: runtime/go/lazuli/auth/jwt.go
Target library:  github.com/golang-jwt/jwt/v5

Keep the public API identical:
  type Claims struct { Subject, Issuer, Audience string; ExpiresAt, NotBefore, IssuedAt int64; Custom map[string]any }
  func SignJWT(secret []byte, claims Claims) (string, error)
  func VerifyJWT(secret []byte, token string) (Claims, error)
  var ErrJWTInvalid, ErrJWTExpired, ErrJWTSignature error

Implementation notes:
- Use jwt.NewWithClaims(jwt.SigningMethodHS256, ...) for signing.
- Use jwt.ParseWithClaims(..., jwt.WithValidMethods([]string{"HS256"})) for verify.
- Map jwt.ErrTokenExpired → ErrJWTExpired, jwt.ErrSignatureInvalid → ErrJWTSignature, everything else → ErrJWTInvalid.
- Custom claims: implement jwt.Claims interface; merge Custom map on sign, extract on parse.
- Do NOT touch jwt_test.go — existing tests must pass unchanged.
- Do NOT touch any other file.
- Estimated output: ≤ 60 eff. LOC.

Commit message: "runtime/auth: rewrite jwt.go as wire of golang-jwt/jwt/v5 (AUTH-1)"
```

**Estimated size:** S (≤ 60 LOC post-rewrite).

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 9 |
| Wire-clean | 8 (88.9%) |
| Rewrite-as-wire | 1 (`jwt.go`) |
| Questionable | 1 (`session.go`) |
| Delete-candidate | 0 |
| Codex cells generated | 1 (AUTH-1) |
| Hostpoint-blocker risk | Medium (`jwt.go` must be rewritten before RS256 is needed) |
