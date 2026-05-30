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
  hook before_create: Hook[CreateCustomer]
  fn risk_score: Function[Customer, Integer]
```

By default, Lazuli resolves `status_cell` to `features/customer/ui/status_cell.*`.

The declaration keyword is the call-site namespace. `fn risk_score` resolves as `@fn.risk_score`, `hook before_create` resolves as `@hook.before_create`, and `client status_cell` resolves as `@client.status_cell`.

Call sites reference extension contracts through capability namespaces instead of a single `ext.*` namespace:

```lazuli
cells
  status @client.status_cell

let score = @fn.risk_score(target)
validates field tier @validator.validate_tier
```

The closed extension namespace set is `@client.*`, `@fn.*`, `@hook.*`,
`@validator.*`, `@adapter.*`, and `@query_modifier.*`. In registry
integration bindings, `@adapter.*` means a local adapter extension reference.
Package adapter sources such as `@runtime/...` and `@plugin/...` are adapter
provenance markers, not extension namespaces.

Use `at` only when the convention is not enough:

```lazuli
extensions
  client status_cell: CellRenderer[Customer] at "@shared/customer/status_cell.tsx"
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
type RiskScoreFn func(ctx Context, customer Customer) (int, error)
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
- Integration adapters referenced by auth or other reusable contracts

Background job handlers, webhook verifiers, and webhook handlers are custom source files, but they are declared inline on `job` and `webhook` rather than listed in `extensions`.

## Runtime registry hooks

Adapter packs install these at `init()` to bind cross-cutting behavior
into the framework without taking a hard import dependency. All four
ship **experimental** for the first minor that introduces them (per
`docs/release-policy.md` §"Stability tiers").

| Hook | Where | Purpose | Stability |
|---|---|---|---|
| `webhooks.RegisterEventPublisher(p EventPublisher)` | `runtime/go/lazuli/webhooks/receive.go` | Breaks the `lazuli ↔ webhooks` import cycle; installed by `lazuli.init` so receivers can fire `Emits` without importing the root. | experimental |
| `webhooks.RegisterIdempotencyChecker(fn)` | `runtime/go/lazuli/webhooks/receive.go` | Installs an inbound dedupe hook (mirrors the prelude/increment hook pattern). Returns `(seen, err)`; on `seen == true`, the receiver responds `200` without re-dispatch. | experimental |
| `notifications.Registry.RegisterThrottleStore(store)` | `runtime/go/lazuli/notifications/dispatch.go` | Optional binding consulted before each dispatch when `contract.Throttle != nil`. Adapter packs wire Redis / Postgres / etc. | experimental |
| `notifications.Registry.RegisterDigestStore(store)` | `runtime/go/lazuli/notifications/dispatch.go` | Optional binding for `digest` mode. When wired, `Send` enqueues to the store and an external flusher emits on window close. | experimental |

## Not Allowed As Feature Extensions

- Replacing the table generator
- Replacing the policy system
- Replacing the router
- Replacing schema migrations
- Unmarked raw SQL inside normal `query`

If a feature needs to replace a whole generated structure, it should use an explicit escape hatch.

## SQL Queries

Normal queries stay declarative:

```lazuli
query.list list
  paginate 50
```

SQL-backed queries stay in the `query.*` reference namespace, but the declaration mode must be explicit:

```lazuli
record CustomerLtv
  customer_id: ID
  amount: @semantic.Money
  currency: Text

query.sql customer_lifetime_value
  returns CustomerLtv[]

  scope
    org = ctx.user.org

  sql "./queries/customer_ltv.sql"
```

`query.sql` means:

- Lazuli cannot fully derive policy safety.
- The return shape must resolve to a `record`, resource, or registered external contract before codegen.
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
- `workflow` may declare shared defaults such as `policy @policy.update` and `emits status_changed`; transitions inherit them, and a transition uses `requires @policy.<name>` only when it needs stronger authority.
- Views inherit read safety from their `source query.*`; inherited tenancy/soft-delete scope plus local query `scope` remain the source of tenant and soft-delete boundaries.

Query modifiers are explicit query attachments:

```lazuli
query.list list
  modifier @query_modifier.query_scope_modifier
```

Use them when a generated query still needs adapter-specific scoping or ranking logic that cannot fit the fixed predicate language. Do not declare a `query_modifier` extension without attaching it to a query.

Query modifiers run after inherited scope, local scope, and filters. They cannot remove tenant or soft-delete predicates; use `scope override` for explicitly cross-tenant/admin queries.

## Escape Routes

Some routes do not belong inside Lazuli. Mark them explicitly:

```lazuli
escape_route "/admin/raw-sql-console"
  at "./pages/sql_console.tsx"
  policy @role.admin
  tenant org
```

Lazuli should know that the route exists and should still track its coarse security envelope. It should not try to generate the route's queries, views, commands, or internal behavior.

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

`lazuli inspect` should also expose a derived manifest of every custom file implied by inline declarations such as `handler`, `verify`, `validates resource`/`validates field`, and inline `block ... at`. The authoring syntax keeps implementations near their semantic use; the manifest preserves the old index-style view for generators and agents.

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
    delegated_to
      invoice: "invoicing"
      scoring: "credit scoring engine"

  # Complementary context prose lives in the co-located customer.ctx.md sidecar.
```

The context file is resolved by CONVENTION: a co-located `<feature>.ctx.md` markdown sidecar next to the feature's `.lzi` (here `customer.ctx.md`), probed at a single base — no keyword, no path argument, no override. (The former `attach_ctx "<path>"` directive is retired; the parser hard-errors `E-ATTACH-CTX-RETIRED`.) It is complementary prose only: history, AI guidance, performance notes, narrative examples, and decision logs. It should not duplicate schema, operations, policies, rules, events, or extension contracts from the `.lzi` file.
