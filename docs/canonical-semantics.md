# Lazuli Canonical Semantics

This is the short spec for canonical `.lzi` files. The goal is a single authoring voice: explicit enough for agents and compilers, still readable by humans.

## Canonical Shape

A feature is organized by responsibility:

```lazuli
feature customer
  purpose "CRM customers within an org."

  non_goals
    "invoicing - see feature invoice"

  uses org, user

  domain
    resource Customer
    query list
    event customer_created

  policies
  command create
  workflow lifecycle on Customer.status
  surface web admin
  extensions
```

The canonical form avoids compact aliases. Use `domain`, `resource`, `query`, `policies`, `command`, `workflow`, `surface`, and `extensions` explicitly.

## Feature Granularity

A feature is a product capability, not an entity bucket. Multiple features may operate on the same domain entity when they own distinct behavior.

Prefer:

```txt
feature customer        # lifecycle, ownership, tiering
feature customer_auth   # login, sessions, MFA
feature customer_tags   # tag taxonomy and assignments
feature customer_import # uploads, row validation, background import
```

Avoid a single `feature customer` that owns every concern touching `Customer`. Oversized capsules hide policy boundaries, blur ownership, and degrade agent edits. A healthy feature should usually fit in a few focused screens of text. When a capsule starts accumulating unrelated policies, jobs, webhooks, auth, and surfaces, split by capability and connect features through `uses`, queries, commands, events, and extensions.

The kitchen-sink fixture in `examples/full-capsule.lzi` intentionally contains several feature blocks to show these boundaries under pressure.

## Dependencies

`uses` declares feature-level semantic dependencies:

```lazuli
feature issue
  uses team, user, project, cycle, label
```

It is not an import statement for generated code. It gives the analyzer, graph, and agent context an explicit dependency list before they inspect the full body. Types referenced in the capsule should resolve through local declarations, `uses`, built-ins, or adapter-provided registries.

## Cross-Feature References

Local references omit the feature prefix:

```lazuli
target customer = query.by_id(id: params.id)
source query.list
```

Cross-feature references must be feature-qualified and the referenced feature must appear in `uses`:

```lazuli
target customer = customer.query.by_id(id: params.customer_id)
extends customer.surface.web.admin.view.detail
```

The qualifier is the feature id, not a generated package name. Canonical `.lzi` should not rely on import-like aliasing in v0.

When one feature calls another feature's query, the provider query's effective scope still applies. The caller's policy authorizes the caller's operation; the provider feature's query scope preserves the provider's data boundary. `explain` should show both edges:

```txt
customer_auth.command.enable_mfa
  policy: customer_auth.policies.update
  target: customer.query.by_id
  provider scope: customer.tenancy org + customer.soft_delete
```

Cross-feature event consumers (`on customer.customer_archived`) do not move ownership of the producer event. The producer owns the event contract; the consumer owns its reaction.

`uses` is strict in canonical v0. Every listed feature should be referenced by type, query, command, event, view extension, or another semantic edge in the capsule. Do not use `uses` for conceptual prose dependencies; put those in `purpose`, `non_goals`, or `<feature>.ctx.md`.

## Context Files

The standard long-form context file is co-located beside the capsule as `<feature>.ctx.md`. It is source prose, not generated frontend/backend output.

Use inline `purpose` and `non_goals` for short metadata. Use `<feature>.ctx.md` for history, gotchas, performance notes, decision logs, and narrative examples. Do not duplicate schema, operations, policies, rules, events, or extension contracts there.

`context` is only an override when the convention is not enough:

```lazuli
feature customer
  context "@docs/customer/customer.ctx.md"
```

The compiler may validate that the referenced file exists and `lazuli inspect` may aggregate it, but Lazuli should not rewrite the markdown file.

## Resources And Relations

Fields are declared as `name: Type modifier`. Required/optional should be visible for non-default cases.

```lazuli
resource Issue
  tenancy team
  parent: Issue optional
  labels: many Label
  status: IssueStatus = backlog
```

`tenancy team` is the source of truth for the tenant axis. It injects a required `team: Team` field into the resource and the default query scope `team = ctx.team`.

`soft_delete` injects the default query scope `deleted_at = nil`.

Together:

```lazuli
resource Issue
  tenancy team
  soft_delete
```

means declarative queries inherit:

```lazuli
scope
  team = ctx.team
  deleted_at = nil
```

Do not restate inherited tenancy or soft-delete scope in normal queries. The analyzer should report redundant scope lines.

`many Label` is the simple many-to-many form. Lazuli may generate a join table, but the relation has no author-owned metadata, ordering, or lifecycle of its own.

If the relation needs payload such as `added_by`, `added_at`, `role`, ordering, or audit behavior, model it as an explicit resource instead of `many`:

```lazuli
resource IssueLabel
  issue: Issue required
  label: Label required
  added_by: User required
  added_at: DateTime required
```

Fields whose type is a resource from another feature create a semantic foreign-key edge:

```lazuli
feature customer_auth
  uses customer

  domain
    resource CustomerSession
      customer: Customer required on_delete restrict
```

The default hard-delete behavior is `on_delete restrict`. Use `on_delete cascade` or `on_delete nullify` only when that behavior is part of the product contract.

`on_delete` governs hard delete. Soft delete does not cascade automatically across resource references. If `Customer` is soft-deleted, `CustomerSession` rows may still exist; generated loaders and views should respect the referenced feature's query scopes when they expose referenced data. If a product needs soft-delete cascade, model it as an explicit command, job, rule, or extension.

## Queries

Queries have four common parts:

- `params`: caller-provided inputs.
- `key`: record identity for single-record queries.
- `scope`: extra safety boundary beyond inherited tenancy/soft-delete scope.
- `filters`: query predicates, either always applied or guarded with `when`.

```lazuli
query list
  params
    status: IssueStatus optional
    label: Label optional

  filters
    status when params.status
    labels has params.label when params.label

  order updated_at desc
  paginate 100
```

Inherited scope is always applied unless a query explicitly uses `scope override`. Local `scope` extends inherited scope and should be reserved for safety boundaries. `filters` describe data predicates. A filter without `when` is always applied. A filter with `when` is conditional.

```lazuli
filters
  parent.id = params.parent_id
  status when params.status
  labels has params.label when params.label
```

`status when params.status` means "apply `status = params.status` only when the param is present." For collection fields, name the operation: `labels has params.label when params.label`.

Path expressions are allowed in query filters:

```lazuli
filters
  parent.id = params.parent_id
```

This means "the related parent record has this id"; it is not a new field named `parent.id`.

### `key` And `scope`

`key` identifies the requested record. Inherited and local scope prove the current actor may see that record.

```lazuli
query by_id
  params
    id: ID

  key id = params.id
```

`key` alone never means "unscoped lookup." The effective scope for a tenanted, soft-deleted resource still includes the inherited tenant and soft-delete predicates.

Use `filters` for required caller predicates:

```lazuli
query sub_issues
  params
    parent_id: ID

  filters
    parent.id = params.parent_id
```

This keeps local `scope` reserved for safety boundaries.

Use `scope override` only for an explicitly cross-tenant or admin query:

```lazuli
query global_audit
  scope override
    deleted_at = nil
```

An override disables inherited tenancy scope and should require a strong policy.

### Raw Queries

`raw` means the query bypasses Lazuli's declarative query builder and is backed by an external SQL file. Lazuli can connect it, type it, and record it in the graph, but it cannot fully analyze the SQL body.

```lazuli
query lifetime_value raw
  returns CustomerLtv[]

  scope
    org = ctx.user.org

  sql "./queries/customer_lifetime_value.sql"
```

Raw queries still need `params`, `scope`, and a declared return type. The SQL can be hand-written, but the capsule must keep tenant and soft-delete boundaries visible because Lazuli should not silently rewrite arbitrary SQL.

## Commands

Commands have three conceptual inputs:

- `params`: route/locator data.
- `target`: loaded resource instance for existing-record mutations.
- `input`: payload being changed or created.

Canonical v0 keeps `params` and `input` separate. `params` are locator/caller coordinates that can come from routes, jobs, API calls, tests, or other commands. `input` is the domain payload being written. Do not depend on implicit `route.id` inside a command; surfaces may pass route values into command params, but commands are not route-owned.

Create commands often have only `input` and `derive`:

```lazuli
command create
  creates Customer
  input name, email
  derive owner = ctx.user
  policy create
  emits customer_created
```

Commands that mutate an existing resource should bind their target explicitly:

```lazuli
command reassign
  params
    id: ID

  target customer = query.by_id(id: params.id)

  input owner: User
  policy update
  emits customer_reassigned
```

The target binding gives rules and hooks a named resource instance. In this example, predicates can reference `customer.status` because `target customer` exists.

If a resource field is `required`, a create command must provide it through `input`, `derive`, a resource default, or resource-level injection such as `tenancy`. Required fields should not be filled by invisible convention.

`creates Customer` is the create-command counterpart to `target`: it makes the write effect explicit for humans, agents, plans, and generated handlers.

Update and delete commands should be explicit too:

```lazuli
command update_tier
  updates Customer

  params
    id: ID

  target customer = query.by_id(id: params.id)
  input tier: CustomerTier
  policy update
```

```lazuli
command remove_tag
  deletes CustomerTagAssignment

  params
    customer_id: ID
    tag_id: ID

  target assignment = query.assignment_by_customer_tag(customer_id: params.customer_id, tag_id: params.tag_id)
  policy update
```

`deletes X` means the command removes the targeted resource. Soft-deleted resources should use the resource's soft-delete mechanism unless the adapter or command explicitly opts into hard delete later.

`updates X` means the command changes the targeted resource. In canonical authoring, use one explicit effect for every command: `creates`, `updates`, or `deletes`. The analyzer should reject commands with multiple effects or none, except for non-mutating auth/session commands that return data and are explicitly documented by their adapter.

`input` has two canonical forms. Use the short list when the command creates or updates fields whose types can be inferred from the target resource:

```lazuli
input name, email, tier
```

Use a typed block when the inputs do not map one-to-one to fields, when multiple target resources are involved, or when inference would be ambiguous:

```lazuli
input
  customer: Customer
  tag: CustomerTag
```

Commands may return typed data when the operation is intentionally request/response rather than pure fire-and-redirect:

```lazuli
command login
  input email, password
  policy login
  returns AuthSession
```

This is not auth-specific; generated adapters should expose the return type in the client API. Prefer events for domain side effects and `returns` for immediate caller data.

Use `returns` when the caller needs immediate response data that is not simply the updated resource shape, such as an auth session, generated download URL, preview payload, or import validation summary. Do not use `returns` as a substitute for events.

## Workflows

A workflow owns transitions for one resource field:

```lazuli
workflow status on Issue.status
  policy update
  emits issue_status_changed

  start: todo -> in_progress
  complete: in_review -> done
```

Workflow-level `policy` and `emits` are defaults inherited by every transition.

Policy behavior:

- `policy` on the workflow is the default for all transitions.
- `policy` on a transition overrides the workflow default for that transition.

Event behavior:

- `emits` on the workflow always fires for every transition.
- `emits` on a transition fires additionally for that transition.
- If the workflow has no `emits`, only transition-specific events fire.

This gives consumers a stable macro event such as `issue_status_changed` while still allowing richer transition events such as `customer_archived`.

Unlisted transitions are invalid; if an enum value is reachable, model the transition that reaches it.

## Rules

A rule belongs to the feature that owns the command or workflow being denied.

```lazuli
rule "archived customers cannot be reassigned"
  deny Customer.reassign when customer.status = CustomerStatus.archived
  message "Cannot reassign an archived customer"
```

Rules are automatic preconditions. The runtime evaluates matching rules before the target command or workflow transition executes; authors do not manually call them from command code.

`Customer.reassign` is an operation reference. In canonical v0, operation matching is exact:

- `Customer.reassign` matches `command reassign` on `Customer`.
- `Customer.archive` matches a workflow transition named `archive` on `Customer`.
- `Customer.reassign` does not match `Customer.bulk_reassign`.

Policies answer "who may attempt this operation?" Rules answer "is this operation valid in the current domain state?" A typical generated flow is: decode input, derive fields, load target, check policy, evaluate rules, execute operation, publish events.

Cross-feature predicates are allowed, but the target operation owner should own the guard:

```lazuli
feature invoice
  rule "archived customers cannot receive invoices"
    deny Invoice.create when customer.status = CustomerStatus.archived
```

Avoid placing this rule in `feature customer`, because enforcement happens when `Invoice.create` runs.

## Predicate Expressions

Rule predicates, query filter predicates, and command/workflow guards share one small predicate language. The full set:

- Equality: `=`, `!=`
- Membership: `has` (collection contains element)
- Composition: `AND`, `OR`
- Operands: paths (`customer.status`, `params.id`, `ctx.user.org`), enum literals (qualified or unqualified where unambiguous), strings, integers, `nil`

Everything else is intentionally rejected:

- `NOT` — invert with `!=` or restate the rule.
- `<`, `>`, `<=`, `>=` — use a server-side validator or a raw query.
- Arithmetic — same as ordered comparisons; this is not an expression engine.
- Functions like `length`, `is_null`, `lower` — same.
- Aggregations — same.

`is_nil` is not a function; use `= nil` and `!= nil`.

The ceiling is fixed by design. If a feature needs richer logic, it is leaving the declarative path; reach for `extensions.server validate_*: Validator[...]` or a raw query.

## Enum Literals

Enum values may be unqualified where the type is obvious:

```lazuli
status: CustomerStatus = lead

workflow lifecycle on Customer.status
  archive: active -> archived
```

In free predicates, prefer qualified literals:

```lazuli
deny Customer.reassign when customer.status = CustomerStatus.archived
```

Enum values may reserve explicit storage mappings for adapters or legacy schemas:

```lazuli
enum IssueStatus
  backlog = 0
  todo = 10
```

If no explicit value is given, the semantic value is the identifier.

## Events

Events are external contracts, so their payloads should be visible in canonical v0. Lazuli does not implicitly add `<feature>_id` to every event.

When many events for one resource share the same envelope fields, declare that envelope explicitly on the resource:

```lazuli
resource Customer
  tenancy org

  event_payload
    customer_id = id
    org_id = org.id

event customer_archived
  by_id: ID
```

The event contract expands to:

```lazuli
event customer_archived
  customer_id: ID
  org_id: ID
  by_id: ID
```

`event_payload` is explicit sugar, not hidden magic. It belongs to the resource whose events share the envelope. `lazuli expand` and `lazuli inspect` must show the fully expanded event payload.

```lazuli
event customer_archived
  customer_id: ID
  by_id: ID
```

Use per-event fields for data specific to that event. Use `event_payload` only for stable, repeated envelope fields such as resource id, tenant id, or actor id.

## Policies

Policy atoms such as `role_admin`, `role_sales`, and `same_org` are semantic references. They must resolve through the project policy registry, auth adapter, or another imported feature before code generation.

The capsule can reference them, but the analyzer should validate that they exist.

`system` is reserved for internal work without an end-user actor: event consumers, webhooks after verification, scheduled jobs, queues, generated maintenance operations, and field writes performed by those operations. A project should not redefine `system` as an ordinary user role.

A feature that only consumes events or runs jobs may omit `policies` if every operation declares `policy system` inline. Add a `policies` block only when the feature needs reusable policy categories.

Policy names have two layers:

- Project/global atoms such as `role_admin`, `same_org`, `public`, `none`, and reserved `system`.
- Feature-local policy categories such as `create`, `update`, `import`, `login`, and `global_read`.

`policy update` inside `feature customer_tags` refers to that feature's local `update` policy category. It does not call `customer.policies.update` unless explicitly feature-qualified in a future extension.

## Surfaces

Read views consume query sources. A view does not need to restate `policy read` if the source query is scoped and the feature has a `read` policy.

```lazuli
view detail SidePanel
  source query.by_id(id: route.id)
```

Write forms submit commands:

```lazuli
view login Form
  submit command.login
  fields email, password
```

Use `source` for data loading and `submit` for write targets. Avoid `source command.*`; it overloads source with two directions of data flow.

The compiler should surface this derivation in `explain`.

`filter` inside a view describes UI controls. `filters` inside a query describes data predicates. The view filter names should be backed by query params and query filters when they affect server-side data.

### Cross-Feature View Composition

A feature may extend a view owned by another feature when it owns an adjacent capability:

```lazuli
feature customer_tags
  uses customer

  extends customer.surface.web.admin.view.detail
    block ext.tag_editor
```

Use the fully qualified target path in canonical `.lzi`: `<feature>.surface.<target>.<area>.view.<name>`. The extending feature owns the inserted block and its extension implementation; the target feature still owns the base view.

The target view type determines which slots are accepted. For example, `SidePanel` may accept `block`, while `Table` may accept `cells`. The analyzer should reject unsupported slots with a targeted diagnostic.

Cross-feature view composition should not be used to replace the base view. If a feature needs a completely different screen, create its own view or an explicit `escape_route`.

## Event Consumers, Webhooks, And Jobs

`on <feature>.<event>` declares a reaction to another feature's event:

```lazuli
on customer.customer_archived
  policy system
  runs ext.send_archive_survey
```

An event-consumer feature may have no resources of its own. That is valid when the feature owns only reactions or external effects.

Webhook handlers are explicit inbound edges from the outside world. In canonical v0, webhooks should verify and then run a server extension:

```lazuli
webhook stripe_invoice_paid
  path "/webhooks/stripe/invoice-paid"
  verify ext.verify_stripe_signature
  idempotency provider_event_id
  policy system
  runs ext.record_stripe_invoice_paid
  emits invoice_paid
```

Do not make webhook writes magical. Real webhook handling usually needs provider-specific verification, idempotency, upsert, and conflict-resolution behavior.

Jobs are internal asynchronous work:

```lazuli
job process_import
  queue customer_imports
  policy system
  runs ext.process_import

job recompute_scores
  schedule "0 2 * * *"
  policy system
  runs ext.recompute_scores
```

`queue` means triggered/background processing; `schedule` means cron-like recurring processing. A job should use exactly one unless a later adapter explicitly supports both.

## Auth

`auth` is a block because authentication is a family of related subcontracts: identity, password verification, OAuth adapters, MFA, session storage, refresh behavior, and rate limits.

```lazuli
auth
  identity Customer.email

  password
    hash ext.hash_customer_password
    verify ext.verify_customer_password
    rate_limit "5 per 10 minutes"

  sessions
    resource CustomerSession
    ttl "7 days"
    refresh false
```

Use a separate feature such as `customer_auth` when authentication is its own product capability. Do not mix auth into the core entity feature just because it references the same resource.

## Extensions

An extension without `at` uses feature-local convention:

```lazuli
extensions
  client status_cell: CellRenderer[Customer]
  server before_create: Hook[CreateCustomer]
```

For `features/customer/customer.lzi`, the default locations are:

```txt
features/customer/ui/status_cell.*
features/customer/hooks/before_create.*
```

The full convention table:

| Contract                           | Default Path                                |
|------------------------------------|---------------------------------------------|
| `client <name>: CellRenderer[X]`   | `features/<feature>/ui/<name>.tsx`          |
| `client <name>: ViewBlock[X]`      | `features/<feature>/ui/<name>.tsx`          |
| `client <name>: FormField[X]`      | `features/<feature>/ui/<name>.tsx`          |
| `server <name>: Hook[X]`           | `features/<feature>/hooks/<name>.go`        |
| `server <name>: Validator[X]`      | `features/<feature>/hooks/<name>.go`        |
| `server <name>: Function[X, Y]`    | `features/<feature>/domain/<name>.go`       |
| `server <name>: QueryModifier[X]`  | `features/<feature>/queries/<name>.go`      |
| `server <name>: BackgroundJob[X]`  | `features/<feature>/jobs/<name>.go`         |
| `server <name>: WebhookAdapter[X]` | `features/<feature>/integrations/<name>.go` |

Use `at` only when the implementation lives outside the convention.

The convention is part of the IR ABI (see `ir-abi.md`): changing a default path is a major bump. Adding a contract type is a minor bump.

## Identity Across Renames

When a command, transition, query, field, or resource is renamed, downstream artifacts that relied on its identity (event lineage, deploy plans, semantic diffs, persisted job data) lose continuity. `previously` declares continuity explicitly:

```lazuli
command register previously create
  creates Customer
  ...

workflow lifecycle on Customer.status
  ship previously deliver: ready -> shipped

resource Account previously Customer
  ...

resource Customer
  lifecycle_stage: CustomerStatus previously status = lead
```

The `previously` clause carries one or more prior names. The compiler records them on the IR node as `previous_names`. The planner, MCP, and semantic diff respect the link instead of treating the rename as drop-and-create.

`previously` is a migration tool. Use it when continuity matters. Do not use it as a versioning hint or a design alias for documentation; commentary belongs in `<feature>.ctx.md`.

`previously` does not chain implicitly. To preserve identity across multiple renames, list each prior name:

```lazuli
command register previously create, signup
```

## Reserved For Later

These are intentionally not solved by the simple canonical syntax yet:

- Many-to-many relations with payload or ordering. Use an explicit join resource.
- Raw query SQL verification beyond declared params/scope/returns.
- Workflow transition groups such as `any -> canceled`.
- Non-exact rule matching such as matching both `reassign` and `bulk_reassign`.
