---
id: 0004
title: Defaults Hoist — defaults rate_limit + defaults audit
type: prd
track: evolve/ship
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
test_gate: "cargo test -p lazuli_syntax defaults && cargo test -p lazuli_codegen_go defaults"
---

# PRD — Defaults Hoist (rate_limit + audit)

## Problem
The feature-level `defaults` block already hoists `tenancy`, `timestamps`, `soft_delete`, and `retention` (and `policy_for` via the resource-conventions surface), but `rate_limit` and `audit` are still spelled out on every command. The pilots prove the cost in pure copy-paste with near-zero variation:
- `rate_limit` repeated ~230 duplicate lines — pauta `media_price_tables` ×18, pauta `customer_management` ×16, hostpoint `operations` 5/5 identical, the account dev-override line ×25.
- `audit default` repeated ~215 duplicate lines — pauta 126 / hostpoint 89, **zero variation**.
- Total ~445 lines of boilerplate every author (human or agent) must copy correctly per command, and that drifts silently when one copy is edited.

## Why now (or why ever)
This is the highest-volume mechanical repetition the pilot audit found (~445 lines, two pilots) and the fix is the cheapest kind: reuse the inheritance rule `defaults` already has for `tenancy`/`timestamps`/`policy_for`. No new mental model, no data-model redesign. Leaving it means every new feature re-copies the same two lines per command forever.

## Outcome — done means
1. The feature-level `defaults` block accepts two new keys: `defaults rate_limit "<spec>"` and `defaults audit default`.
2. Each command inherits the feature default unless it sets its own value (per-command value wins); `audit off` on a command opts that command out.
3. IR carries the hoisted `rate_limit`/`audit` on the defaults node; codegen resolves the effective per-command value so emitted Go is **byte-identical** to the pre-hoist output.
4. `lazuli doctor` emits a hint when a feature repeats an identical `rate_limit` or `audit` on ≥3 commands (suggest the hoist).
5. Both pilots migrated; the ~445 duplicate lines erased.
6. `docs/lazuli_way/feature-defaults.md` (stub created by 0001) is filled; after this lands, the 0001 seed `note` feature is upgraded to use `defaults rate_limit` + `defaults audit`.

## Non-goals
- **String→struct axis stays deferred.** This is the HOIST axis ONLY: `rate_limit` stays a string spec, `audit` keeps its existing keyword shape. Turning `rate_limit "<spec>"` into a structured `{count, per, key}` block is a separate, still-deferred change (index "Deliberately cut / deferred") — do not touch it here.
- No new `audit` modes or `rate_limit` algorithms.
- No rewriting of unrelated pilot `.lzi` content — touch only the `rate_limit`/`audit` lines being hoisted.

## User stories
- As an `.lzi` author whose commands all share one `rate_limit`/`audit`, I declare each once in `defaults` and override only where a command actually differs.
- As an agent reading `lazuli doctor`, I get told "these 5 commands repeat the same `rate_limit` — hoist it into `defaults`" with a deep link to the idiom doc.

## Constraints
- `parallel_safe: false` — contends on pilot `.lzi` files (`customer_management.lzi` is also touched by 0003/0005/0015/0017). Serialize the migrate cell per pilot; the language/IR/codegen/doctor BUILD cells are parallel-safe.
- Codegen output must be byte-identical pre/post hoist (the migration's safety anchor).

## Open questions
None. Decisions in the ADR.
