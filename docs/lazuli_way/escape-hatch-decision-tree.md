# Escape-hatch decision tree

## Reach for this

When the declarative surface seems too tight, walk this tree **top to bottom**
and stop at the first match. Each step down is a step away from what the compiler
can see, check, and enforce — so only descend when the step above genuinely can't
say it.

1. **Can a typed effect say it?** A `command`, `query.list`, or `query.lookup`
   covers ordinary writes and reads. Use it. (And if it's plain CRUD, reach for
   [`conventions [crud]`](crud-by-convention.md) before writing a command at
   all.)

2. **Does it need joins / aggregates / window functions / a denormalized row?**
   Declare the SQL as a `query.sql` — with its `returns` record, `policy`, and
   `params` — or compose existing reads with `query.compose`. The SQL is now a
   **declared, reviewable seam**: the compiler sees its shape, its policy, and
   its tenancy contract.

3. **Is it a genuine vendor call or irreducibly imperative step?** (payments,
   maps, email, an external API, multi-step orchestration the surface can't
   express.) Then — and only then — a `@fn` Go handler.

## The rule that closes the loophole

> **Raw SQL must live in a declared `query.sql` / `query.compose`. It must NEVER
> appear as a SQL string literal inside a `@fn` Go handler.**

A `@fn` is for vendor calls and custom imperative logic — *not* a place to bury a
query the compiler should have seen. SQL hidden in Go is invisible to tenancy
checks, policy analysis, and the read-shape walker; it is the exact "invisible
escape hatch" the hostpoint audit flagged.

## Before (anti-example) / After (idiomatic)

**Before** — a multi-line `SELECT` baked into a Go handler as a string literal,
where no Lazuli rule can see its tenancy contract or return shape:

```go
// hostpoint app/features/trust/handlers/list_property_reviews.go
const baseSelectListPropertyReviews = `
SELECT r.id, r.rating, r.comment, ...
FROM reviews r
JOIN ...
WHERE r.property_id = $1`
```

**After** — the same query declared as a `query.sql` (or `query.compose`), with
an explicit `returns` record, `policy`, and `params`, so it is visible to the
compiler and the tenancy / policy rules:

```
query.sql list_property_reviews
  returns PropertyReviewRow
  policy @policy.public
  params
    property_id: ID required
  sql """
    SELECT r.id, r.rating, r.comment, ...
    FROM reviews r JOIN ... WHERE r.property_id = :property_id
  """
```

## Enforced by

`ESC-RAWSQL-IN-HANDLER-001` *(incoming — spec 0010)* — fires on a SQL string
literal (`SELECT`/`INSERT`/`UPDATE`/`DELETE`) inside a `@fn` Go handler,
directing the author to declare it as `query.sql`/`query.compose` instead.
Siblings landing with it: `ESC-SQL-TENANCY-CONTRACT-001`,
`ESC-SCOPE-OVERRIDE-UNGUARDED-001`.
