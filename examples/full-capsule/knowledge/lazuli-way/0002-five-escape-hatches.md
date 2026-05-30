---
title:   "The five escape hatches"
slug:    five-escape-hatches
sector:  lazuli-way
tier:    approved
created: 2026-05-29
updated: 2026-05-29
tags: [doctrine, escape-hatch, scope-discipline]
read_when: "a typed surface cannot express something — before any workaround"
---

# The five escape hatches

The framework models the generic 80% declaratively. The app-specific 20% has exactly **five** named escape hatches. No sixth is coming — if a need can't fit these five, the grammar grows (via proposal), not the app. Always reach for a typed surface first; reserve a hatch for shapes the typed surface genuinely cannot express (e.g. prefer `query.list`/`query.lookup` over a hand-rolled endpoint #2).

1. **`@fn.<name>` handlers** — `command`/`query`/`job`/`webhook`/`poller` surfaces lower a `@fn.<name>` ref to authored Go. For: vendor API call, custom validation, derived-field computation, or multi-step orchestration outside the typed effect catalog.
2. **`api foo / handler "./path.go"`** — full-control HTTP endpoint that doesn't fit the typed `command`/`query` shape. Use when the response body is genuinely opaque or vendor-quirky (e.g. webhook with nested polymorphic envelopes).
3. **`query.sql @file`** — raw SQL for aggregations, joins, window functions, or recursive CTEs that typed `query.list`/`query.lookup` don't cover.
4. **`extends @anchor.<name>`** — view extensibility: a sibling feature adds a cell, drawer, column, or panel under a slot the source feature exposed.
5. **User-owned `main.go`** — generated `main.go` is replaceable for runtime-topology decisions (per-worker job registration, middleware ordering, integration-test hooks).

Authoritative spec: `docs/scope-discipline.md` §"The five escape hatches".
