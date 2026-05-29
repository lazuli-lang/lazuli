---
title:   "The five escape hatches"
slug:    five-escape-hatches
sector:  lazuli-way
tier:    approved
created: 2026-05-29
updated: 2026-05-29
cites:
  - customer.Customer
  - customer.list
tags: [doctrine, escape-hatch, scope-discipline]
---

# The five escape hatches

The framework models the generic 80% declaratively. The remaining 20% is
app-specific, and exactly **five** named escape hatches cover it. There is no
sixth coming — if a need can't be expressed via these five, the grammar must
grow (through a proposal), not the app.

1. **`@fn.<name>` handlers** — `command`/`query`/`job`/`webhook`/`poller`
   surfaces lower a `@fn.<name>` reference to authored Go. Use for: a vendor
   API call, custom validation, a derived-field computation, multi-step
   orchestration that doesn't fit the typed effect catalog.
2. **`api foo / handler "./path.go"`** — full-control HTTP endpoints that
   don't fit the typed `command`/`query` shape (e.g. a vendor webhook with
   nested polymorphic envelopes). Use when the response body is genuinely
   opaque or vendor-quirky.
3. **`query.sql @file`** — raw SQL for aggregations, joins, window functions,
   or recursive CTEs the typed `query.list`/`query.lookup` don't cover.
4. **`extends @anchor.<name>`** — view extensibility: a sibling feature adds a
   cell, drawer, column, or panel under a slot the source feature exposed.
5. **User-owned `main.go`** — the generated `main.go` is replaceable for
   runtime-topology decisions (which jobs register per worker, middleware
   ordering, integration-test hooks).

In this example, `customer.Customer` reads come through the typed `customer.list`
query rather than a hand-rolled endpoint — escape hatch #2 is reserved for the
shapes the typed query genuinely cannot express.

Authoritative spec: `docs/scope-discipline.md` §"The five escape hatches".
