---
title:   "Jobs and pollers: async work the framework owns"
slug:    jobs-and-pollers
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, job, poller, async, background, idempotency]
---

# Jobs and pollers: async work the framework owns

Not every effect is a synchronous command. Some work fires off an event, runs on
a cron, or chases an external system until it settles. Lazuli has two declarative
constructs for this — `job` and `poller` — and both keep the *contract* in `.lzi`
(trigger, tenancy, idempotency, retry, effects, events) while the *logic* lives in
authored Go reached through `@fn`/`handler`. Inbound HTTP from a third party is a
different shape: that is a `webhook`, covered by
[justified-opt-outs](0005-justified-opt-outs.md).

## `job` — triggered or scheduled background work

A job is fired one of two ways, declared by `trigger`:

- **`trigger event <event>`** — runs when a domain event lands. The job reads
  `payload.*` (authored event fields) and `envelope.*` (bus metadata like
  `envelope.id`). For a cross-feature producer, qualify it: `trigger event
  customer.invoice_paid` (the feature must be in `uses`).
- **`trigger schedule "<cron>"`** — runs on a cron expression. The job reads
  `schedule.*` (e.g. `schedule.day`) and, for per-tenant fan-out, `tenant.*`.

Event-triggered jobs may carry a small declarative body — `target`, `let`,
`updates`/`creates`/`deletes`, `emits` — exactly like a command, plus async
plumbing. The job below recomputes a score when an invoice is paid:

```lazuli
  job recompute_score_after_invoice
    trigger event customer.invoice_paid
    tenant_from payload.org_id
    idempotency by envelope.id
    target query.by_id(id: payload.invoice_id)
    let new_score = @fn.risk_score(target)
    updates Invoice
      score = new_score
    emits invoice_score_recomputed
      score = new_score
```

`tenant_from payload.<axis>_id` resolves the tenant from the event payload — an
event-triggered job in a tenant-scoped feature needs it, because the bus does not
carry your tenant scope for free. `idempotency by envelope.id` makes redelivery
safe: the framework dedupes on that key so a job never double-applies. The
`@fn.risk_score` reference is the escape hatch — its Go lives in the feature's
`domain/` (declared under `extensions`), and the filename-match rule is strict
(see below).

Scheduled jobs typically delegate the whole body to a handler. Use `fanout
tenants <axis>` to run the job once per tenant, and `retry <n> backoff
<exponential|linear|constant>` for failure handling:

```lazuli
  job recompute_scores
    trigger schedule "0 2 * * *"
    fanout tenants org
    idempotency by tenant.org_id, schedule.day
    retry 3 backoff exponential
    handler "./jobs/recompute_scores.go"
```

`handler "./jobs/<name>.go"` is the all-in-one escape hatch: the job's logic is
authored Go, not a declarative body. A scheduled job in a tenant-scoped feature
should declare `fanout tenants <axis>` (or an explicit global scope with a
reason) so it does not silently run untenanted.

When a job calls a provider-neutral integration operation, use `calls
<slot>.<operation>` with an argument block, plus `queue` to route it and
`timeout` to bound it. The slot comes from `requires integration <slot>:
<Capability>` and is bound to a concrete adapter by the app — the job never names
a vendor:

```lazuli
  job process_import
    trigger event import_uploaded
    queue customer_imports
    tenant_from payload.org_id
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
      org_id = payload.org_id
    timeout "30s"
    handler "./jobs/process_import.go"
    emits import_completed
```

Job children: `trigger`, `tenant_from`, `fanout`, `queue`, `idempotency`,
`retry`, `timeout`, `target`, `let`, `requires`, `creates`/`updates`/`deletes`,
`calls`, `emits`, `handler`. Policy comes from the feature `defaults policy_for
jobs, webhooks: @actor.system` — jobs are not user-facing, so they take a default
actor rather than a per-job `policy @policy.*`.

## `poller` — drive a cursor row through a closed lifecycle

A `poller` is the contract for "keep ticking external work until each row of a
cursor table resolves." It is the V8-bureau / async-consult pattern made
declarative: the dispatcher selects eligible rows on a timer, calls a `@fn` per
row, and advances each row through a closed set of states until it reaches a
terminal one. Unlike a `job`, the `poller` body is a **closed catalog** — no
declarative `updates`/`emits` free-for-all; every child is fixed so the analyzer
sees a fully-typed lifecycle.

```lazuli
  poller resolve_pending_consult
    source PendingConsult
    cursor
      eligible_when next_check_at, resolved_at
      attempts attempts
    retry
      max_attempts 30
      backoff exponential base 30s cap 10m
    states
      pending initial
      resolved terminal
      failed terminal
    resolve via @fn.poll_consult
    terminal_status_field final_status
    tick every 15s batch 100
    tenant_from row.org_id
    idempotency by row.id, row.attempts
    audit default
    emits consult_resolved
```

Reading it top to bottom:

- **`source <Resource>`** (required) — the cursor table the poller walks. Its
  fields carry the lifecycle state inline.
- **`cursor`** — `eligible_when <next_at_field>, <resolved_at_field>` (a row is
  due when `next_at` has arrived and `resolved_at` is still null) plus `attempts
  <field>` (the per-row retry counter). Both are required.
- **`retry`** — `max_attempts <int>` (the bound that prevents an unbounded poll
  loop) and `backoff <fixed|linear|exponential>` with optional `base
  <duration>` / `cap <duration>`.
- **`states`** — at least two entries, each `<name> [initial|intermediate|
  terminal]`. There must be an initial state and at least one terminal state; a
  terminal state has no outgoing transition.
- **`resolve via @fn.<name>`** — the per-row work handler (authored Go).
- **`terminal_status_field` / `terminal_result_field`** — the columns the
  dispatcher writes when a row lands terminal.
- **`tick every <duration> [batch <int>]`** — the scheduler interval and the
  rows-per-tick batch size.
- **`tenant_from row.<axis>_id`** — note `row.*`, not `payload.*`: the cursor row
  *is* the producer here.
- **`idempotency by row.<field>[, row.<field>]*`** — dedupe key(s), `row.`-rooted.
- **`emits <event>`** — fired when a row resolves (zero-or-many).

The closed shape exists so doctor can prove the lifecycle is sound: a terminal
state with an outgoing edge, a missing terminal, an unbounded `max_attempts`, a
cursor with no resolved field — each is its own `POLLER-*` diagnostic. You get a
provably-terminating poll loop instead of a hand-rolled goroutine. An optional
`retry_quirk <kind>` block (with `when` / `counter` / `mutate row.<field> =
<transform>`) captures a per-provider retry oddity without leaking it into the
core lifecycle.

## The filename-match rule (both constructs)

Every `@fn`/`handler` reference resolves to an authored file by a strict naming
convention the doctor enforces — get it wrong and `HANDLER-MISSING-001` fires:

- `@fn.poll_consult` → `handlers/poll_consult.go` (or `domain/poll_consult.go`)
  exporting `func PollConsult(...)`.
- `@fn.risk_score` → `domain/risk_score.go` with `func RiskScore(...)`.
- A job `handler "./jobs/<name>.go"` → that exact path, owning the job's logic.

Snake_case in `.lzi`, PascalCase Go export, matching basename. The `.lzi` declares
the *contract*; the Go file is the *wire* —
[wire-not-reimplement](0001-wire-not-reimplement.md). When unsure which bindings
and effects the compiler derived for a job or poller, ask it:
`lazuli inspect <feature> --expand=all`
([the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md)).

Authoritative spec: `docs/grammar.lzi.md` (`job_block`, §"Poller"),
`docs/keyword-reference.md` (Job + Poller sections), and the blessed
`examples/full-capsule/full-capsule.lzi` (jobs) +
`examples/production-grade/features/queries/queries.lzi` (poller).
