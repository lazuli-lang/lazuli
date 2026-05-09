# Lazuli Language Backlog

This document tracks design pressure that is not yet part of the core canonical syntax.

## Closed v0 Decisions

- Canonical `.lzi` is the official authoring format; no compact/expanded dual syntax.
- A feature is a product capability, not an entity bucket. Split auth/import/tags/etc. into separate features even when they all touch the same resource.
- Local references omit the feature prefix; cross-feature references must be feature-qualified and backed by `uses`.
- `uses` is strict; do not list conceptual dependencies that are not referenced semantically.
- `params` and `input` stay separate in commands.
- Commands are not route-owned; surfaces pass route data into command params.
- Commands declare exactly one explicit effect: `creates`, `updates`, or `deletes`.
- Command policy may be omitted when it derives cleanly from that effect.
- Commands may `returns` typed data when the immediate caller needs response data, not as a substitute for events.
- `policies` is a named dictionary; entries use `name: predicate, predicate`.
- `field_policies` and `non_goals` use the same `name: value` punctuation.
- The common `command <name> on <Resource>` target shorthand expands to `params id` plus `target <resource> = query.by_id(id: params.id)`.
- Feature-level `defaults` can provide repeated `tenancy`, `timestamps`, and async/system `policy` defaults.
- `scope` is reserved for safety boundaries; ordinary caller predicates belong in `filters`.
- Read views use `source query.*`; write forms use `submit command.*`.
- Cross-feature view composition uses `extends <feature>.surface.<target>.<area>.view.<name>`.
- `tenancy` and `soft_delete` inject default declarative query scope.
- Raw queries must declare their safety scope explicitly.
- Webhooks run server extensions after verification; no magical declarative webhook upsert in v0.
- Job/webhook handlers are declared inline with `handler`; event consumers are named `job`s with `trigger event`.
- Single-use view blocks and resource validators may declare their implementation inline instead of creating an `extensions` index entry.
- Workflow-level `policy` is a default; transition-level `policy` overrides it.
- Workflow-level `emits` always fires; transition-level `emits` fires additionally.
- Event payloads are explicit; shared repeated envelope fields use resource-level `event_payload <event-pattern>`.
- `context` is only an override for the co-located `<feature>.ctx.md` convention.
- `on_delete` governs hard delete only; soft-delete cascades must be explicit behavior.

## Missing Constructs Under Pressure Test

### Auth

`examples/user-auth.lzi` exercises password login, OAuth, MFA, sessions, refresh tokens, rate limiting, and account recovery. Likely outcome: adapters plus server extensions, not a fully declarative auth language.

### Inbound Webhooks

`examples/billing.lzi` exercises external calls without `ctx.user`: verified inbound payloads, retries, idempotency keys, and event emission.

Candidate shape to test later:

```lazuli
webhook stripe_payment
  path "/webhooks/stripe"
  verify "./integrations/stripe.go"
  handler "./integrations/record_payment.go" returns Payment
  emits payment_received
```

### Async Jobs

`examples/notification.lzi`, `examples/billing.lzi`, and `examples/import-csv.lzi` exercise cron/schedule/queue semantics and progress/error reporting.

Candidate shape to test later:

```lazuli
job recompute_scores
  trigger schedule "0 2 * * *"
  handler "./jobs/recompute_scores.go"

job send_archive_email
  trigger event customer.customer_archived
  idempotency event.id
  retry 3 backoff exponential
  handler "./outreach/send_archive_email.go"
```

### Rich Relations

`examples/comment.lzi` and `examples/org-team.lzi` exercise explicit resources for relations that need payload or lifecycle.

### Cascading Soft Delete

Still open. Need a decision for whether related soft-deleted parents automatically hide children, or whether every query must model that relationship explicitly.

### Multi-Surface

`examples/org-team.lzi` and `examples/user-auth.lzi` start this pressure test with `surface web admin`, `surface web public`, and `surface mobile member`.

## Foundation Work

- Expand `canonical-semantics.md` into a proper language spec with syntax tables and generated IR effects.
- Validate `error-contract.md` against Go/React adapters.
- Validate `migrations.md` against semantic diffs from fixture changes.
- Validate `project-structure.md` against extension/raw/escape conventions.
- Validate `testing-strategy.md` against generated invariant tests.
