# Auth Session — Tenant Column Pin (L1)

**Status**: L1 design proposal. The L0 vocabulary
(`resource UserSession { org: Org required, ... }`,
`auth sessions resource UserSession`) is already shipped and used by
`examples/auth-multi-tenant/` + `examples/hostpoint-shaped-auth/`.
This proposal addresses a **runtime/codegen gap** under that
vocabulary, not a surface change.

**Audience**: Lazuli Go runtime team, codegen-go owners, downstream
product authors who declare multi-tenant session resources.

**Date**: 2026-05-13.

**Pilot bucket**: cross-cutting hardening cell — sits inside the
auth bucket but touches codegen-go. **Not a new bucket**; widens an
existing one.

**Companion**:
- `docs/audit/hostpoint-port-gap-2026-05-13.md` §c1 (TechLead audit
  that surfaced the gap during the [LAZ-3](/LAZ/issues/LAZ-3)
  synthetic fixture).
- `docs/proposals/bucket-auth-cycle.md` (the L0 of the auth
  bucket).
- `docs/proposals/auth-lowering-scope.md` (existing
  scope-lowering — this proposal extends the same lowering shape).
- Paperclip issue [LAZ-5](/LAZ/issues/LAZ-5).

**First consumer**: Pleiades v2 (Phase C of the strategic pivot
2026-05-13) will declare multi-tenant session resources;
landing this fix before C.1 build keeps the fixture path clean.
The Hostpoint Phase 1 Auth port (Phase D, downstream) also
consumes this.

---

## Problem

The DSL accepts a multi-tenant session declaration:

```lzi
feature account
  defaults
    tenancy org

  uses org

  domain
    resource UserSession
      org: Org required
      user: User required
      token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth sessions
    resource UserSession
    ttl "7 days"
```

Schema migration emits the right table:

```sql
CREATE TABLE user_session (
    id        bigserial primary key,
    org_id    bigint not null references org(id),
    user_id   bigint not null references "user"(id),
    token_hash text not null unique,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);
```

But the runtime helpers in `runtime/go/lazuli/auth/session.go`
ignore the `org` column entirely. `IssueSession`
([session.go:67-85](runtime/go/lazuli/auth/session.go#L67-L85))
writes only `(user_id, token_hash, expires_at)` — the INSERT
**omits the `org_id` column**, which is `NOT NULL`, so any real
multi-tenant production write **errors at the database**.

`ResolveSession` ([session.go:87-114](runtime/go/lazuli/auth/session.go#L87-L114))
SELECTs only `user_id, expires_at` and **returns an empty
`SessionAttrs{}`** despite the doc comment claiming it populates
`ctx.Tenant`. Even if the row existed, the runtime would not lift
the tenant column into the context the way `query.list mine` /
`policy.@scope.org_member` expect.

The audit memo at `docs/audit/hostpoint-port-gap-2026-05-13.md` §c1
named this as the single largest correctness blocker for the
Phase D Hostpoint port and the Phase C Pleiades v2 port.

## Why this is a codegen problem, not a runtime problem

Five paths considered:

1. **Grow `IssueSession`'s signature** to take a `tenantID lazuli.ID`
   parameter, conditionally insert based on contract metadata.
   Rejected — bloats the runtime helper, breaks every existing
   single-tenant consumer that already calls the v0 signature.
2. **Add a `Tenant *TenantSlot` field to `SessionsContract`** and let
   the runtime build SQL dynamically based on it. Rejected —
   `SessionsContract` is a value struct in the runtime; the runtime
   would have to know how to spell every possible extra column type
   the DSL might add tomorrow (`Region`, `Channel`, `Brand`, ...).
   This is the "runtime grows with every customer" anti-pattern
   that motivated Aerocoding's deprecation
   (see `docs/architecture.md` §"Founding principle").
3. **Make `SessionAttrs` a write-side input as well as a read-side
   output**, threading arbitrary `{column → value}` through. Rejected —
   wide `map[string]any` boundary erases type safety and lets bad
   DSL slip past Doctor.
4. **Emit per-resource shims at codegen time** (codegen knows the
   resource's tenancy axis and field set; emits a typed wrapper that
   does the wire SQL right). **Accepted.** This is the same shape
   already used for `query.list mine` codegen (per
   `auth-lowering-scope.md`), the same shape `notifications`
   codegen uses for typed envelopes, and the same shape
   `bucket-jobs-cycle.md` uses for typed payload wrappers.
5. **Emit per-resource SQL strings + a generic runtime executor**
   that takes parameterized `[]any` and the SQL. Hybrid of (1) and
   (4). Strictly worse than (4): same emit cost, less type safety.

Path (4) is the founding-principle answer. Wire-thin discipline
holds — the runtime stays small, codegen carries the per-resource
specialization.

## Surface design — none

**No DSL change.** The existing `feature.domain.resource <X>` +
`auth sessions resource <X>` surface is sufficient; this proposal
only changes how the codegen-go emitter consumes that IR.

The codegen reads the existing IR shape:
- `FeatureSpec.domain.resources[<X>]` → `Vec<FieldSpec>` carrying
  field name, type ref, required flag, semantic-type binding.
- `FeatureSpec.auth.sessions.resource_name` → the target resource.
- `FeatureSpec.defaults.tenancy_axis` → the tenant resource name
  (`Org` in the example above).

From those, the emitter can compute the **session-relevant columns**
for the resource (everything except `token_hash` + `expires_at` +
`created_at` + the resource's primary key — those are the runtime
helper's existing concern).

## Generated code shape

For the `account` feature in `examples/auth-multi-tenant/`, codegen
emits a per-feature file
`dist/go/<app>/features/account/session.gen.go`:

```go
// Code generated by lazuli; DO NOT EDIT.
// source: features/account/account.lzi:36

package account_gen

import (
    "context"
    "fmt"
    "time"

    "lazuli.dev/runtime/lazuli"
    "lazuli.dev/runtime/lazuli/auth"
)

// IssueUserSession is the tenant-aware wrapper around
// auth.IssueSession. Codegen-emitted; do not call auth.IssueSession
// directly for tenant-scoped session resources — doctor enforces
// AUTH-SESSION-CALLSITE-001.
func IssueUserSession(
    ctx *lazuli.Ctx,
    userID lazuli.ID,
    orgID lazuli.ID,                       // tenant axis from defaults.tenancy
) (string, time.Time, error) {
    token, tokenHash, expiresAt, err := auth.MintSessionToken(ctx, UserSessionContract)
    if err != nil {
        return "", time.Time{}, err
    }

    // Per-resource INSERT writes every NOT NULL column.
    sql := `INSERT INTO "user_session"
              (org_id, user_id, token_hash, expires_at)
            VALUES ($1, $2, $3, $4)`
    if _, err := auth.SessionDB().Exec(ctx.Context(), sql, orgID, userID, tokenHash, expiresAt); err != nil {
        return "", time.Time{}, err
    }
    return token, expiresAt, nil
}

// ResolveUserSession is the tenant-aware wrapper. Populates
// ctx.User and ctx.Tenant from the session row.
func ResolveUserSession(
    ctx *lazuli.Ctx,
    token string,
) (lazuli.ID, lazuli.ID, error) {           // (userID, orgID, error)
    tokenHash, err := auth.HashSessionToken(token)
    if err != nil {
        return 0, 0, err
    }

    sql := `SELECT user_id, org_id, expires_at FROM "user_session"
            WHERE token_hash = $1 LIMIT 1`
    var userID, orgID lazuli.ID
    var expiresAt time.Time
    err = auth.SessionDB().QueryRow(ctx.Context(), sql, tokenHash).Scan(&userID, &orgID, &expiresAt)
    if err != nil {
        return 0, 0, auth.MapSessionResolveError(err)
    }
    if !expiresAt.After(time.Now()) {
        return 0, 0, auth.ErrSessionExpired
    }
    ctx.User = userID
    ctx.Tenant = orgID
    return userID, orgID, nil
}

// InvalidateUserSession remains generic — no tenant axis matters
// on delete-by-token, but we emit a per-resource wrapper for
// vocabulary consistency.
func InvalidateUserSession(ctx *lazuli.Ctx, token string) error {
    return auth.InvalidateSessionByToken(ctx, UserSessionContract, token)
}

// UserSessionContract is the lowered SessionsContract for this
// resource. Retained for adapter-level introspection (tests, doctor).
var UserSessionContract = auth.SessionsContract{
    Resource: "user_session",
    TTL:      "7 days",
    Refresh:  false,
}
```

## Runtime contract changes

The runtime stays mostly generic. Three small refactors split the
existing helpers so codegen can call them piece-wise:

```go
// runtime/go/lazuli/auth/session.go — additive split

// MintSessionToken generates the (token, tokenHash, expiresAt) tuple
// without writing to the database. Codegen calls this first, then
// builds its own per-resource INSERT. Backward-compatible:
// IssueSession continues to call MintSessionToken internally for
// single-tenant resources (no per-resource codegen emitted).
func MintSessionToken(ctx *lazuli.Ctx, contract SessionsContract) (token, tokenHash string, expiresAt time.Time, err error) {
    token, tokenHash, err = newSessionToken()
    if err != nil { return }
    expiresAt = sessionNow(ctx).Add(sessionTTL(contract.TTL))
    return
}

// HashSessionToken exposes the internal hashSessionToken for codegen.
func HashSessionToken(token string) (string, error) { return hashSessionToken(token) }

// MapSessionResolveError centralizes the error mapping codegen uses
// after its own SELECT roundtrip.
func MapSessionResolveError(err error) error {
    if errors.Is(err, pgx.ErrNoRows) { return ErrSessionNotFound }
    return err
}

// SessionDB exposes the configured sessionDB to codegen. Internal
// otherwise — single point of substitution for tests.
func SessionDB() sessionDB { return sessionDBProvider() }

// InvalidateSessionByToken centralizes the delete-by-token path.
// IssueSession + ResolveSession + InvalidateSession remain shipped
// for backward compatibility (single-tenant resources still call
// them directly via codegen that picks the legacy emission path).
func InvalidateSessionByToken(ctx *lazuli.Ctx, contract SessionsContract, token string) error {
    return InvalidateSession(ctx, contract, token)
}
```

**Wire-thin impact**: `session.go` grows from 246 LOC to ~290 LOC,
all additive helper exports — no logic duplication. The
single-tenant `IssueSession`/`ResolveSession` continue to work as-is
(no codegen change for `examples/auth-roundtrip/` and other
single-tenant fixtures).

## Codegen change — where it lives

`crates/lazuli_codegen_go/src/emitter/auth_session.rs` (new file,
single emitter responsibility).

The emitter walks the IR:

1. For each `FeatureSpec` that declares `auth.sessions`:
2. Look up the named `resource` in the feature's domain.
3. Determine the resource's **session-relevant columns** —
   everything declared on the resource that is NOT one of
   `token_hash`, `expires_at`, `created_at`, the primary key
   (`id` / `<resource>_id`), and the `user` field (already
   carried by `auth.identity`).
   In `examples/auth-multi-tenant/`, the surviving column is
   `org_id` (mapped from `org: Org required`).
4. Emit `Issue<Resource>` / `Resolve<Resource>` / `Invalidate<Resource>`
   into `dist/go/<app>/features/<feature>/session.gen.go`.
5. If no extra columns survive step 3 (pure single-tenant
   resource), emit nothing — the existing `auth.IssueSession`
   call from the policy emitter continues to work.

The emitter must respect the existing semantic-type vocabulary:
- `org: Org required` → `lazuli.ID` (Org references resolve to
  bigserial primary keys by the existing IR convention).
- `device: Device required` (a future axis) → same shape.
- `region: Region required` → same shape.

Any field with a non-ID-shaped type (e.g. `country: @semantic.Country
required`) **blocks emission** with `AUTH-SESSION-TENANT-001` and
escalates to a TechLead decision — the v1 emission supports only
ID-shaped extra columns. This is the closed-grammar invariant
applied at the lowering boundary.

## Doctor codes

`crates/lazuli_cli/src/doctor.rs`:

- `AUTH-SESSION-CALLSITE-001` *(error)* — when a handler calls
  `auth.IssueSession(...)` for a session resource that has
  extra columns beyond the v0 contract, doctor rejects with the
  handler location and recommends calling the per-resource
  `Issue<Resource>` wrapper.
- `AUTH-SESSION-TENANT-001` *(error)* — the codegen-side check
  above (extra column with non-ID-shaped type).
- `AUTH-SESSION-EXTRA-001` *(warning)* — when a resource declares
  more than one extra column (e.g. `org: Org required` AND
  `device: Device required`), warn that the v1 emitter handles all
  of them positionally but the caller surface gains parameters in
  IR declaration order. Acceptance: callers double-check param
  order matches the DSL.

## Cells

### S1 — Runtime split: additive helper exports

**File**: `runtime/go/lazuli/auth/session.go` — additive only. Add
`MintSessionToken`, `HashSessionToken`, `MapSessionResolveError`,
`SessionDB()`, `InvalidateSessionByToken`. Keep
`IssueSession` / `ResolveSession` / `InvalidateSession` byte-for-byte
unchanged.

**Tests**: `session_test.go` gets new cases asserting
`errors.Is(MapSessionResolveError(pgx.ErrNoRows), ErrSessionNotFound)`
and that `MintSessionToken` returns a token whose hash matches
`HashSessionToken(token)`. Existing tests stay green.

**Wire-thin gate**: total file ≤ 320 effective LOC (current is
~246; new additions ~40-50).

**Commit message**: `auth: additive helper exports for codegen-side session shims`.

### S2 — IR threading: tenancy axis on auth.sessions

**File**: `crates/lazuli_ir/src/lib.rs`.

**Spec**: extend the lowered `AuthSessions` IR struct with the
resolved session-relevant column list:

```rust
pub struct AuthSessions {
    pub resource: String,
    pub ttl: String,
    pub refresh: bool,
    pub extra_columns: Vec<SessionExtraColumn>,    // NEW; empty for single-tenant
}

pub struct SessionExtraColumn {
    pub field_name: String,           // "org"
    pub column_name: String,          // "org_id"
    pub go_type: String,              // "lazuli.ID"
    pub references: Option<String>,   // "Org" — the referenced resource
    pub required: bool,
}
```

Lowering pass populates `extra_columns` from the resource's
`Vec<FieldSpec>` minus the v0 baseline (token_hash / expires_at /
created_at / id / user). JSON serde + roundtrip tests.

**Commit message**: `ir: extra_columns on AuthSessions for codegen tenant shim`.

### S3 — Codegen emitter

**File**: `crates/lazuli_codegen_go/src/emitter/auth_session.rs`
(new) + integration into the existing feature emitter.

**Spec**: §"Codegen change" above. Snapshot tests against
`examples/auth-multi-tenant/` and `examples/auth-roundtrip/`:

- Multi-tenant fixture emits `session.gen.go` with
  `IssueUserSession(ctx, userID, orgID)` and
  `ResolveUserSession(ctx, token) (userID, orgID, error)`.
- Single-tenant fixture emits **no** `session.gen.go` (existing
  policy-emitter path continues).

**Generated code acceptance**: `< 80 effective LOC` per
`session.gen.go`. One Go import per generated file
(`lazuli.dev/runtime/lazuli/auth`).

**Commit message**: `codegen: per-feature session shim with tenant column`.

### S4 — Doctor: `AUTH-SESSION-*` codes

**File**: `crates/lazuli_cli/src/doctor.rs`.

**Codes**: three from §"Doctor codes" above
(`-CALLSITE-001`, `-TENANT-001`, `-EXTRA-001`).

**Tests**: snapshot tests via the existing doctor harness against
fixtures with deliberate violations.

**Commit message**: `doctor: AUTH-SESSION-* codes for tenant shim`.

### S5 — Fixture exercise + memo

**Files**:
- Re-run `examples/auth-multi-tenant/` codegen; verify the
  generated `session.gen.go` lives where expected and compiles.
- Re-run `examples/hostpoint-shaped-auth/` (the LAZ-3 fixture) and
  verify the runtime path is now consistent.
- Update `docs/audit/hostpoint-port-gap-2026-05-13.md` §c1 to mark
  the gap closed, with the SHA of the merge commit.

**Acceptance**:
- `lazuli generate examples/auth-multi-tenant/` is green.
- `go test ./dist/go/auth-multi-tenant/features/account/...` is green.
- `cargo check --all-targets` green.
- `go test ./lazuli/auth/...` green (existing tests unchanged).
- Doctor catches a synthetic `country: @semantic.Country required`
  injection with `AUTH-SESSION-TENANT-001`.

**Commit message**: `examples: auth-multi-tenant re-codegen + hostpoint-port-gap memo update`.

## Acceptance (cycle-level)

- All five cells (S1-S5) land green.
- `examples/auth-multi-tenant/` produces a `session.gen.go` whose
  diff against the L1 proposal sample is ≤ formatting drift.
- `examples/auth-roundtrip/` produces NO new generated file
  (back-compat preserved).
- The audit memo §c1 status flips `unresolved` → `resolved` with
  the merge SHA.
- Runtime `runtime/go/lazuli/auth/session.go` total effective LOC
  ≤ 320 (current is ~246).

## Risks

| Risk | Mitigation |
|---|---|
| Existing single-tenant consumers (`examples/auth-roundtrip/`) break because codegen now scans every `auth.sessions` block | Cell S3 includes a snapshot test asserting NO `session.gen.go` emission for single-tenant fixtures. CI catches accidental emission |
| Generated `IssueUserSession` parameter order silently flips when DSL field order changes | Doctor `AUTH-SESSION-EXTRA-001` warns when > 1 extra column exists; emitter writes a comment to the generated file naming the IR declaration order. Future polish: take a struct param instead of positional args. |
| Hostpoint port (Phase D) declares a session resource with > 2 axes (e.g. `org` + `region` + `device`) | Cell S3's positional emission scales linearly. Polish (struct param) becomes blocking only if Hostpoint declares 3+ axes; cite as a re-spin trigger, not v1 scope |
| Codegen-emitted file imports something outside `lazuli.dev/runtime/lazuli/auth` + `lazuli.dev/runtime/lazuli` | Cell S3 acceptance check on imports. Architect static review catches |
| The audit memo §c1 referenced by the runtime cell becomes stale once §c1 closes | Cell S5 updates the memo in the same wave |

## Out of scope (deferred)

- **Struct-param ergonomics** for `Issue<Resource>` — keep positional
  for v1; revisit if a feature declares > 2 extra axes.
- **Refresh-token tenant pin** — `SessionsContract.Refresh` flows
  through `MintSessionToken` for v1; the refresh-token rotation
  path lives in a separate cell that doesn't ship this wave.
- **Cross-feature session sharing** — one resource per feature;
  cross-feature session resources are not a supported pattern.
- **`expose client`-side error mapping** changes — the existing
  401/400 mapping stays (`auth.session_expired` /
  `auth.session_unknown` / `auth.token_invalid`).
- **Tenant resource hierarchies** (e.g. `org` contains `team`) —
  v1 emits the leaf tenant axis from `defaults.tenancy`; hierarchies
  layer on top via policy filters, not session columns.

## Companion docs to update

After cells land:
- `docs/architecture.md` — note the per-feature session shim
  pattern in the auth bucket inventory.
- `docs/invariants.md` — close-grammar note for `AUTH-SESSION-*`
  doctor codes.
- `docs/audit/hostpoint-port-gap-2026-05-13.md` — §c1 marked
  resolved.

## Grade-then-fix gate

This proposal must reach **≥ 8.5/10 with no dimension below 7**
via `lazuli-language-architect`. Hard blockers:

- **Boundary leak**: any cell touching files outside the auth +
  codegen-go boundaries declared here.
- **Wire violation**: runtime split (S1) growing `session.go` past
  ~320 effective LOC, or duplicating logic that the existing
  `IssueSession` already does.
- **Vocabulary drift**: introducing a new `.lzi` `kind`,
  `@-namespace`, or DSL surface — this proposal is explicitly
  surface-free.
- **Back-compat break**: any change to the v0 single-tenant
  `IssueSession` / `ResolveSession` / `InvalidateSession`
  signatures or behavior.

If any blocker survives v1, the proposal blocks at design time and
cells S1-S5 do not launch.
