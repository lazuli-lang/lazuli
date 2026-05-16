# multi-tenant-auth

Synthetic fixture for a multi-tenant authentication flow.

Exercises: email+password login, tenant-scoped sessions (via `defaults { tenancy org }`),
TOTP MFA enrollment + verification, and a protected `command me` + `api me_http` that
implicitly reads the current authenticated user and tenant from `ctx.actor`.

All surfaces are expressed using existing Lazuli DSL vocabulary — no new `@-namespace`
or `kind` keywords introduced.
