# PRD — 0013 Actor-Relative `query.compose`

> Status: ready · Created: 2026-05-31 · Track: evolve/ship

## Problem

`query.compose` (built 2026-05-30, `feat/query-compose`) closed roughly half of Hostpoint's read escape-hatches — but it stops at the door of the most common real read: **the actor-relative read**. Every "my reservations", "my conversations", "reviews I'm allowed to see", "agenda for the properties I host" is scoped to *who is asking*, and compose can't yet express that scope.

So those reads stay where they were: multi-JOIN SQL string literals buried in Go handlers, declared in the `.lzi` only as opaque `fn …: Function[…]`. They are **invisible to `inspect`, to exposure analysis, and to the LSP** — the HIGH-1 finding. The toolchain cannot see what tables they touch, what actor guard they enforce, or whether they leak rows across tenants/owners. A reviewer reading the `.lzi` sees three opaque function signatures and a comment that says "trust me, the SQL is in `handlers/`."

Evidence (Hostpoint, `C:\Users\lucas\hostpoint\app`):
- `features/trust/handlers/list_property_reviews.go:17-34` — multi-JOIN read whose `WHERE` carries an actor branch: `published = true OR $2 = (SELECT host_id FROM property …) OR $3 = true`. Declared only as `fn list_property_reviews: Function[…]` (`trust.lzi:276`).
- `features/operations/handlers/list_host_agenda.go:16-25` — ownership join `JOIN property p ON p.id = res.property_id WHERE p.host_id = $1`. Declared only as `fn list_host_agenda: Function[…]` (`operations.lzi:307`).
- The `list_my_*` / `list_property_*` / `list_traveler_reservations` / `list_my_conversations` family across `trust.lzi:276-278`, `operations.lzi:305-307`, `messaging.lzi:276-277`.

These are the demand signal for actor-relative compose.

## Users & jobs

- **Pilot devs / agents**: need to write "list the things this actor is allowed to see" as a *declared* read, not a hand-rolled SQL literal — so the compiler can see, type, and expose it.
- **Reviewers / auditors**: need every actor-scoped read to show its scope (owner column, ownership join, or participant join) in the `.lzi`, not in Go.
- **The toolchain (`inspect` / exposure / LSP)**: needs these reads to be first-class IR so it can analyze tenancy and actor-leak risk.

## Requirements

- Extend `query.compose` so a composed read can be scoped by `ctx.actor` via three shapes seen in the pilots: **direct owner column** (`owner_id = ctx.actor`), **ownership join** (`property.host_id = ctx.actor`), and **participant join** (join-through a membership relation such as `conversation_participant`).
- Where a read *still* can't be expressed by compose, it falls back to a **declared `query.sql`** (visible in the `.lzi`) — never an opaque `@fn`.
- Migrate the Hostpoint `trust` / `operations` / `messaging` `list_*` reads off raw-SQL-in-`@fn`-Go onto declared `query.compose` (or declared `query.sql`).
- Spec 0010's doctor rule `ESC-RAWSQL-IN-HANDLER-001` goes silent for the migrated handlers.
- Teach the path in `docs/lazuli_way/escape-hatch-decision-tree.md`.

## Success

- All targeted Hostpoint `list_*` reads are declared (compose or `query.sql`); zero remain as opaque `fn …: Function[…]` raw-SQL handlers.
- `ESC-RAWSQL-IN-HANDLER-001` reports clean for `trust` / `operations` / `messaging`.
- `inspect` shows the actor scope (column / join path) for each migrated read.
- `cargo test -p lazuli_codegen_go compose_actor_relative` green; Hostpoint `lazuli check . && doctor . && go build ./...` green.

## Out of scope

- Write-side actor scoping (commands) — that is Hole-2, separately specced.
- New aggregation / window-function support in compose.
- Cross-tenant admin "see everything" bypasses beyond the existing `IsAdmin` branch already present in the pilots.

## Risk note

This is **graded / pilot-evidence-gated**: the grammar+IR+codegen addition ships only after the Hostpoint migration proves it expresses all three actor-scope shapes without dropping back to `query.sql` for the *common* cases. If a shape needs `query.sql`, that is an accepted, visible fallback — not a failure — but the decision tree must say so.

---

## BLOCKED 2026-06-01 — query.compose not in the integration lineage

0013 finishes the actor-relative LIMIT of `query.compose` (the emitter's stubbed `actor_scope`). But `query.compose` DOES NOT EXIST on `loop-serial`/`origin/main`: it was built end-to-end on branch `feat/query-compose` (c058444e), which is **99 commits behind origin/main and only 1 ahead — never integrated**. Our whole spec stack descends from origin/main (081785f3), which has no compose (`crates/.../emitter/query/compose*` absent; 0 grep hits for ComposeNode/query.compose/actor_scope).

So 0013 has an unmet PREREQUISITE: `feat/query-compose` must first be merged forward into the integration line (resolving 99 commits of drift) before its actor-scope limit can be finished and the 11 hostpoint `list_*` handlers (flagged live by 0010's ESC-RAWSQL-IN-HANDLER-001) can be migrated onto it.

This is a substantial, risky integration of a user-authored+pushed branch that other swarms may depend on — a MERGE DECISION for the maker, not in-scope for the spec-closing pass. 

**Disposition: 0013 BLOCKED-ON-INTEGRATION.** Sequence when unblocked: (1) maker merges `feat/query-compose` forward to the integration line; (2) THEN 0013 finishes actor_scope + migrates the 11 list_* handlers; (3) verify 0010's ESC-RAWSQL-IN-HANDLER-001 goes from 31 hits → only-the-genuinely-imperative remainder. Note: hostpoint go-build is also gated on the PT-BR scalar plugin codegen follow-up (see 0009).
