---
title:   "The three operators: `:`, `=`, `==`"
slug:    the-three-operators
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, syntax, gaffe, operators]
---

# The three operators: `:`, `=`, `==`

The single most common gaffe an agent makes writing Lazuli is reaching for the
wrong operator. Lazuli uses three, and **which one is correct is decided by the
construct, not by taste**. Get this wrong and the parser rejects the file — or
worse, a sibling construct accepts it and the meaning silently shifts.

## `:` — declarations (type annotations, named arguments, lookup keys)

Colon binds a *name* to a *type* or a *labelled value*. Use it for:

- **Field / input / param declarations** — `name: Text required`, `amount: Money`.
- **Named call arguments** — `target customer.query.by_id(id: input.customer_id)`.
- **`query.lookup` single-key declarations** — `query.lookup by_id by id: ID`.
- **`non_goals` / `policies fields` entries** — `invoice: "invoicing"` (under a
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

Inside a command's effect block (`creates` / `updates`), an assignment writes a
*value* into a *field of the resource being mutated*. This is `=`, never `:`.

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

`creates Invoice from input` is the sugar when every input slot maps 1:1 onto a
field; mix `from input` with explicit `field = expr` lines for the rest. Writing
`number: input.number` here is the classic gaffe — the parser errors with
*"command effect assignments use `<field> = <expr>`"*.

## `==` — predicate equality (filters, policy expressions)

In a `filters` block (and policy predicates), `==` is the *equality test* that
scopes a query. It is a comparison, not an assignment — so it doubles up.

```lazuli
    query.list invoices_for_customer
      params
        customer_id: ID
      filters
        customer.id == params.customer_id
```

`filters customer.id = params.customer_id` (single `=`) is rejected — filters
predicate, they do not assign.

## The one-line mnemonic

> **Declare with `:`. Mutate with `=`. Compare with `==`.**

When unsure which a construct wants, do not guess — `lazuli check .` tells you
immediately, and an existing feature in `app/features/` almost always shows the
idiom. See [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).

Authoritative spec: `docs/grammar.lzi.md`, `docs/quickref.md`,
`docs/canonical-semantics.md`.
