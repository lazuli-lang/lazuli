# Auth Tenant Column Gap — Audit Memo

- **Date:** 2026-05-13
- **Auditor:** TechLead
- **Fixtures:** `examples/auth-multi-tenant/`, `examples/marketplace-shaped-auth/`
- **Proposal:** `docs/proposals/auth-session-tenant-pin.md`

This memo tracks the correctness gap surfaced during the [LAZ-3](/LAZ/issues/LAZ-3)
synthetic fixture work and remediated through the S1–S5 cell sequence.

---

## §c1 — Session row missing tenant column in codegen output

| Field | Value |
|---|---|
| **Status** | ✅ closed |
| **Filed** | 2026-05-13 |
| **Closed** | 2026-05-14 (S5, wave `paperclip/techlead-laz-28`) |
| **Proposal** | `docs/proposals/auth-session-tenant-pin.md` |

### Problem

The runtime helpers `IssueSession` / `ResolveSession` in
`runtime/go/lazuli/auth/session.go` ignored the `org_id` column on
multi-tenant session resources. A DSL declaration of:

```lzi
feature account
  defaults
    tenancy org
  domain
    resource UserSession
      org: Org required
      ...
  auth sessions
    resource UserSession
    ttl "7 days"
```

produced a schema with `org_id NOT NULL` but the generated INSERT
omitted `org_id`, causing every production write to fail at the
database level.

### Resolution

Cells S1–S5 of `docs/proposals/auth-session-tenant-pin.md`:

- **S1** — Additive runtime exports (`MintSessionToken`, `HashSessionToken`,
  `SessionDB`, `MapSessionResolveError`) to enable per-resource codegen shims
  without touching the generic `IssueSession` path.
- **S2** — IR `extra_columns` field on `AuthSessions` carries the tenant axis
  from `defaults.tenancy` through to the codegen emitter.
- **S3** — Codegen emitter `crates/lazuli_codegen_go/src/emitter/auth_session.rs`
  emits per-feature `session.gen.go` when `extra_columns` is non-empty; single-tenant
  resources produce no new file (back-compat preserved).
- **S4** — Doctor rules `AUTH-SESSION-CALLSITE-001`, `AUTH-SESSION-TENANT-001`,
  `AUTH-SESSION-EXTRA-001` enforce correct usage at fixture level.
- **S5** *(this cell)* — Fixtures re-codegen'd; `session.gen.go` emitted for
  `examples/auth-multi-tenant/features/account/` and
  `examples/marketplace-shaped-auth/features/account/`.

### Generated artifacts

- `examples/auth-multi-tenant/features/account/session.gen.go`
  — `IssueUserSession`, `ResolveUserSession`, `InvalidateUserSession`
- `examples/marketplace-shaped-auth/features/account/session.gen.go`
  — same shape; sanitized analogue for the downstream-port pattern

### Verification

`go test ./lazuli/auth/...` and `cargo check --all-targets` green
on branch `paperclip/techlead-laz-28`.
Single-tenant fixture `examples/auth-roundtrip/` produces no
`session.gen.go` (back-compat preserved, confirmed by doctor rule
`AUTH-SESSION-CALLSITE-001` absence of false-positives).

---

## Out-of-scope gaps (not tracked here)

- `@key.tenant` envelope for OAuth provider tokens — no L0 proposal;
  separate follow-up if a downstream product requires it.
- Refresh-token tenant pin — deferred to a future cell
  (see proposal §"Out of scope").
