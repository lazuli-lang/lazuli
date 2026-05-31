# ADR — 0014 Referential Guards

> Status: Accepted · Created: 2026-05-31 · Track: evolve/ship

## Context

The audit found the same delete-safety guard hand-written 15 times across 11 features in both pilots, with 7 handlers explicitly commenting "same guard pattern as \<other feature\>". The body is invariant up to table/column names:

```sql
-- COUNT variant (guard_billing_type_in_use.go)
SELECT COUNT(*) FROM invoice
WHERE billing_type_id = $1 AND tenant_id = $2 AND deleted_at IS NULL;   -- reject if > 0

-- EXISTS variant (guard_template_not_in_use.go)
SELECT EXISTS (SELECT 1 FROM job WHERE workflow_template_id = $1 AND deleted_at IS NULL);  -- reject if true
```

Two surface choices are open:
- A **command-level** clause — `guard references <relation>` as a precondition on the specific delete command.
- A **resource-level** clause — `restrict on_delete` on the resource being protected, which applies to any delete of that resource.

Forces:
- The pilots attach the guard at the *command* granularity (each `guard_*` handler is wired into one delete command), but the *intent* is a property of the resource ("a billing_type cannot be deleted while referenced").
- Soft-delete awareness (`deleted_at IS NULL`) and tenant scope (`tenant_id = …`) are non-negotiable and currently depend on the author remembering them — exactly the kind of correctness the framework should own.
- Some guards key on a non-id column or a compound relation; the grammar must name the *referencing relation and its foreign key*, not assume `<resource>_id`.

## Decision

Adopt a **resource-level `restrict on_delete` declaration**, desugaring to a **command-level precondition** at every delete/destructive-mutate of that resource. The author declares, on the protected resource, the relations that must have no live references:

```
resource billing_type {
  …
  restrict on_delete references invoice via billing_type_id
  restrict on_delete references quote    via billing_type_id
}
```

Each `restrict on_delete references <relation> via <fk>` clause lowers to one scoped existence check that runs as a precondition before the delete commits. The check is generated **tenant-scoped and soft-delete-aware by construction**: the emitter always adds `deleted_at IS NULL` when the referencing relation has a soft-delete column, and always adds `tenant_id = :tenant` when it is tenant-scoped — the author cannot forget them.

Resource-level is chosen over pure command-level because the integrity rule belongs to the *resource* (it holds for every way the resource can be deleted), and declaring it once protects all current and future delete paths. The lowering is still a command-level precondition, so the runtime behavior matches the hand-written guards exactly.

Both `COUNT(*) > 0` and `EXISTS(...)` hand-written forms lower to the **same** generated `EXISTS` check (EXISTS is strictly cheaper and is the canonical lowering) — the COUNT variant in the pilots is incidental.

Rejection raises the existing `ErrReferencedInUse` runtime error the pilots already return, so callers and HTTP mapping are unchanged.

## Consequences

- 15 hand-written guards + 7 "same pattern as…" comments collapse into one declaration per protected relation. Duplication and cross-reference comments disappear.
- The integrity rule becomes visible on the resource, inspectable by the toolchain, instead of hiding in a `@fn` whose name is the only documentation.
- Tenant scope and soft-delete are generated, not remembered — removing the most likely correctness bug class in hand-written guards.
- A doctor rule can now *suggest* the primitive whenever a `@fn` matches the COUNT/EXISTS-then-reject shape, driving future code toward vocabulary.
- We accept that only *restriction* is supported, not cascade-delete; cascade is deferred as a separate, riskier decision.
- A resource with many inbound references accrues several `restrict on_delete` lines — verbose but explicit; we prefer that over an implicit "guard everything" mode.

## Alternatives considered

- **Command-level `guard references` only** — rejected as the primary surface: the rule belongs to the resource and should protect every delete path, not just the one command the author remembered to annotate. (We keep command-level as the *lowering target*, not the author surface.)
- **Rely on database foreign keys / `ON DELETE RESTRICT`** — rejected: raw FKs are neither soft-delete-aware (they'd block on already-deleted rows or require partial indexes) nor tenant-scoped, and they surface as opaque DB errors instead of the typed `ErrReferencedInUse`.
- **Cascade-delete in the same primitive** — rejected for now: silently deleting referencing rows is far riskier than rejecting; out of scope, revisit separately.
- **Auto-infer guards from declared relationships** — rejected: too implicit; whether a reference *restricts* deletion is a policy decision the author must state, and the explicit `restrict on_delete` line is the documentation.
