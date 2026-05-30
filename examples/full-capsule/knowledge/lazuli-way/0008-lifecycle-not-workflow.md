---
title:   "Lifecycle, not workflow"
slug:    lifecycle-not-workflow
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, lifecycle, state-machine, transitions, retired]
read_when: "modeling a state machine / status field (NEVER `workflow`)"
---

# Lifecycle, not workflow

State machines are the #1 cold-read gaffe: the obvious shape was deleted. Reaching
for `workflow <name> on Resource.field`? **Stop** — hard-retired, no `workflow`
feature kind, parser rejects it. (`docs/canonical-semantics.md` still teaches the
old form; it is stale — trust the parser, see
[the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).) `workflow` sits
on the retired table in
[retired-forms-and-replacements](0004-retired-forms-and-replacements.md).

A state machine is now **two halves** in two places:

1. **`lifecycle <field>`** (child of `resource`) — legal states + edges = the
   *shape*.
2. A command's **`triggers transition <name>`** — binds a write to one edge = *how
   you move*.

Deliberate split: the lifecycle owns the state graph + authority rules in one
auditable place; commands stay plain writes advancing a known edge. Nothing sets
`status = "active"` by hand.

## The lifecycle block

Declare inside the resource. The discriminator field name follows `lifecycle`
(`lifecycle status` → a `status` field). Exactly one state is `initial`; terminal
states are marked `terminal`. Each `transition` names `from`/`to`, carries a
`policy`, and `emits` events.

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

The block **owns** the discriminator field *and* its enum. Do **not** also write
`status: SubscriptionStatus = trialing` in the fields, nor declare a sibling
`enum SubscriptionStatus` matching the states — doctor fires
`LIFECYCLE-FIELD-DOUBLE-DECLARED` and `LIFECYCLE-ENUM-DUPLICATE`. The state names
*are* the enum; other features reference it by the auto-emitted name
(`SubscriptionStatus.active`) in queries and rules.

`requires @policy.remove` on a transition is the **capability-upgrade pattern**:
the edge keeps its normal `policy @policy.edit` for visibility while declaring
that *this* edge needs a stronger feature-local category. Use it for
archive/cancel/force edges; don't express it via a transition-specific `policy`.
`policy` and `requires` take canonical categories — `author / view / edit /
remove`, never CRUD verbs.

`tests` is an inline truth table: `allows from <state>` / `denies from <state>`
(state axis), `allows` / `denies as @role.<r>` (actor axis). `--strict-tests`
(and `tdd-iron-hand`) warn on a guarded transition with no tests.

## Binding a write: `triggers transition`

The lifecycle declares the edge; a command takes it. The command owns the write
and policy; `triggers transition <name>` gates on `Subscription.status == <from>`,
performs the update, then advances `status` to `<to>` in one transaction. You
never assign the discriminator yourself.

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

The `updates` block does *not* set `status` — the transition does; the command may
write other fields. One command binds one (or several, comma-separated)
transitions; each must exist on the resource the command writes. Command anatomy:
[command-and-query-anatomy](0007-command-and-query-anatomy.md).

## Guarding surfaces by state

Surfaces read the machine through **view guards** — a screen renders only when the
entity is in an acceptable state. Two forms, both from the blessed example:

```lzx
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

- `requires_lifecycle <Resource> = <state>` — exact-match gate. The bare `=`
  *binds* a state, it does not compare (see
  [the-three-operators](0003-the-three-operators.md)).
- `requires_lifecycle_in <Resource> [s1, s2]` — allow-list gate, the canonical set
  form. On a `route` it nests under the `policy` guard, as above.

Guard unmet? Route back with `on_lifecycle_pending @resume <router>` (the
resource's `@resume` lifecycle router). Doctor checks these against the declared
states via `ROUTE-GUARD-LIFECYCLE-*` — a typo'd or undeclared state is caught, not
ignored.

## Mental model

| Construct | Question it answers |
|---|---|
| `lifecycle` | what states exist and which edges are legal? |
| `triggers transition` | which edge does this write take? |
| `requires_lifecycle` | may this surface render in the entity's current state? |

Three questions, three places. Confirm what the compiler derived — reachable
states, edge policies, generated transition tests — with
`lazuli inspect <feature> --expand=all`.

Authoritative spec: `docs/grammar.lzi.md` (§11, workflow retirement),
`docs/keyword-reference.md` (`lifecycle` / `transition` / `triggers` /
`requires_lifecycle*` rows), and the blessed `examples/full-capsule/`.
