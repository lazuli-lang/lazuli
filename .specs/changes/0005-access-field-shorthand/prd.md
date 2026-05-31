---
id: 0005
title: Field-Policy access: shorthand — symmetric read/write
type: prd
track: evolve/ship
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
test_gate: "cargo test -p lazuli_syntax field_access_shorthand"
---

# PRD — Field-Policy `access:` Shorthand

## Problem
Field policy blocks require separate `read:` and `write:` policies even when they are identical. In Pauta this is the common case, not the exception:
- pauta `app/features/customer_management/customer_management.lzi:184-216` — all 6 Contact fields and 5/6 Customer fields have byte-identical `read == write`.
- pauta `app/features/supplier/supplier.lzi:110-134` — 6/8 fields symmetric.
- ~28 symmetric `read:`/`write:` pairs in Pauta, each spelling the same policy twice. The asymmetric minority (1/6 Customer, 2/8 Supplier) is real but small.

This is backlog line ~232: "Decide whether field policy shorthand such as `access: @role.admin` is worth adding for symmetric read/write policies." This PRD promotes that line from deferred to shipped.

## Why now (or why ever)
The Pauta repetition is concrete (~28 pairs spelling each policy twice) and the fix is a pure desugaring — `access: P` ⇒ `read: P` + `write: P` — with byte-identical codegen and near-zero framework risk. It is a low-cost win that also makes symmetry *intent* explicit instead of implied by two matching lines.

## Outcome — done means
1. Field policy blocks accept `access: <policy>`, setting both read and write.
2. Explicit `read:`/`write:` still works for the asymmetric minority; a field uses one form or the other, never both (mixing is a parse error).
3. IR desugars `access: P` to the same field-policy node as `read: P` + `write: P`; codegen output is byte-identical.
4. `lazuli doctor` hints `access:` when a field sets `read:` and `write:` to the same policy.
5. Pauta `customer_management` + `supplier` symmetric pairs migrated to `access:`; the asymmetric minority and hostpoint untouched.
6. `docs/lazuli_way/field-policy.md` (stub from 0001) filled; backlog line ~232 un-deferred / marked RESOLVED.

## Non-goals
- **Not core-critical — Pauta-driven nice-to-have.** Apply the editorial veto "does Hostpoint need this?": Hostpoint has **zero** policy-block field pairs — it expresses field sensitivity with `@cap.PII` field markers, not `read:`/`write:` blocks. So `access:` is a convenience for the Pauta shape, not a framework necessity. It ships because Pauta earns it, and the ADR/PRD say so plainly.
- Not deprecating or removing explicit `read:`/`write:` — the asymmetric minority keeps them.
- Not touching hostpoint `@cap.*` field markers (different mechanism, out of scope).
- No policy algebra: no mixing `access:` with `read:`/`write:` on one field.

## User stories
- As a Pauta `.lzi` author writing a field whose read == write, I write one `access:` line instead of two matching lines.
- As an agent reading `lazuli doctor`, I get told "this field's read and write are identical — use `access:`" with a deep link to the idiom doc.

## Constraints
- `parallel_safe: false` — contends on pauta `.lzi` (`customer_management.lzi` also touched by 0003/0004/0015/0017). Serialize the migrate cell; the language/IR/doctor BUILD cells are parallel-safe.
- Codegen byte-identical pre/post desugaring (the migration's safety anchor).

## Open questions
None. Decisions in the ADR.
