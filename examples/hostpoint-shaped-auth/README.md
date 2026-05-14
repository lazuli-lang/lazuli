# hostpoint-shaped-auth

Synthetic fixture mirroring the Hostpoint Phase 1 Auth port shape.

Exercises: email+password login, tenant-scoped sessions (via `defaults { tenancy org }`),
TOTP MFA enrollment + verification, and a protected `command me` + `api me_http` that
implicitly reads the current authenticated user and tenant from `ctx.actor`.

Does NOT reference the Hostpoint repo. All surfaces are expressed using existing Lazuli
DSL vocabulary — no new `@-namespace` or `kind` keywords introduced.

See `docs/audit/hostpoint-port-gap-2026-05-14.md` for the gap analysis.
