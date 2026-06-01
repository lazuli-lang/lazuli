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
The full per-family "reach for this when you need to X" map is the intent section
of [`runtime-surface.md`](runtime-surface.md).

## The three mechanisms (why this stopped happening)

The audit's root cause was not malice, it was **invisibility** — the runtime
surface was not in the building agent's context window. Three mechanisms close
the class permanently:

1. **Smart stubs (mechanism i).** A delegating stub body (`auth.HashPassword`,
   `TransitionAdvance`, the CRUD-effect call) is generated instead of an empty
   `// IMPLEMENT ME` marker, so the runtime call is the default and hand-rolling
   is a wall the agent must tear down. An empty `// IMPLEMENT ME` body is an
   invitation to re-hand-roll argon2; a pre-filled `auth.HashPassword(...)` body
   compiles, is OWASP-correct, and is the documented Lazuli way. The stub stays
   *your* territory (same `//lazuli:pattern extension_stub` marker, same
   `RegisterBindingFn`); only the **body** changed. It is **regenerate-only**
   (an existing hand-written handler on disk is never touched) and
   **compile-gated** (the delegating body emits only when the resolved Go in/out
   types are shape-compatible with the runtime symbol).
2. **The runtime-surface index (mechanism ii — this doc's companion).**
   [`runtime-surface.md`](runtime-surface.md) is generated from
   `runtime/go/lazuli/` and injected into the context pack (`quickref.md` + the
   scaffold `CLAUDE.md`/`AGENTS.md`), so the surface is impossible to not-see.
   It is **intent-keyed**: a flat list of `HashPassword VerifyPassword …` does
   not defeat reinvention; *"hash a password → `auth.HashPassword`"* does.
3. **The doctor rule (mechanism iii).** `VOCAB-RUNTIME-REINVENTED-001` is the
   backstop: it fires at lint time on a handler that imports
   `golang.org/x/crypto/argon2` (or matches a reinvention body-shape) when the
   runtime exports the equivalent — naming the exact symbol to call instead.

Carrot (pre-filled body) + index (impossible to not-see) + stick (lint): the
reinvention class is closed.

## Enforced by

- `VOCAB-RUNTIME-REINVENTED-001`
  ([crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs](../../crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs))
  — fires on a handler that reimplements a runtime-owned mechanism; names the
  equivalent symbol and links here. Suppress a genuine upstream-gap case with
  `# doctor:allow VOCAB-RUNTIME-REINVENTED-001 — reason "..."`.
- `runtime_surface_is_fresh`
  ([tools/xtask/tests/runtime_surface_fresh.rs](../../tools/xtask/tests/runtime_surface_fresh.rs))
  — keeps the index regenerable-identical from the live runtime, so it never
  drifts. Regenerate with `cargo run -p xtask -- gen-runtime-surface`.
- `quickref_and_scaffold_reference_runtime_surface`
  ([crates/lazuli_cli/tests/docs_lazuli_way.rs](../../crates/lazuli_cli/tests/docs_lazuli_way.rs))
  — keeps the index referenced from the context pack so the injection can't be
  silently dropped.
