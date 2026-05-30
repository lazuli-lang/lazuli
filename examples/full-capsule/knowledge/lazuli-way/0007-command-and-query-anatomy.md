---
title:   "Command and query anatomy"
slug:    command-and-query-anatomy
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, command, query, structure]
read_when: "writing a command or a query"
---

# Command and query anatomy

Lazuli keeps these distinctions sharp; blurring them is the usual structural gaffe.

## Commands declare exactly one effect

Exactly one of `creates` / `updates` / `deletes` per command — never two, never zero. A side-effect-only command is a `job` or `@fn`. The analyzer is intended to reject multiple effects; hold to one even where `lazuli check` doesn't yet hard-enforce it. Effect-block fields use `=` ([the-three-operators](0003-the-three-operators.md)).

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

## `route` vs `input` — never conflate

- **`route`** = routing/context values bound *by name* from the request route (`route id: ID` → `route.id`). Surfaces supply them from route context; the author never passes them manually.
- **`input`** = *submitted body* values the caller sends explicitly.

Every `route.*` reference needs a declared `route` slot. A surface calling a command passes **only** `input`, never route values. `lazuli check` catches mixing them.

## Policy is always explicit

Every command and every `scope override` query declares `policy @policy.*` (or a `@role.*` / `@scope.*` / `@actor.*` atom). Effect-derived policy is a generator *suggestion*, never an invisible default — unwritten means absent.

## `returns` ≠ `emits`

`returns <Type>` gives the immediate caller response data. It is **not** a substitute for `emits`: domain events other features react to flow through `emits`/event groups, not return values.

## Query modes are explicit: `list` / `lookup` / `sql`

Declare the mode up front; the compiler won't infer it from body shape.

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

- `query.lookup` children are only `policy`, `params`, `filters`, `gate behind ...`. No `key` child (retired — use `filters` with `==`).
- A `scope override` query **must** carry explicit `policy`: the override replaces the inherited tenant/soft-delete safety scope.

When unsure, `lazuli inspect <feature> --expand=all` shows exactly what the compiler derived — see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).

Authoritative spec: `docs/canonical-semantics.md`, `docs/quickref.md`.
