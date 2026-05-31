# TechSpec — 0013 Actor-Relative `query.compose`

> Track: evolve/ship · Depends on: 0001, 0010 · Parallel-safe: no (mutates Hostpoint `trust`/`operations`/`messaging` `.lzi` + handlers; shares the compose IR/emitter with any other compose work)

## Problem

`query.compose` can express a static composite read (root + includes + static `where` + `order`) but cannot scope a read to `ctx.actor`. The emitter records this as the known limit: `crates/lazuli_codegen_go/src/emitter/query/compose.rs` carries the doc header "Actor-relative filtering … is NOT yet supported — see `compose_actor_relative` (tracked in .specs/changes/0013)", a commented `actor_scope: Option<ActorScope>` field on `ComposeNode`, and an `emit_actor_scope_placeholder()` stub.

Because of that gap, Hostpoint's actor-scoped reads live as raw SQL string literals inside Go handlers, declared only as opaque `fn …: Function[…]`:

- `features/trust/handlers/list_property_reviews.go:17-34` — JOINs `app_user` + `reservation`, actor branch `published = true OR $2 = (SELECT host_id FROM property …) OR $3 = true`; declared `trust.lzi:276`.
- `features/operations/handlers/list_host_agenda.go:16-25` — ownership join `JOIN property p ON p.id = res.property_id WHERE p.host_id = $1`; declared `operations.lzi:307`.
- `list_traveler_reservations` (direct owner column `reservation.traveler_id = actor`), `list_my_conversations` / `list_my_threads` (participant join through `conversation_participant`) — declared `operations.lzi:306`, `messaging.lzi:276-277`.

All are invisible to `inspect` / exposure / LSP (HIGH-1).

## Goals

- Add a `scope by actor` clause to `query.compose` covering the three pilot scope shapes: direct owner column, ownership join, participant join.
- Lower the clause to a deterministic scoped `WHERE` / `JOIN … WHERE` folded into the root SELECT.
- Make exposure analysis treat a scoped read as actor-restricted.
- Migrate the Hostpoint `trust` / `operations` / `messaging` `list_*` reads onto declared `query.compose`; where compose can't express it (actor-conditional visibility OR), migrate to a declared `query.sql`.
- Silence `ESC-RAWSQL-IN-HANDLER-001` (spec 0010) for the migrated handlers.
- Teach the path in `docs/lazuli_way/escape-hatch-decision-tree.md`.

## Non-Goals

- Write-side (command) actor scoping — Hole-2, separate spec.
- Arbitrary actor-conditional predicates inside compose (those go to declared `query.sql`).
- New aggregation / window functions in compose.

## Design

### Grammar (surface)

One new clause on the existing `query.compose` block. Three forms, one keyword:

```
query.compose list_traveler_reservations {
  root reservation
  scope by actor via traveler_id            # (1) direct owner column on root
  order by check_in desc
}

query.compose list_host_agenda {
  root reservation
  include property as property               # belongs_to (existing)
  scope by actor via property.host_id        # (2) ownership join via include edge
  where check_in >= :from and check_out <= :to
  order by check_in asc
}

query.compose list_my_conversations {
  root conversation
  scope by actor through conversation_participant on conversation_id  # (3) participant join
  order by last_message_at desc
}
```

- `scope by actor via <col>` — root column equals `ctx.actor`.
- `scope by actor via <include>.<col>` — the scope column lives on a relation already brought in by `include`; reuses that edge's join.
- `scope by actor through <relation> on <fk>` — membership/participant relation; the clause introduces this *one* relation (an inner join `<relation> ON <relation>.<fk> = root.id AND <relation>.user_id = ctx.actor`). The actor-side column defaults to `user_id`; `… on <fk> matched by <actorcol>` overrides it.

`scope by` round-trips through `fmt`. At most one `scope by` clause per compose.

### IR

Fill the already-stubbed field on `ComposeNode` (compose.rs:167-173 echo; real def in `ir::query`):

```rust
pub struct ComposeNode {
    pub reader_name: String,
    pub root: ComposeRoot,
    pub includes: Vec<IncludeEdge>,
    pub actor_scope: Option<ActorScope>,   // 0013: now populated
}

pub enum ActorScope {
    /// (1) root_col = ctx.actor
    OwnerColumn { column: String },
    /// (2) <include>.<col> = ctx.actor, reusing an existing include edge's join
    OwnershipJoin { include: String, column: String },
    /// (3) inner join a membership relation: rel.fk = root.id AND rel.actor_col = ctx.actor
    ParticipantJoin { relation: String, fk: String, actor_col: String },
}
```

Typechecker invariants:
- The referenced column / include / relation must exist and the column type must unify with the actor id type.
- `OwnershipJoin.include` must name a declared `include` edge.
- A compose node with `Some(actor_scope)` is flagged `actor_restricted`; exposure analysis rejects exposing it as public/anonymous (new diagnostic, anchored to the decision tree).

### Codegen

Replace `emit_actor_scope_placeholder()` with real lowering, folded into `emit_root_select` (compose.rs:57-82). The reader signature already takes `ctx`; the migrated readers also take the actor (the existing handlers already pass `actor runtime.Actor`).

- `OwnerColumn { column }` → append `WHERE <root>.<column> = $N` (or `AND` if a static `where` exists), bind `ctx.Actor.UserID`.
- `OwnershipJoin { include, column }` → ensure the include's relation is INNER-joined in the root SELECT (compose already knows the belongs_to join key), append `AND <include>.<column> = $N`.
- `ParticipantJoin { relation, fk, actor_col }` → emit `JOIN <relation> ON <relation>.<fk> = <root>.id AND <relation>.<actor_col> = $N`.

Sketch (generated, `list_host_agenda`):

```go
const q = `
  SELECT res.id, res.check_in, res.check_out, res.status,
         p.title AS property_title, p.id AS property_id
  FROM reservation res
  JOIN property p ON p.id = res.property_id      -- include edge
  WHERE p.host_id = $1                            -- scope by actor via property.host_id
    AND res.check_in >= $2 AND res.check_out <= $3
  ORDER BY res.check_in ASC`
rows, err := emitter.DB.QueryContext(ctx, q, ctx.Actor.UserID, q.From, q.To)
```

The bind for the actor is always emitted *first* and sourced from `ctx`, never from caller-supplied args — so a caller cannot spoof the scope. Golden tests assert that invariant.

### Doctor / exposure

- `ESC-RAWSQL-IN-HANDLER-001` (from spec 0010) already fires on `fn …: Function[…]` reads whose Go body contains a `SELECT`. Once migrated to `query.compose` / `query.sql`, the handler is generated, not hand-written, so the rule goes silent — verification, not new code.
- New exposure invariant (above): exposing an `actor_restricted` compose read without an actor context emits a diagnostic linking to `docs/lazuli_way/escape-hatch-decision-tree.md#actor-relative-reads`.

### Fallback: declared `query.sql`

`list_property_reviews` carries an actor-conditional *visibility OR* (`published OR is-host OR is-admin`), not a pure scope. Compose does not express it. It migrates to a **declared `query.sql`** in `trust.lzi` — the SQL becomes visible (`inspect` sees the relations and binds) but the boolean logic is written explicitly. This is the accepted, visible fallback; the decision tree documents exactly when to take it.

## Rollout

1. Land grammar (`scope by …`) + fmt round-trip in `lazuli_syntax`.
2. Populate `ComposeNode.actor_scope` + typechecker invariants + exposure flag in the IR crate.
3. Replace `emit_actor_scope_placeholder()` with the three lowering arms in `compose.rs`; add golden tests (`compose_actor_relative`).
4. Migrate Hostpoint reads:
   - `operations`: `list_traveler_reservations` (OwnerColumn), `list_host_agenda` + `list_property_calendar` (OwnershipJoin).
   - `messaging`: `list_my_conversations`, `list_my_threads` (ParticipantJoin).
   - `trust`: `list_my_reviews` (OwnerColumn), `list_host_agenda` (OwnershipJoin), `list_property_reviews` → **declared `query.sql`** (visibility OR).
   - Delete the corresponding `fn …: Function[…]` opaque decls and the hand-written `handlers/list_*.go`.
5. Run `lazuli check . && doctor . && go build ./...` in Hostpoint; confirm `ESC-RAWSQL-IN-HANDLER-001` clean.
6. Extend `docs/lazuli_way/escape-hatch-decision-tree.md` with the "actor-relative read → `query.compose` (`scope by actor`) → else declared `query.sql`" path; link from the new exposure diagnostic.

## Gate

This change is **graded / pilot-evidence-gated**: the grammar+IR+codegen ships only once the Hostpoint migration proves the three scope shapes are expressible without dropping to `query.sql` for the common (pure-scope) reads. The one accepted `query.sql` fallback is `list_property_reviews`.

### Definition of Done

```
- [ ] **Grammar**: surface syntax parses; round-trips through fmt.
- [ ] **IR**: lowers to typed IR; invariants asserted in the typechecker.
- [ ] **Codegen**: emits Go; golden tests cover happy + edge paths.
- [ ] **Doctor**: a lint/doctor rule guides toward the idiom (or explains the escape hatch).
- [ ] **Teach**: the idiom doc exists under `docs/lazuli_way/`, linked from the relevant diagnostic.
- [ ] **Pilot proof**: at least one pilot uses it; raw-SQL/`@fn` escape-hatch count goes down (or is justified).
- [ ] **Gate**: `cargo test` green; pilot `lazuli check . && doctor .` clean; `go build ./...` green.
```

### Concrete gates for THIS change

1. `cargo test -p lazuli_codegen_go compose_actor_relative` is green — golden tests cover all three `ActorScope` arms (OwnerColumn, OwnershipJoin, ParticipantJoin) and assert the actor bind is sourced from `ctx` and emitted first (un-spoofable).
2. In Hostpoint, every `trust` / `operations` / `messaging` `list_*` read is a declared `query.compose` or declared `query.sql`; **zero** remain as `fn …: Function[…]` opaque raw-SQL handlers (grep of `*.lzi` + deletion of the matching `handlers/list_*.go`).
3. Spec 0010's `ESC-RAWSQL-IN-HANDLER-001` reports clean for `trust` / `operations` / `messaging` under `lazuli doctor .`; `inspect` shows the actor scope (column or join path) for each migrated read.
4. `docs/lazuli_way/escape-hatch-decision-tree.md` contains the "actor-relative read" path with the `query.compose` → declared-`query.sql` fallback rule, and the new exposure diagnostic links to its `#actor-relative-reads` anchor; Hostpoint `lazuli check . && doctor . && go build ./...` all green.
