---
title:   "Events and event groups"
slug:    events-and-event-groups
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, events, event-group, cross-feature, payload]
---

# Events and event groups

Events are how one feature tells the rest of the system that something happened,
without knowing or caring who reacts. They are **external contracts**: a payload
is a promise, and the moment another feature reads it, that shape is load-bearing.
So Lazuli makes event payloads explicit and visible in source — it never silently
injects a `<feature>_id` into every event, and it never infers `emits` from a
write. The explicit `emits` line *is* the edge in the reaction graph.

## Three event kinds, one publish mechanism

There are two declaration kinds plus the construct that fires them:

- **`event <name>`** — a domain event other features may react to. It declares a
  `payload` (a list of typed fields). This is the normal kind.
- **`event.trace <name>`** — a domain signal that is *intentionally not* part of
  the feature-to-feature reaction graph: webhook-receipt markers, integration
  logs, side-effect audit pings. Other features should not subscribe to it; in
  strict mode `trigger event <trace-event>` is invalid. Use it to keep an
  observable signal visible without making it look like missing product behavior.
- **`emits <name>`** — the *reference* that publishes an event. It is a child of
  a `command`, lifecycle transition, `job`, `webhook`, or `notification`. `emits`
  works identically for `event` and `event.trace`; the kind only changes
  subscriber warnings and the reaction graph, not how you publish.

Note the join rule: `event.trace` is **dotted** because `trace` is a *variant of*
`event`, the same way `query.list` is a variant of `query`. `event_group` is
**underscored** because it is one atomic concept, not a variant of `event`.

## `event_group <prefix>_* on <Resource>` — shared payload envelope

When many events for one resource carry the same envelope fields (resource id,
tenant id, actor id), declaring them on every event is noise and a drift risk.
`event_group` declares the shared `payload` once; nested `event` / `event.trace`
children inherit it and add their own fields. The pattern is a single trailing
wildcard (`invoice_*`), and a nested short name is *appended to the prefix* — so
`event created` under `event_group invoice_*` declares `invoice_created`.

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

`lazuli inspect invoice.lzi --expand=events` shows the fully merged contract:
`invoice_created` carries `invoice_id, org_id, by_id, number`, and
`invoice_status_changed` carries `invoice_id, org_id, by_id, from, to`. The
`pdf_rendered` trace stays standalone with just `invoice_id`.

Two operator subtleties live in that payload block, both worth memorising:

- Plain envelope mappings use **`=`** (assignment): `invoice_id = id`,
  `org_id = org.id`. These project a resource field into the payload.
- A *conditional* mapping carries a `when` guard, which makes its left side a
  predicate — so it uses **`==`**: `by_id == ctx.user.id when @actor.user`. Bare
  `=` there is a hard error (`PREDICATE-EQ-OPERATOR-001`). This is the same
  `=` vs `==` split that governs all of Lazuli —
  see [the-three-operators](0003-the-three-operators.md).

Payload expressions resolve **against the named resource**, so `org.id` is valid
only because `tenancy org` injected an `org` relation onto `Invoice`. Inheritance
is *mandatory* for same-feature events matching the pattern: there is no
opt-out, because optional inheritance would make the shared block unreliable for
readers and generators. If an event should not carry the envelope, name it
outside the pattern.

## `emits` publishes; the event must already exist

`emits` is always authored explicitly — never inferred from a `creates` or a
transition. The `from creates` form binds the new record so envelope projections
like `invoice_id = id` read from the just-written row.

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

These are not interchangeable, and conflating them is a classic gaffe:

- **`emits`** is a *domain side effect*. It feeds the asynchronous reaction graph:
  audit logs, notifications, downstream features. The emitter does not know or
  wait for its consumers.
- **`returns`** is *immediate response data* handed back to the caller of this
  one command — an auth session, a generated URL, a preview, an import summary.

A request/response command may `returns` with **no** write effect:

```lazuli
  command preview
    input
      number: Text required
    policy @policy.author
    rate_limit "60 per minute per user"
    returns InvoicePreview
```

Never reach for `returns` to broadcast a domain event, and never expect `emits`
to give the caller a value back. Effect vs response is covered further in
[command-and-query-anatomy](0007-command-and-query-anatomy.md).

## Cross-feature reaction — `uses` + a qualified event ref

A consumer reacts to a producer's event by listing the producer in `uses` and
binding a `job` (or `notification`) to a **feature-qualified** event:
`trigger event invoice.invoice_created`. Reacting never moves ownership — the
producer owns the event contract, the consumer owns its reaction.

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

Event-triggered jobs split their bindings deliberately: bus metadata lives in
`envelope.*` (so `idempotency by envelope.id`) and the *authored* event fields
live in `payload.*`. Note the canonical policy categories here — `author` /
`view` / `edit` / `remove`, never the CRUD effect names the doctor rejects.

## The payload boundary: read only what the producer declared

This is the rule that keeps events a real contract: **a consumer may read only the
`payload.*` fields the producer event actually declared.** The `invoice_audit`
job above can read `payload.invoice_id` because `event_group invoice_*` puts
`invoice_id` on every `invoice_*` event — but it cannot reach into `Invoice`'s
columns or invent `payload.total`. When both producer and consumer are present in
the same package, the analyzer validates this across features; widen the contract
by adding a field to the producer's payload, not by guessing in the consumer.

This is exactly why payloads must be explicit and inheritance mandatory: the
producer can refactor its storage freely as long as the declared payload holds,
and every consumer stays insulated from internals it was never promised. Use
`lazuli inspect --expand=security` to see `@pii.*` / `@cap.*` markers on event
payloads so cross-feature consumers can be audited without opening handler code.

When you are unsure what an event actually carries, ask the compiler rather than
guessing — see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).

Authoritative spec: `docs/canonical-semantics.md` (Events), `docs/grammar.lzi.md`
§11.
