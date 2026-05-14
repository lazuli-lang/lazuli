# Hostpoint Phase 1 Auth Port — Gap Analysis

- **Date:** 2026-05-14
- **Auditor:** TechLead
- **Fixture:** `examples/hostpoint-shaped-auth/`
- **Reference:** `docs/audit/auth-wire-readiness-2026-05-13.md`

---

## What generated cleanly

| Surface | DSL construct | Verdict |
|---|---|---|
| Email identity | `auth identity User.email` | ✅ Clean |
| Password auth | `auth password { algorithm argon2id }` | ✅ Clean |
| Password rate limiting | `rate_limit "5 per 10 minutes per ip"` on commands | ✅ Clean |
| TOTP MFA | `auth mfa totp { enroll @fn.enroll_totp; verify @validator.verify_totp }` | ✅ Clean |
| Session resource | `auth sessions { resource UserSession; ttl "7 days" }` | ✅ Clean |
| Tenant scoping | `defaults { tenancy org }` + `Org` FK on `User` and `UserSession` | ✅ Clean |
| Protected command | `command me { returns User; policy @policy.authenticated }` | ✅ Clean |
| Protected API | `api me_http { method GET; path "/me"; policy @policy.authenticated }` | ✅ Clean |
| Tenant-filtered list | `query.list mine { filters { org_id = ctx.actor.org_id } }` | ✅ Clean |
| Audit trail | `audit default` on write commands | ✅ Clean |

All surfaces above parse and pass `lazuli doctor` without error.

---

## What required workaround

### W1 — Feature placed in `features/auth/` but named `account`

The task spec requested `features/auth/auth.lzi`, but the doctor resolves `uses account`
(from `app.lzi`) to a feature declaration named `account`. The file was placed in
`features/auth/auth.lzi` with `feature account` inside; this is valid DSL but deviates
from the directory=feature-name convention used in the scaffold generator.

**Implication for port:** The Hostpoint port should use the standard
`features/account/account.lzi` layout. The `auth` directory name is a fixture-only
convention for clarity.

---

## Hard gaps requiring follow-up

### G1 — Session row does not carry a typed tenant column in codegen output

`defaults { tenancy org }` scopes queries at runtime but the codegen shim in
`crates/lazuli_codegen_go/src/emitter/auth_session.rs` does not yet emit a tenant-column
`WHERE org_id = $tenant_id` clause on `SessionsContract` queries. The DSL expresses the
intent correctly; the lowering to SQL is incomplete.

**Filed as:** [LAZ-5](/LAZ/issues/LAZ-5) (codegen-layer follow-up). Not a blocker for this
fixture since `session.go` runtime is wire-clean (`pgx/v5`).

### G2 — `@key.tenant` envelope not available for `provider_access_token`-style fields

The `user-auth.lzi` pressure-test uses `@cap.Encrypted(key:@key.tenant)` for OAuth
provider tokens. This `@key.tenant` semantic key type has no L0 proposal and no codegen
support. The hostpoint fixture intentionally omits OAuth tokens to avoid using this gap.

**Next step:** A separate L0 proposal for `@key.tenant` envelope is needed before the
Hostpoint OAuth flow can be expressed.

### G3 — `enable_mfa` command pattern requires boilerplate `updates User { mfa_enabled = true }`

There is no `mfa.enable_shorthand` or similar DSL sugar for the common MFA-enable flow.
Users must write the full `command enable_mfa` block with `updates User { mfa_enabled = true }`.
This is acceptable boilerplate for v0; a future vocabulary addition could reduce it.

**Deferred:** No cell filed. Acceptable as-is for the Phase 1 port.

---

## Punch list

| ID | Gap | Severity | Status |
|---|---|---|---|
| G1 | Tenant-column codegen shim missing in `auth_session.rs` | Medium | [LAZ-5](/LAZ/issues/LAZ-5) open |
| G2 | `@key.tenant` encrypted field envelope | Medium | Needs L0 proposal (not filed) |
| G3 | `enable_mfa` boilerplate | Low | Deferred |
| AUTH-1 | `jwt.go` rewrite as wire of `golang-jwt/jwt/v5` | High | From prior audit, unfiled cell |
