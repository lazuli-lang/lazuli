# Delegate to the runtime — don't reinvent it

> **Stub.** This page is filled in by spec 0026 (the runtime surface-index +
> smart-stubs teach cell). For now it exists so the diagnostic message for
> `VOCAB-RUNTIME-REINVENTED-001` resolves to a real anchor.

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
