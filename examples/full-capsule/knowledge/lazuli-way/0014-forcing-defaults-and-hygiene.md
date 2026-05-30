---
title:   "Forcing defaults and feature hygiene"
slug:    forcing-defaults-and-hygiene
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, doctor, audit, policy, hygiene, iron-hand]
read_when: "finishing a feature — will it pass doctor? (policy names, audit, .ctx.md, non_goals, tests)"
---

# Forcing defaults and feature hygiene

The "will my feature pass `lazuli doctor`?" checklist. Under `tdd-iron-hand` / production these warnings become CI-gating errors. Three gaffes dominate: a policy category named after an effect, a write command with no `audit`, a feature missing the context/boundary/test trio.

## 1. Policy categories are semantic, never CRUD

A `policies` block maps **authorization categories** to identity atoms. The category names what the actor may *do*, not an effect verb. Canonical: **`author` / `view` / `edit` / `remove`** (and `manage`). Never `create` / `read` / `update` / `delete` (nor plural `creates` / `reads` / `updates` / `deletes`).

Why: at a reference site, `policy @policy.create` reads as a *write effect*, not a category. Doctor rejects the shadowing with `POLICY-CATEGORY-SHADOWS-EFFECT-001` (**error** under iron-hand/production).

```lazuli
  policies
    author: @role.admin, @role.finance
    view: @scope.same_org
    edit: @role.admin
    remove: @role.admin
```

`create: @role.admin` fires: `policy category 'create' shadows a command effect verb … Rename to a semantic name, e.g. 'create' → 'author'`. Fix = the rename plus every `@policy.create` reference. (The per-field `read:` / `write:` *access directions* under a `fields <Resource>` sub-block are a different closed catalog — fine, never flagged.)

## 2. `audit` is a forcing default on every write command

Rate-limit, webhook `verify`, and tenant-derivation are the forcing defaults in [justified-opt-outs](0005-justified-opt-outs.md). `audit` is the fourth: every command that mutates state (`creates` / `updates` / `deletes`) or `emits` an event must declare an `audit` child, else `VOCAB-AUDIT-001` fires (warn under strict, **error** under production). Three forms. Unlike `rate_limit none` / `verify none`, `audit none` does not *hard-require* a `reason` — but treat the opt-out as deliberate (leave a `# ...` note why), not a reflex:

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

`audit` gives compliance tooling a *typed* contract instead of guessing from event names. No feature-level `defaults audit …` shorthand exists — `defaults` accepts only `tenancy`, `timestamps`, `policy_for`, so the `audit` decision lives per-command (the parser hard-rejects `audit` under `defaults`).

## 3. The hygiene trio: ctx.md, a non_goals boundary, an inline test

Three lints gate a "complete" feature:

- **`<feature>.ctx.md` sidecar** — co-located next to the `.lzi`, ≥100 non-whitespace chars. Feature *context prose* lives here by convention, not in a keyword (`VOCAB-CONTEXT-CTXMD-001`; the old `attach_ctx` is retired — see [retired-forms-and-replacements](0004-retired-forms-and-replacements.md)).
- **≥1 `non_goals` boundary** — shows the feature's scope edges to a cold reader (`VOCAB-CONTEXT-NONGOALS-001`). A flat list of bare quoted strings, or a `delegated_to` / `out_of_scope` partition, both satisfy it.
- **≥1 inline `test` block** — on a command, rule, or lifecycle transition, with real `allows` / `denies` assertions (`VOCAB-TESTS-MISSING-001`). An empty `tests` block is theater and does not count.

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

Pair that `.lzi` with a real `ledger.ctx.md` beside it and the trio is satisfied. Tests speak one verb pair — `allows` / `denies` — with the typed subject naming the dimension (`when <predicate>` here). Do not hand-author the policy *authorization* matrix: doctor derives `permits` / `forbids` rows from `policy @policy.*`; you author only the behavioral predicates it cannot infer.

## Why force these

None are style nits. A silently un-audited write is invisible to compliance; a CRUD-named policy hides whether `@policy.update` means "edit" or "the update effect"; a feature with no boundary, context, or test is a black box to the next cold-reading agent. Forcing the decision to the surface keeps it from being lost. When unsure, **ask the oracle** — `lazuli doctor <dir>` names the exact rule and fix ([the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md)).

Authoritative spec: `docs/quickref.md` §"Security Checklist" + §"Policy Vocabulary", `docs/canonical-semantics.md` §"feature-context-vocabulary", `docs/invariants.md` (audit), and the doctor rule sources (`POLICY-CATEGORY-SHADOWS-EFFECT-001`, `VOCAB-AUDIT-001`, `VOCAB-CONTEXT-CTXMD-001`, `VOCAB-CONTEXT-NONGOALS-001`, `VOCAB-TESTS-MISSING-001`).
