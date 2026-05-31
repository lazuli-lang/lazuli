---
id: 0005
title: Field-Policy access: shorthand — symmetric read/write
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Add `access:` as a symmetric field-policy desugaring; scope it Pauta-driven (Hostpoint-veto: nice-to-have)

## Context
- Field policy blocks take `read:` and `write:` separately. Pauta's fields are overwhelmingly symmetric: ~28 pairs where `read == write` (all 6 Contact + 5/6 Customer fields in `customer_management.lzi:184-216`; 6/8 fields in `supplier.lzi:110-134`).
- Pilot features live at `<repo>\app\features\<feature>\<feature>.lzi` (verified): `app/features/customer_management/customer_management.lzi:184-216`, `app/features/supplier/supplier.lzi:110-134`.
- Hostpoint, by contrast, has **zero** such pairs — it expresses field-level sensitivity with `@cap.PII` markers, not policy blocks. So the editorial veto "does Hostpoint need this?" returns *no* — this is a Pauta convenience, not core-critical.
- The backlog already flags this as an open question (line ~232).

## Decision
- Add `access: <policy>` as a field-policy shorthand that **desugars** to `read: <policy>` + `write: <policy>`.
- Keep explicit `read:`/`write:` for asymmetric fields. A field uses one form or the other — **mixing `access:` with `read:`/`write:` on the same field is a parse error** (no merging, no algebra).
- IR lowers `access: P` to the identical field-policy node as `read: P` + `write: P`, so all downstream consumers (codegen, doctor) see one representation and emitted Go is byte-identical.
- `lazuli doctor` gains a hint: when a field sets `read:` and `write:` to the same policy, suggest `access:` (deep-links `docs/lazuli_way/field-policy.md`).
- **Scope it Pauta-driven and say so plainly** in PRD non-goals and here: this passes the Hostpoint veto only as a nice-to-have desugaring, carrying minimal framework risk.

## Alternatives considered
- **Leave it deferred** — rejected: the repetition is concrete (~28 pairs) and the change is a low-risk desugaring; the backlog line has waited long enough.
- **Make `access:` the only form and deprecate `read:`/`write:`** — rejected: the asymmetric minority is legitimate; removing the explicit form loses expressiveness.
- **Auto-collapse matching `read:`/`write:` in codegen with no keyword** — rejected: hides symmetry intent from both the author and the doctor; an explicit shorthand teaches the idiom.
- **Generalize to a capability-marker scheme like hostpoint's `@cap.PII`** — rejected: that is a different mechanism on a different axis; out of scope.

## Consequences
**We accept:** one more field-policy key; a mixing-is-error rule to enforce; the migrate cell is `parallel_safe: false` (pauta `.lzi` contention).
**We gain:** ~28 Pauta pairs collapse to one line each with explicit symmetry intent; byte-identical codegen; backlog line ~232 resolved. Hostpoint is unaffected (no policy-block pairs; no `@cap.*` change).
**We watch:** if a third pilot also turns out symmetry-heavy, `access:` graduates from "Pauta nice-to-have" toward a core idiom — re-evaluate the veto then.
