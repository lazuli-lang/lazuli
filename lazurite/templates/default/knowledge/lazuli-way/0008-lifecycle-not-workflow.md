---
title:   "Lifecycle, not workflow"
slug:    lifecycle-not-workflow
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, lifecycle, state-machine, transitions, retired]
---

# Lifecycle, not workflow

State machines are where cold-reading agents make their single biggest gaffe in
Lazuli, because the *obvious* shape is the one that was deleted. If your fingers
want to type `workflow <name> on Resource.field`, stop: that feature-level block
is **hard-retired**. The parser rejects it outright — there is no `workflow`
feature kind anymore. (`docs/canonical-semantics.md` still teaches the old form
in places; it is stale. Trust the parser, not that page — see
[the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).)
`workflow` is on the retired table in
[retired-forms-and-replacements](0004-retired-forms-and-replacements.md).

A state machine is now expressed in **two halves** that live in different places
and answer different questions:

1. The **`lifecycle <field>`** block, a child of a `resource`, declares the legal
   states and the edges between them — the *shape* of the machine.
2. A command's **`triggers transition <name>`** clause binds a write to one of
   those edges — the *how you move*.

This split is deliberate. The lifecycle owns the state graph and its authority
rules in one auditable place; commands stay plain writes that happen to advance a
known edge. Nothing reaches into a resource and sets `status = "active"` by hand.

## The lifecycle block

Declare it inside the resource. The discriminator field name follows
`lifecycle` (`lifecycle status` → a `status` field). Exactly one state is
`initial`; terminal states are marked `terminal`. Each `transition` names its
`from` and `to` states, carries a `policy`, and `emits` domain events.

```lazuli
    resource Subscription
      plan: Text required

      lifecycle status
        state trialing initial
        state active
        state paused
        state cancelled terminal

        transition activate
          from trialing
          to active
          policy @policy.edit
          emits subscription_activated, subscription_status_changed
          tests
            allows from trialing
            denies from active
            denies from cancelled

        transition cancel
          from active
          to cancelled
          policy @policy.edit
          requires @policy.remove
          emits subscription_cancelled, subscription_status_changed
```

The `lifecycle` block **owns** the discriminator field *and* its enum. Do **not**
also write `status: SubscriptionStatus = trialing` in the fields, and do **not**
declare a sibling `enum SubscriptionStatus` whose values match the states —
doctor fires `LIFECYCLE-FIELD-DOUBLE-DECLARED` and `LIFECYCLE-ENUM-DUPLICATE`.
The state names *are* the enum. Other features still reference it by the auto-
emitted name (`SubscriptionStatus.active`) in queries and rules.

`requires @policy.remove` on a transition is the capability-upgrade pattern: the
transition keeps its normal `policy @policy.edit` for visibility while declaring
that *this particular* edge needs a stronger feature-local category. Use it for
archive / cancel / force-style edges; do not reach for transition-specific
`policy` to express it. `policy` and `requires` here use the canonical policy
categories — `author / view / edit / remove`, never CRUD verbs.

The `tests` block is an inline truth table for the edge: `allows from <state>` /
`denies from <state>`, and `allows`/`denies as @role.<r>` for the actor axis.
`--strict-tests` (and `tdd-iron-hand`) warn on a guarded transition with no
tests.

## Binding a write to an edge: `triggers transition`

The lifecycle declares the edge; a command *takes* it. The command owns the
write and the policy; `triggers transition <name>` tells the backend to gate on
`Subscription.status == <from>`, perform the update, and advance `status` to
`<to>` in one transaction. You never assign the discriminator yourself.

```lazuli
  command cancel
    route id: ID
    input
      reason: Text required
    policy @policy.edit
    rate_limit "10 per minute per user"
    audit default
    triggers transition cancel
    updates Subscription
      plan = "cancelled"
    emits subscription_cancelled
    invalidates
      query.list
      query.by_id(id: route.id)
```

Note the command's `updates` block does *not* set `status` — the transition
does. The command may still write other fields. A single command binds one (or
several, comma-separated) transitions; the named transition must exist on the
resource the command writes. Command anatomy itself (route vs input, the single
effect rule, explicit policy) is covered in
[command-and-query-anatomy](0007-command-and-query-anatomy.md).

## Guarding surfaces by state

Surfaces read the machine through **view guards**, so a screen only renders when
the entity is in an acceptable state. Two forms, both grounded in the blessed
example:

```lazuli
route admin_subscription_detail
  path "/admin/subscriptions/:id"
  route id: Subscription.ID
  to subscription.view.detail(id: route.id)
  surface subscription web
  audience admin
  policy @policy.edit
    requires_lifecycle_in Subscription [active, paused]

experience subscription
  imports subscription

  view detail
    route id: Subscription.ID
    policy @policy.edit
    requires_lifecycle Subscription = active
    source subscription.query.by_id(id: route.id)
    action cancel -> subscription.command.cancel(id: route.id)
```

- `requires_lifecycle <Resource> = <state>` — exact-match gate (note the bare
  `=`; it *binds* a state, it does not compare — see
  [the-three-operators](0003-the-three-operators.md)).
- `requires_lifecycle_in <Resource> [s1, s2]` — allow-list gate, the canonical
  set form. On a `route` it nests under the `policy` guard, as above.

When the guard is unmet you can route the user back into the machine with
`on_lifecycle_pending @resume <router>` (the resource's `@resume` lifecycle
router). Doctor enforces these against the declared states via
`ROUTE-GUARD-LIFECYCLE-*` — a typo'd or undeclared state is caught, not silently
ignored.

## The mental model

`lifecycle` answers *"what states exist and which edges are legal?"`.
`triggers transition` answers *"which edge does this write take?"`.
`requires_lifecycle` answers *"may this surface render in the entity's current
state?"`. Keep those three questions in three places and the machine stays
honest. Confirm what the compiler derived — reachable states, edge policies,
generated transition tests — with `lazuli inspect <feature> --expand=all`.

Authoritative spec: `docs/grammar.lzi.md` (§11, workflow retirement),
`docs/keyword-reference.md` (`lifecycle` / `transition` / `triggers` /
`requires_lifecycle*` rows), and the blessed `examples/full-capsule/`.
