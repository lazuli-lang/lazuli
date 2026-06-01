# Delegate to the runtime — don't reinvent it

> The carrot half of this page — "stubs delegate to the runtime by default"
> (spec 0025) — is below. The stick half (`VOCAB-RUNTIME-REINVENTED-001`, the
> advisory lint) is the rest of the page. A fuller runtime surface-index lands
> with spec 0026.

The Lazuli runtime (`runtime/go/lazuli/…`) already owns the commodity
mechanisms that pilot handlers keep re-implementing by hand. When you reach
for `golang.org/x/crypto/argon2`, `crypto/sha256` + `encoding/hex`,
`crypto/rand`, a `^#?[0-9A-Fa-f]{6}$` regex, an `UPDATE … status IN (…)` +
`RowsAffected == 0` state machine, or a hand-typed `deleted_at IS NULL`
filter — there is almost always a first-class runtime symbol or `.lzi`
primitive that does it better (and that the rest of the toolchain can see).

Reinventing it ships inferior crypto, drifts from the audit/projection
tooling, and hides the mechanism from a cold `.lzi` read.

## The rule that catches it

`VOCAB-RUNTIME-REINVENTED-001` (advisory) scans `handlers/*.go` and fires when
a handler reinvents a known runtime/language primitive, naming the exact
symbol to call instead. See
[`docs/diagnostics/README.md`](../diagnostics/README.md) for the code and
[the rule source](../../crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs)
for the seed `REINVENTION_TABLE`.

| Reinvented | Delegate to |
|---|---|
| argon2id password hashing | `auth.HashPassword` / `auth.VerifyPassword` |
| opaque token hashing | `auth.HashSessionToken` / `auth.HashWithArgon2` (`@cap.Hashed`) |
| opaque token mint | `auth.MintSessionToken` / `auth.RequestPasswordReset` |
| `UPDATE … status IN (…)` + `RowsAffected==0` | a declared `transition` (`TransitionAdvance`) |
| `^#?[0-9A-Fa-f]{6}$` hex validation | `@semantic.HexColor` |

(Soft-delete reinvention — a handler that hand-types a `deleted_at IS NULL`
read that should be a declarative `soft_delete by` lookup — is intentionally
not flagged by the substring oracle: a bare `deleted_at IS NULL` is the
normal, correct way to respect soft-delete inside a referential guard. That
family is owned by `VOCAB-SOFT-DELETE-ACTOR-001` + the `.lzi` linters.)

A reinvention that is genuinely gated on an upstream primitive may carry a
reasoned waiver: `# doctor:allow VOCAB-RUNTIME-REINVENTED-001 — reason "…"`.

## Stubs delegate to the runtime by default

The lint above is the **stick**: it catches a reinvention *after* you write it.
Spec 0025 adds the **carrot**: when codegen emits a starter stub whose binding
**site** maps to a known runtime symbol, it pre-fills the **delegating call** as
the default body — so a fresh scaffold/regenerate hands you the correct
wire-thin body, not an empty invitation to re-hand-roll argon2.

An empty `// IMPLEMENT ME` body is an invitation: you open the file, see a blank
canvas, and write argon2 by hand. A pre-filled `auth.HashPassword(...)` body is
a wall you'd have to actively tear down to reinvent — and there's no reason to:
it compiles, it's the OWASP-correct path, it's the documented Lazuli way.

**Before** (what an empty stub invites):

```go
// IMPLEMENT ME
func HashPassword(ctx *lazuli.Ctx, input string) (lazuli.HashedRef, error) {
	// … agent reaches for golang.org/x/crypto/argon2, crypto/rand,
	//    hardcoded OWASP constants, encodeArgon2id, PHC parsing …
	var zero lazuli.HashedRef
	return zero, errors.New("hash_password not yet implemented")
}
```

**After** (what 0025 emits — you edit it only for *custom* behavior):

```go
// Delegates to the Lazuli runtime — edit if you need custom behavior.
func HashPassword(ctx *lazuli.Ctx, input string) (lazuli.HashedRef, error) {
	hashed, err := auth.HashPassword(ctx, accountgen.AccountAuthPassword, input)
	if err != nil {
		var zero lazuli.HashedRef
		return zero, err
	}
	// auth.HashPassword returns the canonical PHC string; lazuli.HashedRef
	// is a string alias, so the value is the @cap.Hashed column type as-is.
	return lazuli.HashedRef(hashed), nil
}
```

The stub stays **your territory**: same `//lazuli:pattern extension_stub`
marker, same `func init()` + `lazuli.RegisterFn`, same "Lazuli will not
overwrite this file" header. Only the **body** changed. The override point
survives — if you genuinely need a custom hash, edit the body; doctor's
`VOCAB-RUNTIME-REINVENTED-001` then flags you if you tear the wall down to
re-hand-roll the runtime. Carrot (pre-filled body) + stick (lint).

### Seeded sites

| Site (`stub.site` tail) | Delegates to |
|---|---|
| `.auth.password.hash` | `auth.HashPassword(ctx, <Feature>AuthPassword, input)` |

### Coming as one-row additions

The delegation table grows O(1) — each new family is a single row once its
runtime symbol + contract-var signature is confirmed stable:

| Candidate site | Delegates to |
|---|---|
| `.auth.password.verify` | `auth.VerifyPassword` (waits on a codegen-known verify-input shape) |
| `.auth.session.*` | `auth.MintSessionToken` / `auth.HashSessionToken` |
| `.auth.password.reset.*` | `auth.RequestPasswordReset` / `auth.ConsumePasswordReset` |
| `.auth.verify.*` | `auth.IssueVerification` / `auth.ConsumeVerification` |

### Honest limits

- **Regenerate-only.** Smart stubs shape only what codegen writes on a *fresh*
  scaffold or regenerate of a *not-yet-authored* handler. An existing
  hand-written handler on disk is never touched (regen skips it). The carrot
  prevents the *next* reinvention; the lint + batch fixes cover existing ones.
- **Compile-gated.** The delegating body emits only when the stub's resolved Go
  in/out types are shape-compatible with the runtime symbol (for the hash row:
  input `string`, output `lazuli.HashedRef`/`string`). An un-typed `@fn` (whose
  signature falls back to `any`) takes the plain `// IMPLEMENT ME` path until
  you tighten it to `Function[Text, Hashed(...)]` — a delegating body that
  wouldn't compile is never emitted.
