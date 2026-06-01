# Referential guards

## Reach for this

When deleting a row must be blocked while other live rows still reference it
("you can't delete a `BillingType` while an `invoice` points at it"), declare a
`restrict on_delete references <relation> via <fk>` clause on the protected
resource instead of hand-writing a `@fn` guard that runs a `COUNT`/`EXISTS`
probe and rejects with an in-use error.

The generated guard is **tenant-scoped and soft-delete-aware by construction**:
the `tenant_id = …` and `deleted_at IS NULL` predicates are derived from the
referencing relation's schema, so the author can never forget them — the single
most likely correctness bug in a hand-written guard (a forgotten tenant scope
leaks existence across tenants; a forgotten soft-delete blocks deletes against
already-deleted referrers).

## Before (hand-rolled) / After (idiomatic)

**Before** — a `@fn` guard per protected resource, each re-deriving the scoped
SQL by hand, with a "same guard pattern as …" cross-reference comment because
the shape is invariant up to table/column names. Hand-written **15× across 11
features** in the pilots; **7** carry the "same pattern as" comment:

```go
// pauta features/billing_config/handlers/guard_billing_type_in_use.go
// COUNT variant — "Same guard pattern as agency_service_catalog.guard_no_active_service_items."
const q = `SELECT COUNT(*) FROM invoice
    WHERE billing_type_id = $1 AND tenant_id = $2 AND deleted_at IS NULL`
// → reject with ErrBillingTypeInUse if COUNT > 0

// pauta features/workflow_templates/handlers/guard_template_not_in_use.go
// EXISTS variant — "Same guard pattern as billing_config.guard_billing_type_in_use,
//  but uses EXISTS instead of COUNT."
const q = `SELECT EXISTS (
    SELECT 1 FROM job WHERE workflow_template_id = $1 AND deleted_at IS NULL
)`
// → reject with ErrReferencedInUse if true
```

**After** — declare the guard on the protected resource. One line per inbound
relation that blocks deletion; repeatable:

```
resource BillingType
  name: Text required
  restrict on_delete references invoice via billing_type_id
  restrict on_delete references quote   via billing_type_id

resource WorkflowTemplate
  restrict on_delete references job via workflow_template_id
```

Both the COUNT and EXISTS hand-written forms lower to the **same** canonical
`EXISTS` precondition (EXISTS short-circuits and is strictly cheaper). The
emitted Go guard (`<feature>/guards.gen.go`) matches the hand-written SQL
exactly, including the derived predicates:

```go
// referential guard: restrict on_delete references invoice via billing_type_id
func guardBillingTypeInvoiceRefs(ctx context.Context, db lazuli.DBTX, tenantID, id string) error {
    const q = `SELECT EXISTS (
        SELECT 1 FROM invoice
        WHERE billing_type_id = $1
          AND tenant_id = $2          -- emitted because invoice is tenant-scoped
          AND deleted_at IS NULL      -- emitted because invoice is soft-deletable
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

### Grammar

```
restrict on_delete references <relation> via <fk> [error <CODE>] [where <predicate>]
```

- `<relation>` — the referencing relation (e.g. `invoice`). May live in
  **another feature** (e.g. a `billing_config` guard referencing
  `customer_management.Customer`); the tenant + soft-delete scope is resolved
  module-globally, so a cross-feature guard still emits `AND tenant_id` +
  `AND deleted_at IS NULL` (no cross-tenant breach).
- `<fk>` — the column on `<relation>` pointing at this resource's id.
- `error <CODE>` *(optional)* — pins a per-guard domain error code (e.g.
  `error CATEGORY_HAS_CUSTOMERS`) the emitter rejects with via
  `runtime.NewReferencedInUseError("<CODE>")` instead of the bare
  `runtime.ErrReferencedInUse` sentinel. Omit it to keep the back-compat
  sentinel. Covers wire-pinned codes like `CATEGORY_HAS_CUSTOMERS`.
- `where <predicate>` *(optional)* — narrows the guard to a *subset* of
  references (e.g. only *open* activities), appended as `AND (<predicate>)`.
  Covers `guard_no_open_activities` / `guard_no_open_step_activities`.

### Lowering rules

- `AND tenant_id = $N` is emitted **iff** the referencing relation is
  tenant-scoped.
- `AND deleted_at IS NULL` is emitted **iff** the referencing relation has a
  `deleted_at` (soft-delete) column.
- Multiple `restrict on_delete` lines → multiple guards, run as preconditions
  before the delete in declaration order, short-circuiting on the first hit.
- Rejection raises `runtime.ErrReferencedInUse` (the same error the hand-written
  guards returned), so callers and HTTP mapping are unchanged — unless the guard
  authored `error <CODE>`, in which case it raises
  `runtime.NewReferencedInUseError("<CODE>")`, which `errors.Is`-es back to the
  sentinel and wires to HTTP 409 carrying `<CODE>` as the wire code.

### Escape hatch

`restrict on_delete` *restricts* deletion only. **Cascade delete** (deleting the
referencing rows) is deliberately out of scope — silently deleting referencing
rows is far riskier than rejecting, and is deferred as a separate decision. When
you genuinely need cascade or a cross-database reference, keep an explicit `@fn`
and declare the read as a `query.sql`/`query.compose` (see
[escape-hatch-decision-tree.md](escape-hatch-decision-tree.md)).

## Enforced by

- `SUGGEST-REFERENTIAL-GUARD-001`
  (`crates/lazuli_doctor/src/vocab/referential_guard_001.rs`) — fires when a
  `features/<f>/handlers/*.go` body matches the COUNT/EXISTS-then-reject shape
  (a `SELECT COUNT(*) FROM <rel>` or `SELECT EXISTS (SELECT 1 FROM <rel>)` over a
  `WHERE <fk> = $…` clause, followed by a `> 0` / truthy reject returning an
  in-use error). The hint names the declarative replacement
  (`restrict on_delete references <rel> via <fk>`) and points back at this doc.
  Recognizes both the COUNT and EXISTS variants. Advisory (`warning`); wired
  into `lazuli doctor` via the escape-hatch aggregator.

See also: [soft-delete.md](soft-delete.md) (the `deleted_at` column the guard's
predicate keys on) and the spec at `.specs/changes/0014-referential-guards/`.
