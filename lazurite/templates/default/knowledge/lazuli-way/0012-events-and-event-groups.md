---
title:   "Events and event groups"
slug:    events-and-event-groups
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, events, event-group, cross-feature, payload]
read_when: "emitting or reacting to events"
---

# Events and event groups

An event tells the system something happened without knowing who reacts. Events are **external contracts**: a `payload` is a promise, load-bearing once another feature reads it. So payloads stay explicit and visible — Lazuli never injects a `<feature>_id`, never infers `emits` from a write. The explicit `emits` line *is* the edge in the reaction graph.

## Three constructs, one publish mechanism

- **`event <name>`** — domain event other features may react to. Declares a `payload` (typed fields). The normal kind.
- **`event.trace <name>`** — a signal *intentionally outside* the reaction graph: webhook-receipt markers, integration logs, side-effect audit pings. Subscribing (`trigger event <trace-event>`) is invalid in strict mode. Keeps a signal observable without it looking like missing product behavior.
- **`emits <name>`** — the *reference* that publishes an event; a child of a `command`, lifecycle transition, `job`, `webhook`, or `notification`. Works identically for `event` and `event.trace` — the kind only changes subscriber warnings and the reaction graph, not how you publish.

Join rule: `event.trace` is **dotted** because `trace` is a *variant of* `event` (like `query.list` of `query`); `event_group` is **underscored** — one atomic concept, not a variant.

## `event_group <prefix>_* on <Resource>` — shared payload envelope

When many events for one resource share envelope fields (resource id, tenant id, actor id), repeating them per-event is noise and drift risk. `event_group` declares the shared `payload` once; nested `event`/`event.trace` children inherit it and add their own fields. Pattern is a single trailing wildcard (`invoice_*`); a nested short name is *appended to the prefix* — `event created` under `event_group invoice_*` declares `invoice_created`.

```lazuli
feature invoice
  purpose "Invoices within an org."

  defaults
    tenancy org
    timestamps

  uses org, user

  domain
    enum InvoiceStatus
      open
      paid
      void

    resource Invoice
      number: Text required
      status: InvoiceStatus = open
      owner: User required

    event_group invoice_* on Invoice
      payload
        invoice_id = id
        org_id = org.id
        by_id == ctx.user.id when @actor.user

      event created
        number: Text

      event status_changed
        from: InvoiceStatus
        to: InvoiceStatus

    event.trace pdf_rendered
      invoice_id: ID
```

`lazuli inspect invoice.lzi --expand=events` shows the merged contract: `invoice_created` carries `invoice_id, org_id, by_id, number`; `invoice_status_changed` carries `invoice_id, org_id, by_id, from, to`; the `pdf_rendered` trace stays standalone with just `invoice_id`.

Two payload-block subtleties:

- Plain envelope mappings use **`=`** (assignment): `invoice_id = id`, `org_id = org.id` — project a resource field into the payload.
- A *conditional* mapping carries a `when` guard, making its left side a predicate, so it uses **`==`**: `by_id == ctx.user.id when @actor.user`. Bare `=` there is a hard error (`PREDICATE-EQ-OPERATOR-001`). Same `=` vs `==` split as all of Lazuli — see [the-three-operators](0003-the-three-operators.md).

Payload expressions resolve **against the named resource**, so `org.id` is valid only because `tenancy org` injected an `org` relation onto `Invoice`. Inheritance is **mandatory** for same-feature events matching the pattern (no opt-out — optional inheritance would make the shared block unreliable for readers and generators). For an event that should not carry the envelope, name it outside the pattern.

## `emits` publishes; the event must already exist

`emits` is always authored explicitly — never inferred from a `creates` or transition. `from creates` binds the new record, so projections like `invoice_id = id` read from the just-written row.

```lazuli
feature invoice
  purpose "Invoices within an org."

  defaults
    tenancy org
    timestamps

  uses org, user

  domain
    resource Invoice
      number: Text required
      owner: User required

    event_group invoice_* on Invoice
      payload
        invoice_id = id
        org_id = org.id

      event created
        number: Text

  policies
    author: @role.admin

  command create
    input
      number: Text required
    policy @policy.author
    rate_limit "30 per hour per user"
    creates Invoice
      number = input.number
      owner = ctx.user
    emits invoice_created from creates
```

## `emits` vs command `returns` — side effect vs response data

Not interchangeable; conflating them is a classic gaffe.

- **`emits`** — a *domain side effect* feeding the async reaction graph (audit, notifications, downstream features). The emitter does not know or wait for consumers.
- **`returns`** — *immediate response data* handed to this command's caller (auth session, generated URL, preview, import summary).

A request/response command may `returns` with **no** write effect:

```lazuli
  command preview
    input
      number: Text required
    policy @policy.author
    rate_limit "60 per minute per user"
    returns InvoicePreview
```

Never use `returns` to broadcast a domain event; never expect `emits` to return a value. See [command-and-query-anatomy](0007-command-and-query-anatomy.md).

## Cross-feature reaction — `uses` + a qualified event ref

A consumer reacts by listing the producer in `uses` and binding a `job` (or `notification`) to a **feature-qualified** event: `trigger event invoice.invoice_created`. Reacting never moves ownership — the producer owns the contract, the consumer owns its reaction.

```lazuli
feature invoice_audit
  purpose "Persist invoice events for compliance."

  defaults
    policy_for jobs, webhooks: @actor.system

  uses org, invoice

  domain
    resource AuditEntry
      tenancy org
      subject_id: ID required

      timestamps

  policies
    author: @actor.system
    view: @role.admin
    edit: @scope.none
    remove: @scope.none

  job record_invoice_created
    trigger event invoice.invoice_created
    idempotency by envelope.id
    creates AuditEntry
      subject_id = payload.invoice_id
```

Event-triggered jobs split bindings deliberately: bus metadata in `envelope.*` (`idempotency by envelope.id`), authored event fields in `payload.*`. Policy categories are `author` / `view` / `edit` / `remove`, never the CRUD effect names the doctor rejects.

## The payload boundary: read only what the producer declared

The rule that keeps events a real contract: **a consumer may read only the `payload.*` fields the producer event actually declared.** The job above reads `payload.invoice_id` because `event_group invoice_*` puts `invoice_id` on every `invoice_*` event — but it cannot reach into `Invoice`'s columns or invent `payload.total`. When producer and consumer share a package, the analyzer validates this cross-feature. Widen the contract by adding a field to the producer's payload, not by guessing in the consumer.

Hence payloads must be explicit and inheritance mandatory: the producer refactors storage freely as long as the declared payload holds, and consumers stay insulated from internals they were never promised. Use `lazuli inspect --expand=security` to see `@pii.*` / `@cap.*` markers on event payloads, so consumers can be audited without opening handler code.

When unsure what an event carries, ask the compiler — see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).

Authoritative spec: `docs/canonical-semantics.md` (Events), `docs/grammar.lzi.md` §11.
