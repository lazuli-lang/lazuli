# Lazuli Extension Points

Lazuli should cover the repetitive 70% declaratively and expose controlled escape hatches for the rest.

The rule:

```txt
Custom code runs inside generated structures.
Custom code does not replace generated structures.
```

## Resolution

Extension implementations are colocated with the feature by convention:

```txt
features/customer/
  customer.lzi
  customer.ctx.md
  ui/
    status_cell.tsx
  hooks/
    before_create.go
  domain/
    risk_score.go
```

The capsule declares contracts:

```lazuli
extensions
  client status_cell: CellRenderer[Customer]
  server before_create: Hook[CreateCustomer]
  server risk_score: Function[Customer, RiskScore]
```

By default, Lazuli resolves `status_cell` to `features/customer/ui/status_cell.*`.

Use `at` only when the convention is not enough:

```lazuli
extensions
  client status_cell: CellRenderer[Customer] at "./ui/status_cell.tsx"
```

## TypeScript Contract

The framework generates the contract:

```ts
// .lazuli/generated/types/customer.ts
import type { ReactNode } from "react";
import type { Customer, RenderContext } from "./resources";

export type StatusCellProps = {
  row: Customer;
  value: Customer["status"];
  ctx: RenderContext;
};

export type StatusCellRenderer = (props: StatusCellProps) => ReactNode;
```

The user implements a named export:

```tsx
// features/customer/ui/status_cell.tsx
import type { StatusCellRenderer } from "@/.lazuli/generated/types/customer";

export const statusCell: StatusCellRenderer = ({ value }) => {
  return <Badge>{value}</Badge>;
};
```

Rules:

- Generated types are the contract.
- Extension implementations use named exports, never default exports.
- Export names are derived from Lazuli symbols.
- If a resource changes, generated types change and custom code fails at compile time.

## Go Contract

Go extension contracts are generated too:

```go
// .lazuli/generated/customer/types.go
package customer

type BeforeCreateHook func(ctx Context, input CustomerCreateInput) (CustomerCreateInput, error)
type RiskScoreFn func(ctx Context, customer Customer) (RiskScore, error)
```

The user implements matching named functions:

```go
// features/customer/hooks/before_create.go
package customer

func BeforeCreate(ctx Context, input CustomerCreateInput) (CustomerCreateInput, error) {
	input.Email = strings.ToLower(strings.TrimSpace(input.Email))
	return input, nil
}
```

Lazuli should connect Go extensions by generated registration code, not runtime reflection.

## Allowed Extension Points

- Cell renderers
- Custom form fields
- Custom view blocks
- Lifecycle hooks
- Pure domain functions
- Custom validators
- Query modifiers
- Background jobs
- Webhooks and integration adapters

## Not Allowed As Feature Extensions

- Replacing the table generator
- Replacing the policy system
- Replacing the router
- Replacing schema migrations
- Unmarked raw SQL inside normal `query`

If a feature needs to replace a whole generated structure, it should use an explicit escape hatch.

## Raw Queries

Normal queries stay declarative:

```lazuli
query list
  paginate 50
```

Raw SQL stays in the `query.*` namespace, but it must be marked explicitly:

```lazuli
query customer_lifetime_value raw
  returns CustomerLtv[]

  scope
    org = ctx.user.org

  sql "./queries/customer_ltv.sql"
```

`raw` means:

- Lazuli cannot fully derive policy safety.
- The query must still declare `scope`; raw SQL is not allowed to silently bypass tenant or soft-delete boundaries.
- Schema renames are not automatically safe.
- Manual tests are expected.
- The semantic graph still records the dependency.
- Consumers still reference it through the normal `query.*` namespace.

## Canonical Semantics

These conventions keep the explicit syntax predictable. See also [Canonical semantics](canonical-semantics.md).

- Query params exposed to routes and APIs should prefer scalar IDs, e.g. `parent_id: ID`, over passing hydrated entities.
- Many-to-many filters should name both sides and their guard, e.g. `labels has params.label when params.label`.
- Events use serializable IDs by default, e.g. `customer_id: ID` and `by_id: ID`.
- `workflow` may declare shared defaults such as `policy update` and `emits status_changed`; transitions inherit them unless they override locally.
- Views inherit read safety from their `source query.*`; inherited tenancy/soft-delete scope plus local query `scope` remain the source of tenant and soft-delete boundaries.

## Escape Routes

Some routes do not belong inside Lazuli. Mark them explicitly:

```lazuli
escape_route "/admin/raw-sql-console" at "./pages/sql_console.tsx"
```

Lazuli should know that the route exists, but it should not try to generate its policies, queries, views, or commands.

## Inspect Output

This design preserves small agent context:

```txt
Resource: Customer
Queries: 2
Commands: 2
Views: 2
Extension points implemented:
  status_cell   -> features/customer/ui/status_cell.tsx
  before_create -> features/customer/hooks/before_create.go
  risk_score    -> not implemented
```

`lazuli inspect` should also expose a derived manifest of every custom file implied by inline declarations such as `handler`, `verify`, `validates`, and inline `block ... at`. The authoring syntax keeps implementations near their semantic use; the manifest preserves the old index-style view for generators and agents.

The agent reads the capsule plus the small set of custom files, not the generated application.

## Context Files

Feature capsules can have a short markdown context file next to the capsule:

```txt
features/customer/
  customer.lzi
  customer.ctx.md
```

Lazuli resolves `<feature>.ctx.md` by convention. It is source, versioned with the feature, and never generated into frontend/backend output.

Use `context` only as an override when the convention is not enough:

```lazuli
feature customer
  purpose "CRM customers within an org. Tracks lifecycle status, ownership, and tier."

  non_goals
    invoice: "invoicing"
    scoring: "credit scoring engine"

  context "@docs/shared/customer-context.md"
```

The context file is complementary prose only: history, AI guidance, performance notes, narrative examples, and decision logs. It should not duplicate schema, operations, policies, rules, events, or extension contracts from the `.lzi` file.
