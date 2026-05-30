---
title:   "Forcing defaults and feature hygiene"
slug:    forcing-defaults-and-hygiene
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, doctor, audit, policy, hygiene, iron-hand]
---

# Forcing defaults and feature hygiene

This is the "will my feature pass `lazuli doctor`?" checklist. Under the
`tdd-iron-hand` / production posture these warnings become CI-gating errors, so
internalise them once and the shapes write themselves. Three classes of gaffe
dominate: a policy category named after an effect, a write command with no
`audit`, and a feature missing its context/boundary/test trio.

## 1. Policy categories are semantic, never CRUD

A `policies` block maps **authorization categories** to identity atoms. The
category name must be *semantic* — what the actor is allowed to do — not an
effect verb. Canonical categories are **`author` / `view` / `edit` / `remove`**
(and `manage`). Never `create` / `read` / `update` / `delete` (nor the plural
`creates` / `reads` / `updates` / `deletes`).

The reason is a near-collision: at a reference site, `policy @policy.create`
reads as a *write effect*, not an authorization category. The doctor rejects the
shadowing outright with `POLICY-CATEGORY-SHADOWS-EFFECT-001` (an **error** under
iron-hand and production).

```lazuli
  policies
    author: @role.admin, @role.finance
    view: @scope.same_org
    edit: @role.admin
    remove: @role.admin
```

Write `create: @role.admin` instead and doctor fires:
`policy category 'create' shadows a command effect verb … Rename to a semantic
name, e.g. 'create' → 'author'`. The fix is the rename plus every `@policy.create`
reference that points at it. (Note: the per-field `read:` / `write:` *access
directions* under a `fields <Resource>` sub-block are a different closed catalog
— those are fine and never flagged.)

## 2. `audit` is a forcing default on every write command

Rate-limit, webhook `verify`, and tenant-derivation are the forcing defaults
covered in [justified-opt-outs](0005-justified-opt-outs.md). `audit` is the
fourth: every command that mutates state (`creates` / `updates` / `deletes`) or
`emits` an event must declare an `audit` child, or `VOCAB-AUDIT-001` fires (warn
under strict, **error** under production). It takes one of three forms. Unlike
`rate_limit none` / `verify none`, the checker does not *hard-require* a `reason`
on `audit none` — but treat the opt-out as a deliberate, reviewable choice
(leave a `# ...` note saying why) rather than a reflex:

```lazuli
  # audit default — log the framework's default field set
  command record_entry
    input
      amount: Integer required
    policy @policy.author
    rate_limit "120 per minute per user"
    audit default
    creates Entry from input
```

```lazuli
  # audit <fields> — log a specific, named field list
  command annotate
    route id: ID
    input
      memo: Text required
    policy @policy.edit
    rate_limit "120 per minute per user"
    audit actor, target.id, input.memo
    updates Entry
      memo = input.memo
```

```lazuli
  # audit none — the explicit opt-out; note why this command needs no audit trail
  command sync_external
    input
      amount: Integer required
    policy @policy.author
    rate_limit "120 per minute per user"
    audit none
    creates Entry from input
```

`audit` gives compliance tooling a *typed* contract instead of guessing from
event names. There is no feature-level `defaults audit …` shorthand — the
`defaults` block accepts only `tenancy`, `timestamps`, and `policy_for`, so the
`audit` decision lives on each command (the parser hard-rejects `audit` under
`defaults`).

## 3. The hygiene trio: ctx.md, a non_goals boundary, an inline test

Three more lints gate a "complete" feature. Each maps to a small, mechanical
requirement:

- **`<feature>.ctx.md` sidecar** — co-located next to the `.lzi`, ≥100
  non-whitespace characters. Feature *context prose* lives here by convention,
  not in a keyword (`VOCAB-CONTEXT-CTXMD-001`; the old `attach_ctx` is retired —
  see [retired-forms-and-replacements](0004-retired-forms-and-replacements.md)).
- **At least one `non_goals` boundary** — so a cold reader sees the feature's
  scope edges (`VOCAB-CONTEXT-NONGOALS-001`). A flat list of bare quoted strings,
  or a `delegated_to` / `out_of_scope` partition, both satisfy it.
- **At least one inline `test` block** — on a command, rule, or lifecycle
  transition, with real `allows` / `denies` assertions (`VOCAB-TESTS-MISSING-001`).
  An empty `tests` block is theater and does not count.

```lazuli
feature ledger
  purpose "Append-only money movements within an org."

  non_goals
    "Invoice rendering (use the billing feature)"

  defaults
    tenancy org
    timestamps

  domain
    resource Entry
      amount: Integer required
      memo: Text optional

  policies
    author: @role.admin, @role.finance
    view: @scope.same_org
    remove: @role.admin

  command record_entry
    input
      amount: Integer required
    policy @policy.author
    rate_limit "120 per minute per user"
    audit default
    creates Entry from input
    tests
      allows when input.amount == 100
      denies when input.amount == 0
```

Pair that `.lzi` with a real `ledger.ctx.md` beside it and the trio is satisfied.
Tests speak one verb pair — `allows` / `denies` — with the typed subject naming
the dimension (`when <predicate>` here). Do not hand-author the command's policy
*authorization* matrix: doctor derives `permits` / `forbids` rows from
`policy @policy.*` for you; you author only the behavioral predicates it cannot
infer.

## The point of forcing defaults

None of these are style nits. A silently un-audited write is invisible to
compliance; a CRUD-named policy hides whether `@policy.update` means "edit" or
"the update effect"; a feature with no boundary, context, or test is a black box
to the next cold-reading agent. The defaults force the decision to the surface so
it can never be lost. When unsure whether a feature passes, **ask the oracle** —
run `lazuli doctor <dir>` and read the codes
([the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md)); it names the
exact rule and the exact fix.

Authoritative spec: `docs/quickref.md` §"Security Checklist" + §"Policy
Vocabulary", `docs/canonical-semantics.md` §"feature-context-vocabulary",
`docs/invariants.md` (audit), and the doctor rule sources
(`POLICY-CATEGORY-SHADOWS-EFFECT-001`, `VOCAB-AUDIT-001`,
`VOCAB-CONTEXT-CTXMD-001`, `VOCAB-CONTEXT-NONGOALS-001`,
`VOCAB-TESTS-MISSING-001`).
