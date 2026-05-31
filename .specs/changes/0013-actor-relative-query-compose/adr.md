# ADR — 0013 Actor-Relative `query.compose`

> Status: Accepted · Created: 2026-05-31 · Track: evolve/ship

## Context

When `query.compose` was built (`feat/query-compose`, 2026-05-30) we deliberately stopped at static reads: a root SELECT, `include` edges, a static `where`, and `order`. The emitter (`crates/lazuli_codegen_go/src/emitter/query/compose.rs`) records the gap in its own header doc and leaves a commented `actor_scope: Option<ActorScope>` slot on `ComposeNode` plus an `emit_actor_scope_placeholder()` stub — the known limit, recorded on purpose.

The pilots prove the shape of that limit. Across Hostpoint's `trust` / `operations` / `messaging` reads, the actor scope takes exactly three forms:

1. **Direct owner column** — `reservation.traveler_id = ctx.actor` (`list_traveler_reservations`).
2. **Ownership join** — `JOIN property p … WHERE p.host_id = ctx.actor`, where the scope column lives on a *joined* relation, not the root (`list_host_agenda`, `list_property_reviews`).
3. **Participant join** — join-through a membership relation, e.g. `conversation_participant.user_id = ctx.actor` (`list_my_conversations`, `list_my_threads`).

A fourth wrinkle: `list_property_reviews` mixes the scope with a *visibility OR* (`published = true OR actor-is-host OR actor-is-admin`). That is an actor-conditional row filter, not a pure scope — it is the boundary case where compose may legitimately hand off to a declared `query.sql`.

The forces: (a) compose's strength is the declared, inspectable root+includes graph — we must not turn it into an arbitrary query builder; (b) but if we under-build, the common actor reads stay in opaque `@fn` Go and the HIGH-1 invisibility persists; (c) whatever we add must keep the *scope* visible in the `.lzi` so `inspect`/exposure/LSP can reason about actor-leak and tenancy.

## Decision

Add a single declarative scoping clause to `query.compose` — **`scope by actor`** — that names how the composed root is restricted to `ctx.actor`, using a *path* into the include graph rather than free-form SQL. It compiles to a deterministic, scoped `WHERE` (direct column) or `JOIN … WHERE` (ownership / participant) folded into the root SELECT.

The path grammar reuses the relations already declared by `include`, so the scope is expressed in terms the compose node already knows:

```
query.compose list_host_agenda {
  root reservation
  include property as property   # belongs_to
  scope by actor via property.host_id   # ownership join
  where check_in >= :from and check_out <= :to
  order by check_in asc
}
```

Three scope forms map to one clause:
- `scope by actor via <column>` — direct owner column on the root.
- `scope by actor via <include>.<column>` — ownership join through a declared include edge.
- `scope by actor through <relation> on <fk>` — participant join through a membership relation declared inline (the only new relation the clause may introduce).

When the read carries an actor-*conditional visibility OR* (the `list_property_reviews` case), compose does **not** try to express it. The decision tree routes it to a **declared `query.sql`** — visible in the `.lzi`, analyzable by the toolchain, but with the boolean logic written explicitly. The load-bearing rule: *visible-and-declared beats clever-and-opaque.*

## Consequences

- The three common actor-scope shapes become declared, inspectable reads. HIGH-1 invisibility is closed for them.
- The IR gains exactly one optional field (`ComposeNode.actor_scope: Option<ActorScope>`), already stubbed — minimal blast radius on the existing emitter.
- The `scope by` clause is intentionally *not* a general predicate language: it scopes, it does not filter arbitrarily. Arbitrary actor-conditional logic is pushed to declared `query.sql`. We accept that one Hostpoint read (`list_property_reviews`) lands as `query.sql`, by design.
- Exposure analysis must now treat a `scope by actor` read as actor-restricted (cannot be exposed as a public/anonymous read without a diagnostic) — a new invariant the typechecker asserts.

## Alternatives considered

- **General `where ctx.actor …` predicate in compose** — rejected: turns compose into an open query builder, defeats the "declared graph" inspectability that is its whole point, and makes actor-leak analysis undecidable.
- **Leave actor reads in `query.sql` entirely; don't extend compose** — rejected: the three scope shapes are the *common* case (most `list_my_*` reads), so the idiom must cover them; otherwise compose teaches "drop to SQL" for the majority of real reads.
- **A separate `query.scoped` primitive** — rejected: it would duplicate compose's root+include machinery; the scope is one clause on the existing primitive, not a new one.
- **Auto-infer the scope from a relation's `owner` annotation** — rejected (for now): too magical; the ownership/participant join path must be explicit so the reader sees exactly which join enforces the guard. Revisit if pilots show one obvious convention.
