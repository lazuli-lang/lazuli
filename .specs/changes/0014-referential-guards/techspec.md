# TechSpec — 0014 Referential Guards

> Track: evolve/ship · Depends on: 0001 · Parallel-safe: no (mutates both pilots' `.lzi` resources + deletes 15 guard handlers; adds grammar/IR/codegen shared across the syntax + go-codegen crates)

## Problem

The "referential guard" — load target, COUNT/EXISTS live references, reject if any — is hand-written **15× across 11 features in both pilots**, with **7 handlers commenting "same guard pattern as \<other feature\>"**. It is the strongest vocabulary signal in the audit.

Evidence:
- `pauta features/billing_config/handlers/guard_billing_type_in_use.go:13,15-26` — *"Same guard pattern as agency_service_catalog.guard_no_active_service_items."* `SELECT COUNT(*) FROM invoice WHERE billing_type_id = $1 AND tenant_id = $2 AND deleted_at IS NULL` → reject if `> 0`.
- `pauta features/workflow_templates/handlers/guard_template_not_in_use.go:13-22` — *"Same guard pattern as billing_config.guard_billing_type_in_use, but uses EXISTS instead of COUNT."* `SELECT EXISTS (SELECT 1 FROM job WHERE workflow_template_id = $1 AND deleted_at IS NULL)`.
- 12 in pauta (`guard_billing_type_in_use`, `guard_no_active_service_items`, `guard_template_not_in_use`, `guard_broadcast_area_in_use`, `guard_category_in_use`, `guard_payment_terms_in_use`, `guard_no_active_invoice`, `guard_no_active_teams`, `guard_no_open_activities`, `guard_no_open_step_activities`, `guard_active_table_delete`, `guard_attachment_not_deleted`) + 3 in hostpoint (`listings/guard_no_active_reservations`, `trust/guard_no_published_reviews`, `messaging/guard_conversation_empty`) = 15.

Each is wired in as a precondition before a delete/destructive-mutate command, returning `runtime.ErrReferencedInUse`.

## Goals

- Add a declarative referential-integrity guard primitive: resource-level `restrict on_delete references <relation> via <fk>`.
- Lower it to a tenant-scoped, soft-delete-aware `EXISTS` precondition that rejects with `ErrReferencedInUse`.
- Add a doctor rule that detects a hand-written `@fn` guard matching the COUNT/EXISTS-then-reject shape and suggests the primitive.
- Migrate all 15 guards in both pilots; delete the handlers and their "same pattern as…" comments.
- Fill `docs/lazuli_way/referential-guards.md` (stub from 0001).

## Non-Goals

- Cascade delete (deleting the referencing rows) — restriction only.
- Schema/DDL-level FK enforcement.
- Cross-database references.

## Design

### Grammar (surface)

Resource-level clause, repeatable, on the resource being protected:

```
resource billing_type {
  …
  restrict on_delete references invoice via billing_type_id
}

resource workflow_template {
  …
  restrict on_delete references job via workflow_template_id
}
```

- `restrict on_delete references <relation> via <fk>` — `<relation>` is the referencing relation, `<fk>` its column pointing at this resource's id.
- Optional `where <predicate>` for guards that only count a *subset* of references (e.g. only *open* activities): `restrict on_delete references activity via step_id where status = 'open'`. Covers `guard_no_open_activities` / `guard_no_open_step_activities`.
- Repeatable: a resource may declare several `restrict on_delete` lines (one per inbound relation).
- Round-trips through `fmt`.

### IR

A new resource-level construct lowered to a command precondition:

```rust
pub struct RestrictOnDelete {
    pub relation: String,       // referencing relation
    pub fk: String,             // column on `relation` pointing at this resource
    pub extra_where: Option<Predicate>,  // optional subset filter
    // resolved by typechecker:
    pub tenant_scoped: bool,    // relation has tenant_id
    pub soft_delete: bool,      // relation has deleted_at
}
```

Lowering: every delete (and destructive-mutate) command on the resource gains a `Precondition::ReferentialGuard(RestrictOnDelete)` ahead of the mutation, in declaration order.

Typechecker invariants:
- `<relation>` exists; `<fk>` exists on it and its type unifies with this resource's id type.
- `tenant_scoped` is set iff `<relation>` carries a tenant column; `soft_delete` iff it carries `deleted_at`. These are *derived*, not author-supplied, so they can't be forgotten.
- `extra_where` type-checks against `<relation>`'s columns.

### Codegen

Both COUNT and EXISTS hand-written forms lower to a single canonical `EXISTS` check (cheaper, short-circuits). Generated precondition (Go), for `billing_type` → `invoice`:

```go
// referential guard: restrict on_delete references invoice via billing_type_id
func guardBillingTypeInvoiceRefs(ctx context.Context, db DBTX, tenantID, id string) error {
    const q = `SELECT EXISTS (
        SELECT 1 FROM invoice
        WHERE billing_type_id = $1
          AND tenant_id = $2          -- emitted because invoice.tenant_id exists
          AND deleted_at IS NULL      -- emitted because invoice.deleted_at exists
    )`
    var inUse bool
    if err := db.QueryRowContext(ctx, q, id, tenantID).Scan(&inUse); err != nil {
        return err
    }
    if inUse {
        return runtime.ErrReferencedInUse
    }
    return nil
}
```

Rules:
- `AND tenant_id = $N` emitted **iff** `tenant_scoped`.
- `AND deleted_at IS NULL` emitted **iff** `soft_delete`.
- `extra_where` appended as additional `AND (...)`.
- The guard runs inside the same transaction as the delete, before the mutation, in declaration order. Multiple `restrict on_delete` lines → multiple guards, short-circuiting on the first hit.
- Golden tests assert the tenant + soft-delete predicates are present/absent exactly per the relation's schema (the load-bearing correctness check).

### Doctor

New rule `SUGGEST-REFERENTIAL-GUARD-001`: when a `@fn` / handler body matches the shape `SELECT (COUNT(*)|EXISTS(SELECT 1)) FROM <rel> WHERE <fk> = $… [AND …] ` followed by a `> 0` / truthy reject returning `ErrReferencedInUse`, emit a hint:

```
hint: this looks like a referential guard. Declare it on the resource:
        restrict on_delete references <rel> via <fk>
      see docs/lazuli_way/referential-guards.md
```

The rule recognizes both COUNT and EXISTS variants and is anchored to the idiom doc.

## Rollout

1. Land grammar (`restrict on_delete references … via … [where …]`) + fmt round-trip in `lazuli_syntax`.
2. Add `RestrictOnDelete` IR + `Precondition::ReferentialGuard` lowering + typechecker invariants (derive `tenant_scoped` / `soft_delete`).
3. Codegen the EXISTS precondition in `lazuli_codegen_go`; golden tests (`referential_guard`) covering: tenant+soft-delete present, neither present, `extra_where`, multiple guards on one resource.
4. Add doctor rule `SUGGEST-REFERENTIAL-GUARD-001`.
5. Migrate pilots:
   - **pauta** (12): add `restrict on_delete` to the protected resources (`billing_type`, `service_catalog`, `workflow_template`, `broadcast_area`, `category`, `payment_terms`, `invoice`-target, `team`, `activity`-targets, `workflow_step`, `table`, `attachment`); delete the 12 `guard_*.go` handlers and their decls.
   - **hostpoint** (3): add `restrict on_delete` for `property` (→ reservation), `property`/review (→ published review), `conversation` (→ message); delete the 3 `guard_*.go` handlers.
   - Remove the 7 "same guard pattern as…" comments with their code.
6. Run both pilots: `lazuli check . && doctor . && go build ./...`.
7. Fill `docs/lazuli_way/referential-guards.md` (What / When / Grammar / Lowering / Escape hatch / See also); link from `SUGGEST-REFERENTIAL-GUARD-001`.

## Gate

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

1. `cargo test -p lazuli_syntax referential_guard` green — `restrict on_delete references … via … [where …]` parses and round-trips through fmt, including the multi-clause and `where`-subset forms.
2. `cargo test -p lazuli_codegen_go referential_guard` green — golden tests prove the generated EXISTS emits `tenant_id = $N` iff the relation is tenant-scoped and `deleted_at IS NULL` iff soft-deletable, handles `extra_where`, and rejects with `ErrReferencedInUse` (both COUNT- and EXISTS-origin guards lower identically).
3. All 15 hand-written guards are migrated: the 12 pauta + 3 hostpoint `guard_*.go` handlers are deleted along with the 7 "same guard pattern as…" comments, and `restrict on_delete` declarations replace them; `SUGGEST-REFERENTIAL-GUARD-001` flags a deliberately-reintroduced hand-written guard and names the primitive.
4. `docs/lazuli_way/referential-guards.md` is filled (the 0001 stub) and linked from `SUGGEST-REFERENTIAL-GUARD-001`; both pilots green: `lazuli check . && doctor . && go build ./...`.
