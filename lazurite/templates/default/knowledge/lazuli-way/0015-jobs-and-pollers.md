---
title:   "Jobs and pollers: async work the framework owns"
slug:    jobs-and-pollers
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, job, poller, async, background, idempotency]
read_when: "writing a job or a poller (async work)"
---

# Jobs and pollers: async work the framework owns

Async effects use two constructs: `job` and `poller`. Both keep the *contract* in `.lzi` (trigger, tenancy, idempotency, retry, effects, events); the *logic* lives in authored Go via `@fn`/`handler`. Inbound HTTP from a third party is instead a `webhook` — see [justified-opt-outs](0005-justified-opt-outs.md).

## `job` — triggered or scheduled background work

Fired one of two ways, declared by `trigger`:

- **`trigger event <event>`** — runs on a domain event. Reads `payload.*` (authored event fields) and `envelope.*` (bus metadata, e.g. `envelope.id`). Cross-feature producer: qualify it (`trigger event customer.invoice_paid`); the feature must be in `uses`.
- **`trigger schedule "<cron>"`** — cron-fired. Reads `schedule.*` (e.g. `schedule.day`) and, for per-tenant fan-out, `tenant.*`.

Event-triggered jobs may carry a small declarative body — `target`, `let`, `updates`/`creates`/`deletes`, `emits` — like a command, plus async plumbing:

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

- `tenant_from payload.<axis>_id` resolves the tenant from the payload. Required for an event-triggered job in a tenant-scoped feature — the bus does not carry your tenant scope.
- `idempotency by envelope.id` dedupes on that key so redelivery never double-applies.
- `@fn.risk_score` is the escape hatch; its Go lives in `domain/` (declared under `extensions`), filename-match strict (below).

Scheduled jobs typically delegate the whole body to a handler. `fanout tenants <axis>` runs once per tenant; `retry <n> backoff <exponential|linear|constant>` handles failure:

```lazuli
  job recompute_scores
    trigger schedule "0 2 * * *"
    fanout tenants org
    idempotency by tenant.org_id, schedule.day
    retry 3 backoff exponential
    handler "./jobs/recompute_scores.go"
```

`handler "./jobs/<name>.go"` is the all-in-one escape hatch: logic is authored Go, not a declarative body. A scheduled job in a tenant-scoped feature should declare `fanout tenants <axis>` (or explicit global scope + reason) so it never silently runs untenanted.

To call a provider-neutral integration op, use `calls <slot>.<operation>` with an argument block, `queue` to route, `timeout` to bound. The slot comes from `requires integration <slot>: <Capability>`, bound to a concrete adapter by the app — the job never names a vendor:

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

Job children: `trigger`, `tenant_from`, `fanout`, `queue`, `idempotency`, `retry`, `timeout`, `target`, `let`, `requires`, `creates`/`updates`/`deletes`, `calls`, `emits`, `handler`. Policy comes from feature `defaults policy_for jobs, webhooks: @actor.system` — jobs are not user-facing, so they take a default actor, not a per-job `policy @policy.*`.

## `poller` — drive a cursor row through a closed lifecycle

A `poller` keeps ticking external work until each row of a cursor table resolves (the V8-bureau / async-consult pattern, declarative): the dispatcher selects eligible rows on a timer, calls a `@fn` per row, and advances each row through a closed state set to a terminal one. Unlike a `job`, the body is a **closed catalog** — no free-form `updates`/`emits`; every child is fixed so the analyzer sees a fully-typed lifecycle.

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

- **`source <Resource>`** (required) — the cursor table walked; its fields carry lifecycle state inline.
- **`cursor`** (both required) — `eligible_when <next_at_field>, <resolved_at_field>` (row is due when `next_at` arrived and `resolved_at` still null); `attempts <field>` (per-row retry counter).
- **`retry`** — `max_attempts <int>` (bounds the poll loop) and `backoff <fixed|linear|exponential>` with optional `base <duration>` / `cap <duration>`.
- **`states`** — ≥2 entries, each `<name> [initial|intermediate|terminal]`; needs one initial and ≥1 terminal; a terminal state has no outgoing transition.
- **`resolve via @fn.<name>`** — per-row work handler (authored Go).
- **`terminal_status_field` / `terminal_result_field`** — columns the dispatcher writes when a row lands terminal.
- **`tick every <duration> [batch <int>]`** — scheduler interval and rows-per-tick.
- **`tenant_from row.<axis>_id`** — note `row.*`, not `payload.*`: the cursor row *is* the producer.
- **`idempotency by row.<field>[, row.<field>]*`** — dedupe key(s), `row.`-rooted.
- **`emits <event>`** — fired when a row resolves (zero-or-many).

The closed shape lets doctor prove the lifecycle sound — terminal-state-with-outgoing-edge, missing terminal, unbounded `max_attempts`, cursor with no resolved field are each a `POLLER-*` diagnostic — giving a provably-terminating loop instead of a hand-rolled goroutine. Optional `retry_quirk <kind>` (with `when` / `counter` / `mutate row.<field> = <transform>`) captures a per-provider retry oddity without leaking it into the core lifecycle.

## The filename-match rule (both constructs)

Every `@fn`/`handler` reference resolves to an authored file by strict naming the doctor enforces — wrong and `HANDLER-MISSING-001` fires:

- `@fn.poll_consult` → `handlers/poll_consult.go` (or `domain/poll_consult.go`) exporting `func PollConsult(...)`.
- `@fn.risk_score` → `domain/risk_score.go` with `func RiskScore(...)`.
- A job `handler "./jobs/<name>.go"` → that exact path.

Snake_case in `.lzi`, PascalCase Go export, matching basename. The `.lzi` is the *contract*; the Go file is the *wire* — [wire-not-reimplement](0001-wire-not-reimplement.md). To see the bindings/effects the compiler derived: `lazuli inspect <feature> --expand=all` ([the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md)).

Authoritative spec: `docs/grammar.lzi.md` (`job_block`, §"Poller"), `docs/keyword-reference.md` (Job + Poller sections), and the blessed `examples/full-capsule/full-capsule.lzi` (jobs) + `examples/production-grade/features/queries/queries.lzi` (poller).
