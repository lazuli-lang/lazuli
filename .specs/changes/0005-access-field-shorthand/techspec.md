---
id: 0005
title: Field-Policy access: shorthand — symmetric read/write
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
test_gate: "cargo test -p lazuli_syntax field_access_shorthand"
agent: unassigned
---

# TechSpec — Field-Policy `access:` Shorthand

## Approach
Pure desugaring: `access: P` lowers to `read: P` + `write: P` at IR build, so codegen and doctor see one representation and emitted Go is byte-identical to the explicit form. Grammar adds one key + a one-form-per-field rule; IR desugars; doctor hints the collapse. Then MIGRATE Pauta's symmetric pairs only (Hostpoint untouched — no policy-block pairs), fill the teaching doc, and un-defer backlog line ~232. Scope is Pauta-driven by design (Hostpoint veto → nice-to-have).

## Surface
**Modify (grammar / keywords):**
- `crates/lazuli_keywords/src/registry/sections/` — add an `access` field-policy key alongside the existing `read`/`write` policy keys (locate the section that registers `read`/`write` field-policy statements; `s05.rs` carries the resource-body field modifiers, the `policy_for`/defaults wiring is in `s08.rs`). Register `access` in the same section/`Context` as `read`/`write`.

**Modify (parser / IR / doctor):**
- `crates/lazuli_syntax/` — parse `access: <policy>` in field policy blocks; reject mixing `access:` with `read:`/`write:` on the same field (clear diagnostic).
- `crates/lazuli_ir/` — desugar `access: P` → `{ read: P, write: P }` field-policy node at IR build, identical to the explicit form.
- `crates/lazuli_doctor/` — hint rule: a field whose `read:` and `write:` are the same policy → suggest `access:`, deep-link `docs/lazuli_way/field-policy.md`.

**Create (tests):**
- `crates/lazuli_syntax/tests/field_access_shorthand.rs` — `access: @role.admin` parses; mixing `access:` + `read:` on one field is a parse error; explicit `read:`/`write:` still parse; assert `access: P` and explicit `read: P`/`write: P` produce identical field-policy IR.

**Migrate (Pauta only):** features verified at `<repo>\app\features\<feature>\<feature>.lzi`.
- `C:\Users\lucas\dev\pauta-web-monorepo\app\features\customer_management\customer_management.lzi` (field-policy block, ~lines 184–216) — 6 Contact + 5/6 Customer symmetric fields → `access:` (leave the 1/6 asymmetric Customer field explicit).
- `C:\Users\lucas\dev\pauta-web-monorepo\app\features\supplier\supplier.lzi` (field-policy block, ~lines 110–134) — 6/8 symmetric fields → `access:` (leave the 2/8 asymmetric fields explicit).
- Hostpoint is NOT touched (no policy-block field pairs; uses `@cap.PII`).

**Teach / docs:**
- `docs/lazuli_way/field-policy.md` — replace the 0001 stub: fixed idiom-doc shape; document `read:`/`write:` and the `access:` symmetric shorthand + the one-form-per-field rule, with the Pauta before/after excerpt.
- `docs/language-backlog.md` — un-defer line ~232; mark RESOLVED (implemented here).
- `.specs/index.md` — already lists 0005; no row change.

## Contracts
**Desugaring rule:** `access: P` ≡ `read: P` + `write: P`. One form per field — `access:` and `read:`/`write:` on the same field is a parse error.

**Byte-identity invariant:** for any field, codegen of `access: P` == codegen of explicit `read: P` + `write: P`. This is the migration safety net.

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p lazuli_syntax field_access_shorthand` green (parse + IR-identity + mixing-error).
2. MIGRATE: pauta `customer_management` + `supplier` symmetric pairs collapsed to `access:` (~28 pairs); asymmetric minority untouched; hostpoint untouched; `lazuli check && lazuli doctor && go build ./...` clean in pauta-web; pre/post Go byte-identical.
3. TEACH: `docs/lazuli_way/field-policy.md` filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added; backlog line ~232 un-deferred.
4. ENFORCE: the read==write doctor hint fires on a symmetric field OR the migrated Pauta demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

## Plan — for the executing agent
1. Add the `access` field-policy key in `s11.rs` + surface in `s05.rs`.
2. Parse `access:` in `lazuli_syntax`; add the mixing-is-error rule. Write `crates/lazuli_syntax/tests/field_access_shorthand.rs` first (TDD), including the IR-identity assertion.
3. Desugar `access: P` → `{read:P, write:P}` in `lazuli_ir`.
4. Add the read==write hint in `lazuli_doctor`; assert silent on asymmetric fields and on `access:` fields.
5. MIGRATE Pauta `customer_management.lzi` then `supplier.lzi` — collapse only the symmetric pairs; leave the asymmetric minority explicit. After each, diff generated Go pre/post: must be empty.
6. Fill `docs/lazuli_way/field-policy.md`; add the CLAUDE.md.tmpl + AGENTS.md.tmpl "Authoring idioms" bullet ("reach for `access:` when read==write").
7. Un-defer backlog line ~232 in `docs/language-backlog.md` (mark RESOLVED, implemented here).

## Tests first (TDD)
- [ ] `access_shorthand_parses` — `access: @role.admin` parses in a field policy block.
- [ ] `access_desugars_to_read_write` — `access: P` and explicit `read: P`/`write: P` produce identical field-policy IR.
- [ ] `mixing_access_and_read_errors` — `access:` + `read:` on one field is a parse error.
- [ ] `explicit_read_write_still_parses` — asymmetric `read:`/`write:` unaffected.
- [ ] `doctor_suggests_access_on_symmetric` — hint fires when read==write, silent otherwise.

## Gate
`test_gate` green **and** Pauta migrated with empty pre/post Go diff **and** `field-policy.md` reads as a coherent before/after idiom doc **and** backlog line ~232 marked RESOLVED.

## Risks & rollback
- Desugaring not bit-identical to explicit form → mitigation: the IR-identity test + byte-identity Go diff are hard gates; migrate only after green.
- Over-migration (collapsing an asymmetric field by mistake) → mitigation: migrate field-by-field, confirm read==write before each collapse; the empty-Go-diff check catches any behavior change.
- Pauta `.lzi` contention with 0003/0004/0015/0017 → mitigation: `parallel_safe: false`; serialize the migrate cell.

**Rollback:** `git revert` the language commit (`access:` is additive; explicit `read:`/`write:` still parse) and revert the Pauta migration commit independently.
