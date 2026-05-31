---
id: 0004
title: Defaults Hoist — defaults rate_limit + defaults audit
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
test_gate: "cargo test -p lazuli_syntax defaults && cargo test -p lazuli_codegen_go defaults"
agent: unassigned
---

# TechSpec — Defaults Hoist (rate_limit + audit)

## Approach
Reuse the existing `defaults`-block inheritance machinery (the one `policy_for`/`tenancy` already ride) to add two keys: `defaults rate_limit "<spec>"` and `defaults audit default`. Mechanical, no data-model redesign — `rate_limit` stays a string, `audit` keeps its shape (string→struct axis stays deferred, a hard non-goal). Build spans grammar → IR → codegen → doctor; then MIGRATE both pilots and prove codegen is byte-identical pre/post. Finally fill the teaching doc and upgrade the 0001 seed.

## Surface
**Modify (grammar / keywords):**
- `crates/lazuli_keywords/src/registry/sections/s11.rs` — extend the "defaults-block: project-default modifiers" group (~line 218, `tenancy`/`timestamps`/`soft_delete`/`retention`) with two `stmt(...)` entries: `rate_limit` and `audit`, both `Context::Defaults`, `DEFAULTS` flag.
- `crates/lazuli_keywords/src/registry/sections/s08.rs` — mirror the existing `policy_for` `Context::Defaults` wiring (s08.rs:31) for the two new keys; touch `s05.rs` only if the surface table there also needs them.

**Modify (parser / IR / codegen / doctor):**
- `crates/lazuli_syntax/` — parse `rate_limit`/`audit` inside the `defaults` block; keep per-command `rate_limit`/`audit`/`audit off` parsing as today.
- `crates/lazuli_ir/` — the feature defaults node gains `rate_limit: Option<String>` and `audit: Option<AuditMode>` (reuse the existing audit enum); mirror how `policy_for`/`tenancy` defaults are carried.
- `crates/lazuli_codegen_go/` — effective per-command resolution: `command.rate_limit.or(defaults.rate_limit)`; `audit` likewise with `audit off` clearing the inherited default. Emitted Go must equal the fully-explicit form.
- `crates/lazuli_doctor/` — new hint rule: a feature with an identical `rate_limit` or `audit` repeated on ≥3 commands → suggest the hoist, deep-link `docs/lazuli_way/feature-defaults.md`.

**Create (tests):**
- `crates/lazuli_syntax/tests/defaults.rs` — parse a feature with `defaults rate_limit "<spec>"` + `defaults audit default`; assert per-command override wins and `audit off` opts out; assert IR defaults node carries both.
- `crates/lazuli_codegen_go/tests/defaults.rs` — golden: a hoisted feature and its fully-explicit equivalent emit identical Go; an `audit off` command emits no audit.

**Migrate (pilots):** both pilots store features at `<repo>\app\features\<feature>\<feature>.lzi` (verified).
- `C:\Users\lucas\dev\pauta-web-monorepo\app\features\media_price_tables\media_price_tables.lzi` — `rate_limit` ×18 → `defaults`.
- `C:\Users\lucas\dev\pauta-web-monorepo\app\features\customer_management\customer_management.lzi` — `rate_limit` ×16 → `defaults`.
- `C:\Users\lucas\dev\pauta-web-monorepo\app\features\account\account.lzi` — dev-override `rate_limit` line ×25 → `defaults`.
- `C:\Users\lucas\hostpoint\app\features\operations\operations.lzi` — `rate_limit "10 per 10 minutes per ip"` + `audit default` 5/5 identical (verified, lines ~237–301) → `defaults`.
- pauta + hostpoint `.lzi` `audit default` lines — pauta 126 / hostpoint 89, zero variation → `defaults audit default`.

**Teach / docs:**
- `docs/lazuli_way/feature-defaults.md` — replace the 0001 stub: fixed idiom-doc shape (Reach for this → Before/After pilot excerpt → Enforced by); document the `rate_limit`/`audit` hoist + inheritance + `audit off` opt-out.
- `lazurite/templates/default/app/features/note/note.lzi` — upgrade the 0001 seed: replace the placeholder comment with real `defaults rate_limit` + `defaults audit`.
- `.specs/index.md` — already lists 0004; no row change (downstream specs fill files, not the index).

## Contracts
**Inheritance rule (identical to `policy_for`):** effective value = per-command value if present, else feature `defaults` value; `audit off` on a command clears the inherited default for that command.

**Byte-identity invariant:** for any feature, codegen of `{defaults rate_limit X; audit default}` + N commands == codegen of N commands each spelling `rate_limit X` + `audit default` explicitly. This is the migration safety net.

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p lazuli_syntax defaults` and `cargo test -p lazuli_codegen_go defaults` green for the new grammar/IR/codegen.
2. MIGRATE: pauta (`app/features/media_price_tables`, `customer_management`, `account`) + hostpoint (`app/features/operations`) hoisted; ~445 dup lines erased; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and pauta-web; pre/post Go byte-identical.
3. TEACH: `docs/lazuli_way/feature-defaults.md` filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added; 0001 seed `note` upgraded to use `defaults rate_limit`+`audit`.
4. ENFORCE: the ≥3-identical doctor hint fires on a hand-rolled repeat feature OR the upgraded seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

## Plan — for the executing agent
1. Add `rate_limit` + `audit` to the `defaults` group in `s11.rs` (~line 218) and the surface in `s05.rs`.
2. Parse them in `lazuli_syntax`; thread onto the defaults IR node in `lazuli_ir` (reuse the audit enum). Write `crates/lazuli_syntax/tests/defaults.rs` first (TDD).
3. Implement effective-value resolution in `lazuli_codegen_go`; write `crates/lazuli_codegen_go/tests/defaults.rs` golden proving byte-identity + `audit off`.
4. Add the ≥3-identical hint in `lazuli_doctor`; assert it stays silent at 2 commands or with any variation.
5. MIGRATE pauta (3 features) then hostpoint (`operations`) — per pilot, serialized. After each, diff generated Go pre/post: must be empty.
6. Fill `docs/lazuli_way/feature-defaults.md`; add the CLAUDE.md.tmpl + AGENTS.md.tmpl "Authoring idioms" bullet ("reach for `defaults rate_limit`/`audit`, not per-command copies").
7. Upgrade the 0001 seed `note.lzi` to use the new keys; re-run `lazuli check && lazuli doctor` on the templated app until clean.

## Tests first (TDD)
- [ ] `defaults_rate_limit_audit_parse` — feature-level `defaults rate_limit`/`defaults audit` parse; IR defaults node carries both.
- [ ] `per_command_override_wins` — a command's own `rate_limit`/`audit` beats the default.
- [ ] `audit_off_opts_out` — `audit off` clears the inherited default for that command.
- [ ] `codegen_byte_identical` — hoisted vs fully-explicit feature emit identical Go.
- [ ] `doctor_hoist_hint_ge3` — hint fires at 3 identical, silent at 2 and on variation.

## Gate
`test_gate` green **and** both pilots migrated with empty pre/post Go diff **and** `feature-defaults.md` reads as a coherent before/after idiom doc **and** the seed `note` visibly uses `defaults rate_limit`+`audit`.

## Risks & rollback
- Codegen drift between hoisted and explicit forms → mitigation: the byte-identity golden test is the hard gate; migrate only after it is green.
- `audit off` interaction with the inherited default mis-handled → mitigation: explicit `audit_off_opts_out` test before any migration.
- Pilot `.lzi` contention with 0003/0005/0015/0017 → mitigation: `parallel_safe: false`; serialize the migrate cell per pilot.

**Rollback:** `git revert` the language commit (keys are additive; old per-command spellings still parse) and revert each pilot migration commit independently.
