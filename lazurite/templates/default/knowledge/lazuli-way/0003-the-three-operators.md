---
title:   "The three operators: `:`, `=`, `==`"
slug:    the-three-operators
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, syntax, gaffe, operators]
read_when: "writing any assignment, declaration, or filter (: vs = vs ==)"
---

# The three operators: `:`, `=`, `==`

The top agent gaffe is the wrong operator. The correct one is fixed by the
**construct, not taste**. Wrong → parser rejects, or a sibling construct accepts
it and the meaning silently shifts.

> **Declare with `:`. Mutate with `=`. Compare with `==`.**

## `:` — declarations (name → type / labelled value)

- **Field / input / param** — `name: Text required`, `amount: Money`.
- **Named call arguments** — `target customer.query.by_id(id: input.customer_id)`.
- **`query.lookup` single-key** — `query.lookup by_id by id: ID`.
- **`non_goals` / `policies fields`** — `invoice: "invoicing"` (under a
  `delegated_to` block — see [retired-forms](0004-retired-forms-and-replacements.md)).

```lazuli
feature billing
  domain
    resource Invoice
      number: Text required
      amount: Money required
      issued_at: DateTime required
    query.lookup by_id by id: ID
```

## `=` — command effect assignments

In a command effect block (`creates` / `updates`), `=` (never `:`) writes a
*value* into a *field of the resource being mutated*.

```lazuli
  command issue_invoice
    input
      number: Text required
    policy @policy.author
    rate_limit "60 per minute per user"
    creates Invoice
      number = input.number
      amount = input.amount
      issued_at = ctx.now
```

`creates Invoice from input` is sugar when every input slot maps 1:1 onto a
field; mix `from input` with explicit `field = expr` for the rest. `number:
input.number` here is the classic gaffe — parser errors *"command effect
assignments use `<field> = <expr>`"*.

## `==` — predicate equality (filters, policy predicates)

In `filters` / policy predicates, `==` is an equality *test* scoping a query — a
comparison, not an assignment.

```lazuli
    query.list invoices_for_customer
      params
        customer_id: ID
      filters
        customer.id == params.customer_id
```

Single `=` (`filters customer.id = params.customer_id`) is rejected — filters
predicate, they do not assign.

## When unsure

Don't guess — `lazuli check .` answers immediately, and an existing feature in
`app/features/` almost always shows the idiom. See
[the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).

Authoritative spec: `docs/grammar.lzi.md`, `docs/quickref.md`,
`docs/canonical-semantics.md`.
