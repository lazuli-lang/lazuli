---
title:   "Command and query anatomy"
slug:    command-and-query-anatomy
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, command, query, structure]
---

# Command and query anatomy

Most structural gaffes come from blurring distinctions Lazuli keeps deliberately
sharp. Internalise these and the shapes write themselves.

## Commands declare exactly one effect

A command mutates state through **exactly one** of `creates`, `updates`, or
`deletes` — never two, never zero (a side-effect-only command is a `job` or a
`@fn`). This is canonical doctrine: the analyzer is *intended* to reject multiple
effects, and even where `lazuli check` does not yet hard-enforce it, keep to one
effect per command. Effect-block fields use `=`
([the-three-operators](0003-the-three-operators.md)).

```lazuli
  command update_status
    route id: ID
    input
      status: InvoiceStatus required
    policy @policy.edit
    rate_limit "120 per minute per user"
    updates Invoice
      status = input.status
    emits invoice_status_changed
```

## `route` vs `input` — never conflate them

- **`route`** slots are routing/context values bound *by name* from the request
  route (e.g. `route id: ID` → `route.id`). Surfaces supply them from route
  context; the author never passes them manually.
- **`input`** slots are *submitted body* values the caller sends explicitly.

Any `route.*` reference must be backed by a declared `route` slot, and a surface
that calls a command passes **only** `input`, never route values. Mixing them is
a common gaffe that `lazuli check` catches.

## Policy is always explicit

Every command and every `scope override` query declares `policy @policy.*` (or a
`@role.*` / `@scope.*` / `@actor.*` atom). Effect-derived policy is a *generator
suggestion*, never an invisible default — if you don't write it, it isn't there.

## `returns` is for response data, not events

A command may `returns <Type>` when the immediate caller needs response data
back. It is **not** a substitute for `emits` — domain events that other features
react to flow through `emits`/event groups, not through a return value.

## Query modes are explicit: `list`, `lookup`, `sql`

Pick the declaration mode up front; the compiler will not infer it from body
shape:

```lazuli
    # generated, analyzable — typed list with filters
    query.list open_invoices
      filters
        status == InvoiceStatus.open

    # generated single-key lookup (shorthand: `by <field>: <Type>`)
    query.lookup by_id by id: ID

    # an externally-implemented query — the escape hatch for joins / CTEs / windows.
    # query.sql needs `returns <Type>` + a `sql "./path.sql"` child (the raw SQL
    # lives in queries/<name>.sql; @file is exclusive to query.view's `source`).
    query.sql revenue_by_month
      returns RevenueByMonth[]
      sql "./queries/revenue_by_month.sql"
```

`query.lookup` children are only `policy`, `params`, `filters`, and `gate behind
...` — there is no `key` child (that was retired; use `filters` with `==`). A
`scope override` query **must** carry an explicit `policy`, because the override
replaces the inherited tenant/soft-delete safety scope.

When in doubt about any of these, `lazuli inspect <feature> --expand=all` shows
exactly what the compiler derived — see
[the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).

Authoritative spec: `docs/canonical-semantics.md`, `docs/quickref.md`.
