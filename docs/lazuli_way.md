# The Lazuli Way

Authoring idioms canon. Each idiom answers the same question — *what do I reach
for instead of hand-rolling?* — and binds the answer to the doctor rule (or
scaffold seed) that enforces it.

This file is a thin **index**. Every idiom lives in its own file under
`lazuli_way/` so that parallel feature work never collides on one document. Each
idiom doc follows a fixed shape:

```
# <Idiom name>
## Reach for this
<one sentence>
## Before (hand-rolled)  /  After (idiomatic)
<real pilot excerpt, file:line>
## Enforced by
<DOCTOR-RULE-CODE> — <what it fires on>
```

A feature is not *done* until its idiom doc here is filled and the scaffold
bullet exists — see [definition-of-done.md](lazuli_way/definition-of-done.md).

## Idioms

| idiom | reach for | status |
|-------|-----------|--------|
| [crud-by-convention](lazuli_way/crud-by-convention.md) | `conventions [crud]` instead of hand-rolled create/update/delete commands | filled |
| [escape-hatch-decision-tree](lazuli_way/escape-hatch-decision-tree.md) | typed effect → `query.sql`/`query.compose` → `@fn`; never raw SQL inside a `@fn` Go handler | filled |
| [feature-defaults](lazuli_way/feature-defaults.md) | a `defaults` block instead of repeating `tenancy`/`rate_limit`/`audit` per command | stub (spec 0004) |
| [field-policy](lazuli_way/field-policy.md) | symmetric `access:` field shorthand instead of paired read/write blocks | stub (spec 0005) |
| [one-feature-one-capability](lazuli_way/one-feature-one-capability.md) | split a sprawling feature along its resource graph | stub (spec 0008/0009) |
| [referential-guards](lazuli_way/referential-guards.md) | `guard references` / `restrict on_delete` instead of hand-copied guards | stub (spec 0014) |
| [soft-delete](lazuli_way/soft-delete.md) | `soft_delete` with a `deleted_by` actor column | stub (spec 0015) |
| [money](lazuli_way/money.md) | first-class `Money` instead of amount-cents + currency-string pairs | stub (spec 0016) |
| [state-machines](lazuli_way/state-machines.md) | a closed `state {}` bound to `transition` instead of free Text status fields | stub (spec 0017) |
| [comment-hygiene](lazuli_way/comment-hygiene.md) | `doctor:allow … reason` for intentional waivers; no noise comments | stub (spec 0007/0008) |
| [delegate-to-runtime](lazuli_way/delegate-to-runtime.md) | the runtime verb (`auth.HashPassword`, `lazuli.TransitionAdvance`, …) via the intent-keyed [runtime-surface index](lazuli_way/runtime-surface.md) instead of hand-rolling argon2 / a manual transition / a regex validator | filled |
