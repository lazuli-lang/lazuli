---
id: 0026
title: runtime-surface-index — a generated, intent-keyed runtime symbol index injected into the agent context pack
type: techspec
track: tell/ship (reinvention defense)
depends_on: []
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p xtask runtime_surface && cargo test --workspace"
agent: unassigned
---

# TechSpec — Runtime-surface index (mechanism ii)

This is **mechanism (ii)** of the three-part reinvention defense from
`/c/tmp/reinvention-damage-report.json` §5. The other two are mechanism (i)
smart runtime-delegating stubs and mechanism (iii) the
`VOCAB-RUNTIME-REINVENTED-001` doctor rule (spec 0024). This spec owns the
context-pack half: **the runtime surface was not in the building agent's context
window, so it reached for `golang.org/x/crypto/argon2` instead of
`auth.HashPassword`.** The smoking gun: hostpoint's own `binding_fns.go` already
wired `auth.HashPassword`, yet a hand-rolled `hash_password.go` sits beside it —
the agent could not see its own runtime surface.

`docs/quickref.md` is, in its own words, "the context pack to load first when an
agent or a human needs to author, review, or patch canonical `.lzi`/`.lzx`
files." It teaches the **language** surface (keywords, namespaces, primitives)
exhaustively — but it says **nothing** about the runtime Go surface the handlers
actually call. The diagnosis: an agent that has read quickref still does not know
`auth.HashPassword` exists. We close that hole.

## Approach

Mirror the `gen-keyword-reference` discipline verbatim. `keyword-reference.md`
is a **generated, never-hand-edited** projection of the `lazuli_keywords::ALL`
registry, gated fresh by `tools/xtask/tests/keyword_reference_fresh.rs`. We add a
sibling generator that projects the **runtime Go surface**
(`runtime/go/lazuli/**/*.go`, package path `lazuli.dev/runtime/lazuli`) to a
generated `docs/lazuli_way/runtime-surface.md`, gated fresh the same way. It is
GENERATED, not hand-vendored, so it can never drift as the runtime grows — the
exact property that lets `runtime-surface.md` be trusted as the agent's
authoritative "what does the runtime already own" index.

The generator is **NOT a flat alphabetical symbol dump.** The damage report's
finding is precise: the agent knew it *needed* Argon2id, it just didn't know the
runtime *exported* a verb for it. So the index is **INTENT-KEYED** — grouped by
capability family with a one-line *"reach for this when you need to X"* per
family. A flat list of `HashPassword VerifyPassword HashSessionToken …` does not
defeat reinvention; *"hash a password → `auth.HashPassword` / `auth.VerifyPassword`"*
does. The Pareto: the families the audit flagged — **auth (11 of 13 runtime
findings), lifecycle/transition (7 of 18 language findings), semantic scalars, CRUD
effects, roles** — get hand-curated intent annotations; the remaining ~24
packages get a generated symbol list with a lighter one-line package gloss.

Two inputs feed the generator:

1. **The census** (machine-scanned, exhaustive, regenerated every run): every
   exported `func`/`type` per package, scanned from `runtime/go/lazuli/**/*.go`.
   This is the drift-proof skeleton — when the runtime grows a symbol, the next
   regen picks it up and the freshness test fails until committed.
2. **A curated INTENT TABLE** (a `const` in the generator, the deliverable
   shape): `{ family_title, intent_line, package, symbols[] }` rows for the
   Pareto families. The intent table is the teaching layer; the census is the
   exhaustiveness layer. The generator emits a `## Reach for this` intent section
   (driven by the table) **followed by** a `## Full symbol census` section
   (driven by the scan), and asserts every symbol named in the intent table
   actually exists in the census (so a curated row can't name a symbol the
   runtime dropped).

Then we **inject** the index into the context pack at three sites so it is
impossible to not-see: `docs/quickref.md`, `CLAUDE.md.tmpl`, `AGENTS.md.tmpl`.
And we **fill** the keystone teaching doc `docs/lazuli_way/delegate-to-runtime.md`
(the stub spec 0024 creates) that ties all three mechanisms together.

## Surface

**Create:**

- `tools/xtask/src/runtime_surface.rs` — the generator. Owns:
  - `RUNTIME_DIR: &str = "runtime/go/lazuli"` and
    `DOC_REL: &str = "docs/lazuli_way/runtime-surface.md"`.
  - `scan_surface(root) -> BTreeMap<String /*package*/, PackageSurface>` — walks
    `runtime/go/lazuli/**/*.go`, **excluding `*_test.go` and `internal/`**, and
    for each `.go` file extracts exported symbols. A symbol is exported when it
    matches `^func ([A-Z]\w*)` (top-level func; the method-receiver form
    `^func (recv) Name` is **not** a package export and is skipped) or
    `^type ([A-Z]\w*)`. Package name = the leaf directory under
    `runtime/go/lazuli/` (root files → package `lazuli`; the `auth/` subdir →
    `auth`; etc.), matching exactly the package layout
    `/c/tmp/runtime-surface.md` already proves (30 packages). De-dup + sort
    symbols per package for determinism.
  - `const INTENT_TABLE: &[IntentFamily]` — the curated Pareto rows (the
    deliverable; see Contracts).
  - `render(scan, table) -> (String, usize)` — assembles the doc: header banner
    (GENERATED-DO-NOT-EDIT + regen command, byte-for-byte the
    `keyword_reference.rs` HEADER posture), the intent section, the census
    section. Returns `(doc, symbol_count)`.
  - `validate_intent_against_census(table, scan) -> Result<(), String>` — every
    `package.Symbol` named in `INTENT_TABLE` must appear in the scan; a curated
    row naming a vanished symbol fails the generator (and thus the freshness
    test). This is what keeps the *intent* layer honest as the runtime evolves.
  - `run(check: bool) -> Result<(), String>` — IDENTICAL control flow to
    `keyword_reference::run`: build the doc, `normalize()` line endings, compare
    to the committed file; `--check` errors on drift, otherwise writes. Reuse
    `workspace_root()`/`normalize()` shape verbatim.
- `tools/xtask/tests/runtime_surface_fresh.rs` — the freshness gate, mirroring
  `keyword_reference_fresh.rs`:
  ```rust
  #[test]
  fn runtime_surface_is_fresh() {
      if let Err(e) = xtask::runtime_surface::run(true) {
          panic!(
              "docs/lazuli_way/runtime-surface.md is stale — run \
               `cargo run -p xtask -- gen-runtime-surface`.\n{e}"
          );
      }
  }
  ```
- `docs/lazuli_way/runtime-surface.md` — **the generated artifact**, committed
  (so the freshness test has a baseline and agents can read it without running
  cargo). Produced solely by `cargo run -p xtask -- gen-runtime-surface`.

**Modify:**

- `tools/xtask/src/main.rs` — add the `gen-runtime-surface` arm
  (mirror the `gen-keyword-reference` arm; thread `--check`) and add the token to
  `USAGE`.
- `tools/xtask/src/lib.rs` — `pub mod runtime_surface;`.
- `docs/quickref.md` — add a `## Runtime owns these mechanisms — never reimplement`
  section + a pointer to `docs/lazuli_way/runtime-surface.md` (see Contracts for
  exact content). This is the language-context-pack gaining its missing
  runtime half.
- `lazurite/templates/default/CLAUDE.md.tmpl` **AND**
  `lazurite/templates/default/AGENTS.md.tmpl` (kept **byte-identical** — the
  `scaffold_templates_stay_identical` test enforces it) — add one bullet to the
  `## Authoring idioms` block pointing every authoring agent at the
  runtime-surface index BEFORE it writes a handler.
- `docs/lazuli_way/delegate-to-runtime.md` — **fill** the 0024 stub fully (the
  keystone teaching doc; see Contracts).
- `docs/lazuli_way.md` — add the `delegate-to-runtime` index row (the idiom
  table + the `[…](lazuli_way/delegate-to-runtime.md)` link).
- `crates/lazuli_cli/tests/docs_lazuli_way.rs` — add `"delegate-to-runtime"` to
  the `frozen` slug array in `index_links_resolve` (the link set is frozen; this
  spec is the one that lands the row), and add the two new assertion tests (see
  Tests).

## Contracts

### The intent table (the deliverable shape — Pareto families)

```rust
struct IntentFamily {
    title: &'static str,    // "Password hashing"
    reach: &'static str,    // "hash or verify a user password"
    package: &'static str,  // "auth" | "lazuli" | "lifecycle" | …
    symbols: &'static [&'static str],  // ["HashPassword","VerifyPassword",…]
    note: &'static str,     // one-line "the runtime owns X, Y, Z" gloss
}
```

The seed rows (these ARE the priority families the audit flagged; every symbol
below is confirmed present in `/c/tmp/runtime-surface.md`):

| family | reach for this when you need to… | package.symbols | note |
|--------|----------------------------------|-----------------|------|
| **Password hashing** | hash or verify a user password | `auth.HashPassword`, `auth.VerifyPassword`, `auth.HashWithArgon2`, `auth.Argon2Params`, `auth.SetArgon2Concurrency` | argon2id + concurrency cap + tuning are runtime-owned. Prefer dropping `@fn.hash_password` and letting `@cap.Hashed(algorithm:argon2id)` auto-wire. **Never import `golang.org/x/crypto/argon2`.** |
| **Sessions** | mint, hash, issue, rotate, or invalidate a session | `auth.MintSessionToken`, `auth.HashSessionToken`, `auth.IssueSession`, `auth.RotateSession`, `auth.ResolveSession`, `auth.InvalidateSession`, `auth.InvalidateSessionByID` | token + hash stay in lockstep with the runtime session resolver. Don't hand-roll opaque-token mint/hash with `crypto/rand` + `crypto/sha256`. |
| **Password reset / email verification** | issue or consume a reset/verify token | `auth.RequestPasswordReset`, `auth.ConfirmPasswordReset`, `auth.PasswordResetToken`, `auth.PasswordResetContract`, `auth.IssueEmailVerificationToken`, `auth.VerifyEmailToken`, `auth.EmailVerificationToken`, `auth.EmailVerificationContract` | format + hashing + TTL + single-use consume come from the runtime against the declared contract. No manual `SELECT … used_at … expiry` dance. |
| **JWT / OAuth / MFA** | sign/verify a JWT, run an OAuth leg, enroll/verify MFA | `auth.SignJWT`, `auth.VerifyJWT`, `auth.Claims`, `auth.BuildOAuthConfig`, `auth.GoogleOAuthRedirectURL`, `auth.GoogleOAuthCallback`, `auth.EnrollMFA`, `auth.VerifyMFA` | the auth leg is wired; you supply the binding, not the crypto. |
| **Roles** | check the actor's role | `lazuli.HasRole`, `lazuli.RequireRole`, `lazuli.RequireActor` | inline `lazuli.HasRole(ctx,"ADMIN")` — never an app-local `actorHasRole` helper. |
| **Money** | construct or parse a money value | `lazuli.BRL`, `lazuli.USD`, `lazuli.EUR`, `lazuli.MoneyValue`, `lazuli.ParseMoneyLiteral` | currency-aware money is a runtime type; see `docs/lazuli_way/money.md`. |
| **Rate limit** | parse / apply a rate-limit spec | `lazuli.ParseRateLimit`, `lazuli.RateLimit`, `lazuli.RateLimitFromDefault`, `lazuli.RateLimitByEnv`, `lazuli.RateLimitMiddleware` | declare `rate_limit "<spec>"`; the runtime parses + enforces. |
| **Lifecycle / state transitions** | advance a resource's status through a state machine | `lazuli.TransitionAdvance`, `lazuli.Transition`, `lazuli.LifecycleStateMismatchError`, `lifecycle.Machine`, `lifecycle.New`, `lifecycle.Transition` | declare a `lifecycle <field>` + `transition`; the runtime enforces the from-state set and emits the mismatch error. Never hand-roll `UPDATE … status IN (…) … RowsAffected == 0`. See `docs/lazuli_way/state-machines.md`. |
| **CRUD effects** | create / update / delete / reorder a resource | `lazuli.Creates`, `lazuli.CreatesEffect`, `lazuli.CreatesWithOwnerCheck`, `lazuli.Updates`, `lazuli.UpdatesEffect`, `lazuli.Deletes`, `lazuli.DeletesEffect`, `lazuli.Reorder`, `lazuli.ReorderEffect`, `lazuli.NewUpdate`, `lazuli.PartialUpdate`, `lazuli.UpdateBuilder`, `lazuli.OwnedByActor` | declare `creates`/`updates`/`deletes`; the runtime applies defaults, `ctx.now`, actor-owner scoping. No raw `db.Exec` INSERT with literal defaults; no hand-built partial-UPDATE placeholder bookkeeping. See `docs/lazuli_way/crud-by-convention.md`. |
| **Semantic scalars** | validate a hex color / percentage / email at the boundary | `lazuli.HexColor`, `lazuli.Percentage` (+ the closed `@semantic.*` catalog: `@semantic.HexColor`, `@semantic.Percentage`, `@semantic.Email`, `@semantic.Money`) | declare the field as the scalar (`color: @semantic.HexColor`); the type validates at decode. Never a regex `^#?[0-9A-Fa-f]{6}$` validator. |
| **Notifications** | send a notification / digest | `notifications.Send`, `notifications.NewRegistry`, `notifications.NotificationContract`, `notifications.NewSMTPDispatcher` | the channel/digest/throttle machinery is runtime-owned. |
| **Storage / signed URLs** | issue a signed upload/download URL | `storage.IssueSignedURL`, `storage.IssueSignedUploadURL`, `storage.NewS3Store`, `storage.ObjectStore`, `lazuli.ObjectStore` | declare `@cap.File`; the runtime owns presigning. |
| **Encryption** | encrypt/decrypt a field at rest | `encryption.NewCipher`, `encryption.For`, `encryption.ForCtx`, `lazuli.Geocoder` (maps), `encryption.Register` | declare `@cap.Encrypted(key:@key.*)`; the cipher + key rotation are runtime-owned. |

> Symbols beyond these families (the ~24 remaining packages: `billing`, `cache`,
> `events`, `jobs`, `migrations`, `mcp`, `observability`, `payments`, `poller`,
> `report`, `secrets`, `vectorstore`, `waf`, `webhooks`, `breach`, `captcha`,
> `mtls`, `reputation`, `i18n`, `audit`, `probe`, `plangate`, `email`, `maps`)
> render in the **census** section with a one-line package gloss and the sorted
> symbol list — lighter annotation, full exhaustiveness. The intent layer is
> Pareto; the census layer is complete.

### `docs/lazuli_way/runtime-surface.md` — generated doc shape

```
<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: runtime/go/lazuli/**/*.go (the exported Go surface).
     Regenerate with: cargo run -p xtask -- gen-runtime-surface
     Freshness is gated by tools/xtask/tests/runtime_surface_fresh.rs. -->

# Lazuli runtime surface — what the runtime already owns

The runtime owns the MECHANISM; you declare the RESOURCE. Before you write a
handler, scan this index: if the runtime already exports the verb, delegate —
do NOT reimplement. See docs/lazuli_way/delegate-to-runtime.md.

## Reach for this (intent-keyed)
### Password hashing
**Reach for this when you need to:** hash or verify a user password.
`auth.HashPassword` · `auth.VerifyPassword` · `auth.HashWithArgon2` · …
> argon2id + concurrency cap + tuning are runtime-owned. Never import golang.org/x/crypto/argon2.
…(one block per intent family)…

## Full symbol census
_Generated from N exported symbols across M runtime packages._
### auth
> <one-line package gloss>
`Argon2Params` · `AuditEntry` · … (sorted)
…(one block per package)…
```

The header + the entire body are machine-owned and rewritten every run (same
total-ownership posture as `keyword-reference.md`); the `--check` gate is total.

### `docs/quickref.md` — the injected runtime section

Add, near the `## Security Checklist` / `## Do Not Add In v0` region, a new
section:

```markdown
## Runtime owns these mechanisms — never reimplement

Quickref above teaches the **language** surface. The **runtime** surface — the Go
verbs your handlers call — is indexed, intent-keyed, in
**`docs/lazuli_way/runtime-surface.md`** (generated from `runtime/go/lazuli/`,
never hand-edited). Scan it BEFORE writing any `handlers/<x>.go`: if the runtime
already exports the verb, delegate; do not hand-roll it.

The reinvention hot spots (from the pilot audit) and their runtime owners:

| You reach for… | The runtime already owns it |
|----------------|------------------------------|
| password hashing | `auth.HashPassword` / `auth.VerifyPassword` — never `golang.org/x/crypto/argon2` |
| session/reset/verify tokens | `auth.MintSessionToken` / `auth.RequestPasswordReset` / `auth.ConfirmPasswordReset` / `auth.VerifyEmailToken` |
| role checks | `lazuli.HasRole` — never an app-local `actorHasRole` |
| advancing a status | declare a `lifecycle`/`transition`; runtime owns `lazuli.TransitionAdvance` + `LifecycleStateMismatchError` |
| hex color / percentage | `@semantic.HexColor` / `@semantic.Percentage` — never a regex validator |
| create/update/delete | `creates`/`updates`/`deletes` effects (`lazuli.Creates` …) — never raw `db.Exec` |
| money | `lazuli.BRL` / `lazuli.ParseMoneyLiteral` |

This is mechanism (ii) of the reinvention defense; the
`VOCAB-RUNTIME-REINVENTED-001` doctor rule (mechanism iii) is the backstop. See
`docs/lazuli_way/delegate-to-runtime.md`.
```

### Scaffold bullet (byte-identical in both templates)

Add to the `## Authoring idioms` list in `CLAUDE.md.tmpl` **and**
`AGENTS.md.tmpl`:

```markdown
- Before you write any `handlers/<x>.go`, scan the **runtime-surface index** —
  `docs/lazuli_way/runtime-surface.md` (intent-keyed: "hash a password →
  `auth.HashPassword`", "advance a state → declare a `transition`", "hex color →
  `@semantic.HexColor`"). The runtime owns the mechanism; you declare the
  resource. Reimplementing a runtime verb (hand-rolled argon2, a manual
  `UPDATE … status IN (…)` transition, a regex hex validator) trips
  `VOCAB-RUNTIME-REINVENTED-001`. See `docs/lazuli_way/delegate-to-runtime.md`.
```

(Insert at the same relative position in both files so the diff is identical and
`scaffold_templates_stay_identical` stays green.)

### `docs/lazuli_way/delegate-to-runtime.md` — the keystone teaching doc (fill fully)

Follows the fixed lazuli_way doc shape (`## Reach for this` / `## Before … /
After …` / `## Enforced by`). It is the doc that **ties all three mechanisms
together**; the 0024 stub gives it a header + the rule reference, this spec fills
it:

```markdown
# Delegate to the runtime

## Reach for this

When a handler needs a MECHANISM — hash a password, mint a session token,
advance a status, validate a hex color, create/update/delete a row — declare the
resource and let the runtime own the mechanism. **Delegate, don't reimplement.**
The Lazuli runtime (`runtime/go/lazuli/`) already exports the verb; the
intent-keyed index of every exported symbol is
[`runtime-surface.md`](runtime-surface.md). Scan it BEFORE you write
`handlers/<x>.go`. The agent's default is "I am writing Go"; the discipline is
"I am declaring a Lazuli resource and the runtime owns the mechanism."

## Before (hand-rolled) / After (idiomatic)

**Before** — the pauta/hostpoint audit found 13 handlers reimplementing runtime
verbs, 11 of them in `auth`. The canonical case: a hand-rolled
`hash_password.go` importing `golang.org/x/crypto/argon2`, re-deriving the
argon2id encode/PHC-parse from scratch — *beside* a `binding_fns.go` that already
wired `auth.HashPassword`. The agent could not see its own runtime surface:

```go
// hostpoint app/features/account/handlers/hash_password.go (BEFORE)
import "golang.org/x/crypto/argon2"
func HashPassword(plaintext string) (string, error) {
    salt := make([]byte, 16); /* rand */
    h := argon2.IDKey([]byte(plaintext), salt, argonTime, argonMemory, argonThreads, 32)
    return encodeArgon2id(salt, h), nil   // ~60 LOC of hand-rolled PHC encode
}
```

**After** — delete the file; the runtime owns it. `@cap.Hashed(algorithm:argon2id)`
on the password field auto-wires `auth.HashPassword` / `auth.VerifyPassword`
(argon2id + concurrency cap + OWASP tuning included). If a binding is still
needed, it is wire-thin:

```go
// app/features/account/handlers/binding_fns.go (AFTER) — already present, was duplicated
lazuli.RegisterBindingFn("account.hash_password", func(ctx context.Context, pw string) (string, error) {
    return auth.HashPassword(ctx, accountgen.AccountAuthPassword, pw)
})
```

The same shape applies to every flagged family: a manual
`UPDATE … status IN (from-set) … RowsAffected == 0` → declare a `transition`
and call `lazuli.TransitionAdvance`; a `^#?[0-9A-Fa-f]{6}$` regex validator →
declare `@semantic.HexColor`; an `actorHasRole` helper → inline `lazuli.HasRole`.

## The three mechanisms (why this stopped happening)

The audit's root cause was not malice, it was **invisibility** — the runtime
surface was not in the building agent's context window. Three mechanisms close
the class permanently:

1. **Smart stubs (mechanism i).** A delegating stub body (`auth.HashPassword`,
   `TransitionAdvance`, the CRUD-effect call) is generated instead of an empty
   `// IMPLEMENT ME` marker, so the runtime call is the default and hand-rolling
   is a wall the agent must tear down.
2. **The runtime-surface index (mechanism ii — this doc's companion).**
   [`runtime-surface.md`](runtime-surface.md) is generated from
   `runtime/go/lazuli/` and injected into the context pack (`quickref.md` + the
   scaffold `CLAUDE.md`/`AGENTS.md`), so the surface is impossible to not-see.
3. **The doctor rule (mechanism iii).** `VOCAB-RUNTIME-REINVENTED-001` is the
   backstop: it fires at lint time on a handler that imports
   `golang.org/x/crypto/argon2` (or matches a reinvention body-shape) when the
   runtime exports the equivalent.

## Enforced by

- `VOCAB-RUNTIME-REINVENTED-001`
  ([crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs](../../crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs))
  — fires on a handler that reimplements a runtime-owned mechanism; names the
  equivalent symbol and links here. Suppress a genuine upstream-gap case with
  `# doctor:allow VOCAB-RUNTIME-REINVENTED-001 — reason "..."`.
- `runtime_surface_is_fresh`
  ([tools/xtask/tests/runtime_surface_fresh.rs](../../tools/xtask/tests/runtime_surface_fresh.rs))
  — keeps the index regenerable-identical from the live runtime, so it never
  drifts.
- `quickref_and_scaffold_reference_runtime_surface`
  ([crates/lazuli_cli/tests/docs_lazuli_way.rs](../../crates/lazuli_cli/tests/docs_lazuli_way.rs))
  — keeps the index referenced from the context pack so the injection can't be
  silently dropped.
```

## Plan — for the executing agent

1. Read `tools/xtask/src/keyword_reference.rs` IN FULL — it is your template
   (HEADER banner, `BODY_SENTINEL`, `run(check)` control flow, `normalize()`,
   `workspace_root()`, the `--check` freshness contract). Read its wiring in
   `tools/xtask/src/main.rs` + `tools/xtask/src/lib.rs` and its test
   `tools/xtask/tests/keyword_reference_fresh.rs`.
2. Read `/c/tmp/runtime-surface.md` (the pre-built census — your scan must
   reproduce its package→symbol partition: 30 packages, root files → package
   `lazuli`) and confirm every symbol the INTENT_TABLE names is present there.
3. Write `tools/xtask/src/runtime_surface.rs`: `scan_surface` (glob
   `runtime/go/lazuli/**/*.go`, skip `*_test.go` + `internal/`, regex
   `^func ([A-Z]\w*)` top-level-only + `^type ([A-Z]\w*)`, package = leaf dir,
   root = `lazuli`), the `INTENT_TABLE` const (the Pareto rows above),
   `validate_intent_against_census`, `render`, `run(check)`. Match
   `keyword_reference.rs` byte-posture for the header + normalization.
4. Wire `main.rs` (`gen-runtime-surface` arm + USAGE token) and `lib.rs`
   (`pub mod runtime_surface;`).
5. Generate the doc: `cargo run -p xtask -- gen-runtime-surface` (writes
   `docs/lazuli_way/runtime-surface.md`). Eyeball it: intent section first,
   census second, every auth/lifecycle/scalar family present and correctly
   intent-labelled.
6. Inject: add the `## Runtime owns these mechanisms — never reimplement` section
   to `docs/quickref.md`; add the byte-identical scaffold bullet to BOTH
   `CLAUDE.md.tmpl` and `AGENTS.md.tmpl` (verify with `diff`); fill
   `docs/lazuli_way/delegate-to-runtime.md` fully; add the `delegate-to-runtime`
   row to `docs/lazuli_way.md`.
7. Extend `crates/lazuli_cli/tests/docs_lazuli_way.rs`: add `"delegate-to-runtime"`
   to the `frozen` slug array; add the two new tests (see Tests first). Run
   `diff CLAUDE.md.tmpl AGENTS.md.tmpl` to confirm byte-identity.
8. Write `tools/xtask/tests/runtime_surface_fresh.rs` (mirror
   `keyword_reference_fresh.rs`).
9. GATE: `cargo run -p xtask -- gen-runtime-surface --check` (must say fresh) +
   `cargo test -p xtask runtime_surface` + `cargo test -p lazuli_cli
   docs_lazuli_way` + **`cargo test --workspace`** (FULL sweep, 0 failures
   REQUIRED) + `cargo build --workspace`.
10. Commit on `loop-serial`.

## Tests first (TDD)

In `tools/xtask/tests/runtime_surface_fresh.rs`:
- [ ] `runtime_surface_is_fresh` — `xtask::runtime_surface::run(true)` is `Ok`;
      the committed doc is byte-identical to a regen. (The keyword-reference
      twin.)

In `tools/xtask/src/runtime_surface.rs` `#[cfg(test)]`:
- [ ] `census_partitions_by_package` — `scan_surface` puts `HashPassword` under
      `auth`, `TransitionAdvance` under `lazuli`, `Machine` under `lifecycle`;
      skips `*_test.go` symbols (no `TestArgon2…`) and the `internal/` dir.
- [ ] `intent_symbols_exist_in_census` — `validate_intent_against_census` is
      `Ok` for the seed `INTENT_TABLE` (every named symbol is scanned). Proves
      the curated layer can't name a vanished symbol.
- [ ] `intent_table_is_extensible` — adding a family row to a fixture table that
      names a real scanned symbol renders without touching `render`'s body
      (proves the parameterization, mirroring 0024's `table_is_extensible`).

In `crates/lazuli_cli/tests/docs_lazuli_way.rs`:
- [ ] `quickref_and_scaffold_reference_runtime_surface` — `docs/quickref.md`,
      `CLAUDE.md.tmpl`, and `AGENTS.md.tmpl` each contain the literal
      `docs/lazuli_way/runtime-surface.md`. (The injection can't be dropped.)
- [ ] `delegate_to_runtime_doc_filled` — `docs/lazuli_way/delegate-to-runtime.md`
      contains `## Reach for this`, `auth.HashPassword`, the
      `runtime-surface.md` link, and `VOCAB-RUNTIME-REINVENTED-001` (the keystone
      ties all three mechanisms; this asserts it's filled, not a stub).
- [ ] (existing) `index_links_resolve` — now includes `delegate-to-runtime` in
      `frozen`; the link resolves to the filled file.
- [ ] (existing) `scaffold_templates_stay_identical` — STILL green after the
      byte-identical bullet lands in both templates.

## Gate

### Definition of Done (reinvention-defense gate)

1. **BUILD** — `tools/xtask/src/runtime_surface.rs` implemented; `gen-runtime-surface`
   wired; **`cargo test --workspace` green (FULL sweep)**;
   `cargo run -p xtask -- gen-runtime-surface --check` reports fresh.
2. **INDEX** — `docs/lazuli_way/runtime-surface.md` is GENERATED (never
   hand-edited), intent-keyed (the auth + lifecycle + scalar + CRUD + roles +
   money families carry a "reach for this when you need to X" line), and complete
   (every package from `/c/tmp/runtime-surface.md`'s 30 in the census). The
   freshness test makes drift a build failure — the same discipline as
   `keyword-reference.md`.
3. **INJECT** — `docs/quickref.md` gains the `## Runtime owns these mechanisms`
   section; BOTH scaffold templates gain the byte-identical runtime-surface
   bullet (`scaffold_templates_stay_identical` green);
   `quickref_and_scaffold_reference_runtime_surface` green.
4. **TEACH** — `docs/lazuli_way/delegate-to-runtime.md` is FILLED (the keystone:
   the hash_password before/after, the runtime-surface link, the three
   mechanisms, the `VOCAB-RUNTIME-REINVENTED-001` reference);
   `docs/lazuli_way.md` indexes it; `delegate_to_runtime_doc_filled` +
   `index_links_resolve` green.
5. **ENFORCE** — `runtime_surface_is_fresh` (index regenerates identically) +
   `intent_symbols_exist_in_census` (curated layer stays honest) +
   `quickref_and_scaffold_reference_runtime_surface` (context pack keeps the
   reference) are the regression net.

**Five concrete gates:**
1. **BUILD** — `cargo test -p xtask runtime_surface` + `cargo test --workspace`
   0 failures + `gen-runtime-surface --check` fresh + `cargo build --workspace`.
2. **INDEX** — generated, intent-keyed, complete (30 packages); never hand-edited.
3. **INJECT** — quickref + both scaffold templates reference the index;
   templates byte-identical.
4. **TEACH** — `delegate-to-runtime.md` filled + indexed.
5. **ENFORCE** — freshness + intent-honesty + reference tests green.

## Conventions (techspec gate — MANDATORY)

- **`cargo test --workspace` FULL sweep**, 0 failures, is the hard gate (not a
  scoped subset).
- The new xtask command's freshness test MUST pass: `cargo test -p xtask`
  (specifically `runtime_surface_is_fresh`) green, AND
  `cargo run -p xtask -- gen-runtime-surface --check` reports fresh.
- Scaffold templates MUST stay **byte-identical** — the existing
  `scaffold_templates_stay_identical` test is the guard; verify with `diff`
  before committing.
- `docs/lazuli_way/runtime-surface.md` is a **generated doc — never hand-edit**;
  regenerate via `cargo run -p xtask -- gen-runtime-surface`. A hand-edit fails
  `runtime_surface_is_fresh`.

## Risks & rollback

- **Scan false-exports** (a method `func (r Recv) Name()` mis-read as a package
  export) → mitigation: the `^func ([A-Z]\w*)` regex anchors on a func name with
  no receiver-paren prefix; the `census_partitions_by_package` test asserts the
  partition matches `/c/tmp/runtime-surface.md`'s known-good shape. If a symbol
  is mis-attributed, the freshness baseline catches it on regen.
- **Intent table drifts from the runtime** (a curated row names a renamed
  symbol) → mitigation: `validate_intent_against_census` fails the generator the
  moment a named symbol leaves the scan, so the curated layer can never go stale
  silently — it's the same const-table-is-the-extension-point discipline as
  0024's `REINVENTION_TABLE`.
- **Census churn noise** (every runtime symbol add re-touches the generated doc)
  → that is the INTENDED behavior and the whole point: the doc tracks the runtime
  by construction. The freshness test failing on an un-regenerated runtime change
  is a feature, identical to `keyword-reference.md`.
- **Generated doc adds review noise** → it lives under `docs/lazuli_way/` with the
  GENERATED-DO-NOT-EDIT banner; reviewers skip it the same way they skip
  `keyword-reference.md`.

**Rollback:** `git revert` — purely additive (one generator module + one
generated doc + one freshness test + one quickref section + one byte-identical
scaffold bullet + one filled teaching doc + one index row + two assertion
tests). Absent it, behavior is today's: the runtime surface stays invisible to
the building agent. No runtime/pilot source file is touched.
