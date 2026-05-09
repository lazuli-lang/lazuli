# Lazuli Canonical Semantics

This is the short spec for canonical `.lzi` files. The goal is a single authoring voice: explicit enough for agents and compilers, still readable by humans.

## Canonical Shape

A feature is organized by responsibility:

```lazuli
feature customer
  purpose "CRM customers within an org."

  non_goals
    invoice: "invoicing"

  defaults
    tenancy org
    timestamps

  uses org, user

  domain
    resource Customer
    query list
    event customer_created

  policies
  auth
  command create
  workflow lifecycle on Customer.status
  job recompute_scores
  webhook crm_customer_upsert
  surface web admin
  extensions
  escape_route "/admin/customer-debug"
```

The canonical form avoids compact aliases. Use `domain`, `resource`, `query`, `policies`, `command`, `workflow`, `surface`, and `extensions` explicitly. Domain declarations include enums, resources, constraints, queries, rules, and events.

Feature blocks have a canonical lint/format order:

```txt
meta: purpose, non_goals, context
defaults
uses
domain (enums, resources, constraints, queries, rules, events)
policies
auth
commands
workflows
jobs
webhooks
surfaces
extensions
escape_routes
```

Authors may draft in any order, but `lazuli fmt` should reorder blocks and `lazuli check --strict` should report non-canonical ordering. This is intentionally predictable for LLM context scans and semantic diffs.

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
target query.by_id(id: input.id)
source query.list
```

Cross-feature references must be feature-qualified and the referenced feature must appear in `uses`:

```lazuli
target customer.query.by_id(id: input.customer_id)
extends @customer_detail
```

The qualifier is the feature id, not a generated package name. Canonical `.lzi` should not rely on import-like aliasing in v0.

`<feature>.query.<name>` is the canonical cross-feature query path. The short `query.<name>` form always resolves inside the current feature.

When one feature calls another feature's query, the provider query's effective scope still applies. The caller's policy authorizes the caller's operation; the provider feature's query scope preserves the provider's data boundary. `explain` should show both edges:

```txt
customer_auth.command.enable_mfa
  policy: customer_auth.policies.update
  target: customer.query.by_id
  provider scope: customer.tenancy org + customer.soft_delete
```

Cross-feature event jobs (`job send_archive_survey` with `trigger event customer.customer_archived`) do not move ownership of the producer event. The producer owns the event contract; the consumer owns its reaction.

`uses` is strict in canonical v0. Every listed feature should be referenced by type, query, command, event, view extension, or another semantic edge in the capsule. Do not use `uses` for conceptual prose dependencies; put those in `purpose`, `non_goals`, or `<feature>.ctx.md`.

## Context Files

The standard long-form context file is co-located beside the capsule as `<feature>.ctx.md`. It is source prose, not generated frontend/backend output.

Use inline `purpose` and `non_goals` for short metadata. Use `<feature>.ctx.md` for history, gotchas, performance notes, decision logs, and narrative examples. Do not duplicate schema, operations, policies, rules, events, or extension contracts there.

`non_goals` is a small dictionary of boundary reasons. Prefer a feature id when the boundary is another Lazuli feature, and use `anti_pattern.<slug>` for a design boundary that is not owned by another feature:

```lazuli
non_goals
  customer_auth: "customer login and MFA"
  anti_pattern.generic_etl: "generic ETL platform"
```

Referenced features in `non_goals` are validated as feature ids, but they do not count as `uses`. `anti_pattern.*` entries are intentionally not feature references. These entries document boundaries; they are not semantic dependencies.

`context` is only an override when the convention is not enough:

```lazuli
feature customer
  context "@docs/customer/customer.ctx.md"
```

The compiler may validate that the referenced file exists and `lazuli inspect` may aggregate it, but Lazuli should not rewrite the markdown file.

## Resources And Relations

Fields are declared as `name: Type modifier`. Required/optional should be visible for non-default cases.

Use feature-level `defaults` to remove repeated resource traits:

```lazuli
feature customer
  defaults
    tenancy org
    timestamps

  domain
    resource Customer
      name: Text required
```

This expands each resource in the feature as if it declared `tenancy org` and `timestamps`. Resource-local declarations override defaults. Use explicit resource-level declarations when the feature is mixed. Opt out with `tenancy none` or `no_timestamps` when a resource intentionally breaks the default.

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

Resource-local custom validation may be attached inline when the validator is single-use and belongs to that resource:

```lazuli
resource ImportRow
  raw: JSON required
  validate "./domain/validate_row.go"

resource Customer
  tier: CustomerTier = free
  validates tier "./hooks/validate_tier.go"
```

Use `validate "./path.go"` for whole-resource validation and `validates <field> "./path.go"` for field-level validation. The different verbs are intentional: `validate` runs against the resource write as a whole, while `validates <field>` is scoped to writes that touch that field. Reusable validators should still live in `extensions server` and be referenced by name where needed.

## Formatting

Canonical `.lzi` style is deliberately boring so agents can copy it safely:

- Separate major child blocks (`resource`, `query`, `event`, `command`, `job`, `view`) with one blank line.
- Keep scalar statements inside one block contiguous unless a nested block follows.
- Inside `query`, put one blank line between `params`, `key`/`filters`, `order`, and `paginate`.
- Inside `command`, keep caller slots first (`route`, `input`), then `target`, `let`, `policy`, the write effect (`creates`/`updates`/`deletes`), and `emits`.
- Inside `job`, keep trigger metadata (`trigger`, `queue`, `idempotency`, `retry`, `policy`) contiguous, followed by either a declarative body or a handler body.

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

Commands have two caller-facing slots: `route` and `input`.

Queries use `params`; commands do not. `route` declares locator or context values supplied by the invoking route or caller context. `input` describes the fields a user or API caller submits as the operation body. Locator values such as `route.id` or `event.customer_id` belong in `target`, not in `input`, unless the locator is genuinely entered by the caller.

Any command expression that references `route.<name>` must declare that slot:

```lazuli
command enable_mfa
  route customer_id: ID
  input
    totp_code: Text
  target customer.query.by_id(id: route.customer_id)
  policy update
  updates Customer
```

Surfaces call commands with named arguments for submitted input. They do not pass route values manually; the compiler connects matching `route` slots from the surface route context by name and should reject missing or ambiguous route slots:

```lazuli
view enable_mfa Form
  submit command.enable_mfa
  fields totp_code
```

Create commands declare caller input and then assign resource fields inside `creates <Resource>`:

```lazuli
command create
  input name, email
  policy create
  creates Customer
    name = input.name
    email = input.email
    owner = ctx.user
  emits customer_created
```

Commands that mutate an existing resource declare the resource through `updates X` or `deletes X`. They should bind their target with a lookup expression:

```lazuli
command reassign
  route id: ID
  input owner: User
  target query.by_id(id: route.id)
  policy update
  updates Customer
    owner = input.owner
  emits customer_reassigned
```

`updates Customer` names the resource being changed. The `target` line is only the lookup expression. The loaded target is available as immutable `self` inside command expressions, rules, hooks, and generated code.

`self` is the target snapshot loaded before mutation. It does not change after `updates`. If a value is derived from `self` and later used by both a write and an event payload, bind it with `let`:

```lazuli
target query.by_id(id: event.customer_id)
let new_score = ext.risk_score(self)
updates Customer
  score = new_score
emits customer_score_recomputed
  score = new_score
```

`let` values are evaluated after `target` binding and before the write effect. They may be referenced by `creates`, `updates`, and `emits`.

`on <Resource>` is reserved for non-mutating commands that still operate in the context of a resource for policy, route, or authorization purposes. Do not repeat it on mutating commands where `updates X` or `deletes X` already names the resource.

For the common case where a mutating command targets one local resource by `route.id` and the resource has a local `query by_id`, the lookup can be omitted:

```lazuli
command reassign
  route id: ID
  input owner
  policy update
  updates Customer
    owner = input.owner
```

This expands to:

```lazuli
command reassign
  route id: ID
  input owner
  target query.by_id(id: route.id)
  policy update
  updates Customer
    owner = input.owner
```

Use the explicit form when the locator is not `route.id`, the target is cross-feature, the command has multiple locator values, or the lookup query is not `by_id`.

If a resource field is `required`, a create command must provide it through a `creates` assignment, a resource default, or resource-level injection such as `tenancy`. Required fields should not be filled by invisible convention.

`creates Customer` is the create-command counterpart to `target`: it makes the write effect explicit for humans, agents, plans, and generated handlers. In canonical authoring, `creates` uses an assignment block. This removes the older `derive` primitive and gives create and update bodies the same shape.

Update and delete commands should be explicit too:

```lazuli
command update_tier
  route id: ID
  input tier: CustomerTier
  target query.by_id(id: route.id)
  policy update
  updates Customer
    tier = input.tier
```

```lazuli
command remove_tag
  input
    customer_id: ID
    tag_id: ID
  target query.assignment_by_customer_tag(customer_id: input.customer_id, tag_id: input.tag_id)
  policy update
  deletes CustomerTagAssignment
```

`deletes X` means the command removes the targeted resource. Soft-deleted resources should use the resource's soft-delete mechanism unless the adapter or command explicitly opts into hard delete later.

`updates X` means the command changes the targeted resource. In canonical authoring, use one explicit effect for every mutating command: `creates`, `updates`, or `deletes`. The analyzer should reject mutating commands with multiple effects or none. Non-mutating request/response commands may use `returns` without an effect when documented by their adapter, such as `command login`.

Commands must declare `policy` explicitly. The common mapping (`creates` -> `policy create`, `updates` -> `policy update`, `deletes` -> `policy delete`) is a generator suggestion, not an invisible semantic default. Declare a different policy when the business intent differs from the write shape, such as `assign_tag` using `policy update` even though it creates a join resource.

`input` has two canonical forms. Use the short list when every item is a field whose type can be inferred from the created or updated resource, and none of the items are locator-only metadata:

```lazuli
input name, email, tier
```

Use a typed block when inputs do not map one-to-one to target fields, when multiple resources are involved, when the input contains typed IDs, or when inference would be ambiguous:

```lazuli
input
  customer_id: ID
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

Policies answer "who may attempt this operation?" Rules answer "is this operation valid in the current domain state?" A typical generated flow is: decode input, bind route and target, check policy, evaluate rules, execute assignment blocks, publish events.

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

When many events for one resource share the same envelope fields, declare that envelope explicitly in `domain` with an event-name pattern:

```lazuli
resource Customer
  tenancy org

events customer_* on Customer
  payload
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

`events <event-pattern> on <Resource>` is explicit sugar, not hidden magic. The pattern must currently be a single trailing-wildcard event-name pattern such as `customer_*`. It applies to events in the same feature whose names match that pattern, such as `customer_created`, `customer_archived`, and `customer_score_recomputed`. It is not a payload profile name, metadata label, or global base event. `lazuli expand` and `lazuli inspect` must show the fully expanded event payload, and the analyzer should warn when an `events` pattern matches no events.

Payload expressions under `events <pattern> on <Resource>` resolve against that resource. The analyzer should warn when a payload expression references a field that does not exist on the resource after defaults such as `tenancy org` are applied. For example, `org.id` is valid only when the resource has an `org` relation from an explicit field or tenancy injection.

Inheritance is mandatory for matching events in the same feature. If an event should not carry the shared resource envelope, give it a name outside the pattern or move it to the feature that owns that different event contract. Canonical v0 intentionally has no `no_payload_inheritance` escape hatch because optional inheritance makes the shared block unreliable for readers and generators.

```lazuli
event customer_archived
  customer_id: ID
  by_id: ID
```

Use per-event fields for data specific to that event. Use shared `events ... payload` only for stable, repeated envelope fields such as resource id, tenant id, or actor id.

The analyzer should warn about emitted events with no subscribers unless the event is intentionally for logs, audit streams, or external observers:

```lazuli
event customer_webhook_received
  observability_only
  external_id: Text
```

## Policies

Policy atoms such as `role_admin`, `role_sales`, and `same_org` are semantic references. They must resolve through the project policy registry, auth adapter, or another imported feature before code generation.

The capsule can reference them, but the analyzer should validate that they exist.

`policies` is a named dictionary of feature-local policy categories:

```lazuli
policies
  create: role_admin, role_sales
  update: role_admin, role_sales
  import: role_admin, sales_ops
  read: same_org
```

Commands and workflows reference those categories by name:

```lazuli
command upload
  policy import
  creates CustomerImportBatch
```

Commands write their policy category explicitly, even when it maps directly to the standard effect category:

```lazuli
command create
  policy create
  creates Customer

command rename
  policy update
  updates Customer

command destroy
  policy delete
  deletes Customer
```

This is intentionally a little more verbose than effect-derived policy inference. A reader should not need to remember a hidden rule to know who may run a command. Any divergent business verb should still state the semantic policy inline, such as `command assign_tag` using `policy update` even though it creates a join resource.

`system` is reserved for internal work without an end-user actor: event consumers, webhooks after verification, scheduled jobs, queues, generated maintenance operations, and field writes performed by those operations. A project should not redefine `system` as an ordinary user role.

Feature-level `defaults` may include `policy <name>` for repeated operations:

```lazuli
feature customer_outreach
  defaults
    policy system
```

Local `policy` always wins. Commands should use local `policy`; feature defaults are mainly for jobs, webhooks, and resource-less system features so write commands do not accidentally become system operations.

A feature that only consumes events or runs jobs may omit `policies` when `defaults policy system` covers every operation. Add a `policies` block only when the feature needs reusable policy categories.

Policy names have two layers:

- Project/global atoms such as `role_admin`, `same_org`, `public`, `none`, and reserved `system`.
- Feature-local policy categories such as `create`, `update`, `import`, `login`, and `global_read`.

`policy update` inside `feature customer_tags` always refers to that feature's local `update` policy category, even if the command references `Customer` from the `customer` feature. Cross-feature policy references must be feature-qualified:

```lazuli
policy customer.update
```

Feature-qualified policies are an explicit semantic dependency and require the referenced feature to appear in `uses`.

Field-level policies use the same `name: predicate` punctuation as the feature policy dictionary:

```lazuli
policies
  create: role_admin, role_sales
  update: role_admin, role_sales
  read: same_org

  fields Customer
    email
      read: same_org
      write: role_admin, role_sales
```

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

Every view has an implicit stable id: `<feature>.<surface_id>.<view_name>`, where `surface_id` joins the surface words with `_`. For example, `feature customer` + `surface web admin` + `view detail` has the implicit id `customer.web_admin.detail`. Authors may override it with `id <name>` when they want a shorter public anchor.

`filter` inside a view describes UI controls. `filters` inside a query describes data predicates. The view filter names should be backed by query params and query filters when they affect server-side data.

Custom view slots may either reference a reusable extension or declare a single-use block inline:

```lazuli
view detail SidePanel
  source query.by_id(id: route.id)
  block ext.activity_timeline

view import_detail SidePanel
  source query.by_id(id: route.id)
  block import_progress: ViewBlock[ImportBatch] at "./ui/progress.tsx"
```

Use `ext.*` when the block is named in `extensions` or reused by multiple surfaces. Use the inline `block <name>: <Contract> at "<path>"` form when the implementation exists only for that slot.

### Cross-Feature View Composition

A feature may extend a view owned by another feature when it owns an adjacent capability:

```lazuli
feature customer_tags
  uses customer

  extends @customer_detail
    block ext.tag_editor
```

The target view may declare a shorter stable id:

```lazuli
view detail SidePanel id customer_detail
  source query.by_id(id: route.id)
```

Use `extends @<view_id>` when a view declares an explicit id, or `extends <feature>.<surface_id>.<view_name>` for the implicit id. The `@` form is an explicit id reference; the canonical path form is available for every view and avoids a later rename just to make extension possible. The extending feature owns the inserted block and its extension implementation; the target feature still owns the base view.

The target view type determines which slots are accepted. For example, `SidePanel` may accept `block`, while `Table` may accept `cells`. The analyzer should reject unsupported slots with a targeted diagnostic.

Cross-feature view composition should not be used to replace the base view. If a feature needs a completely different screen, create its own view or an explicit `escape_route`.

## Escape Routes

Escape routes register pages Lazuli should know about but should not govern internally. They must still declare where the file lives and the coarse security envelope:

```lazuli
escape_route "/admin/customer-debug"
  at "./pages/customer_debug.tsx"
  policy role_admin
  tenant org
```

The route implementation remains custom code. Lazuli records the route, policy, tenant axis, and source path in generated manifests so escape hatches do not become invisible security holes.

## Async Work, Webhooks, And Jobs

`job` is the canonical construct for asynchronous work. Its trigger states why the work runs, and the job name is the stable subscription/operation id used for generated code, observability, and per-environment controls.

Event-triggered jobs consume feature events:

```lazuli
job send_archive_survey
  trigger event customer.customer_archived
  idempotency event.id
  retry 3 backoff exponential
  handler "./outreach/send_archive_survey.go"
```

An event-consumer feature may have no resources of its own. That is valid when the feature owns only reactions or external effects.

This is the canonical shape for a resource-less capability feature:

```lazuli
feature customer_outreach
  purpose "Send customer lifecycle outreach from events without owning storage."

  non_goals
    notification: "notification persistence"
    customer: "customer lifecycle ownership"

  defaults
    policy system

  uses customer

  job send_welcome
    trigger event customer.customer_activated
    idempotency event.id
    retry 3 backoff exponential
    handler "./outreach/send_welcome_email.go"
```

Do not fold this kind of capability into `customer` just because it listens to customer events. A feature may be only reactions when that is the product boundary.

Scheduled jobs use a cron-like trigger:

```lazuli
job recompute_scores
  trigger schedule "0 2 * * *"
  handler "./jobs/recompute_scores.go"
```

Event jobs can also declare a queue lane when the adapter should enqueue work instead of running it inline:

```lazuli
job process_import
  trigger event customer_import_uploaded
  queue customer_imports
  idempotency event.batch_id
  retry 3 backoff exponential
  handler "./jobs/process_import.go"
  emits customer_import_completed
```

`idempotency` names the dedupe key for the trigger execution. Event-triggered jobs use the `event.*` namespace; `event.id` refers to the event-envelope id supplied by the event bus and does not need to be repeated in the authored event payload. Webhooks use the verified inbound `payload.*` namespace. Do not write bare webhook keys such as `idempotency external_id`; write `idempotency payload.external_id` so the source is explicit. `retry <count> backoff <strategy>` is declarative delivery policy; adapters should support at least `fixed` and `exponential` before accepting those strategies in strict mode.

Async snippets that omit `policy` assume the surrounding feature declares `defaults policy system`; otherwise write `policy system` inline.

`handler` may declare `returns <Type>` when the return value is semantically consumed elsewhere:

```lazuli
handler "./integrations/upsert_customer_from_crm.go" returns Customer
```

For fire-and-consume jobs whose only meaningful result is success or failure, `handler "./path.go"` is preferred. The input type is derived from the trigger event or job payload, and Go adapters should generate `func <JobName>(ctx, event) error`-style contracts.

Webhook handlers are explicit inbound edges from the outside world. In canonical v0, webhooks should verify and then run a server extension. The verifier and handler input types are derived from the webhook name by adapter convention; only the return type is written when it matters semantically:

```lazuli
webhook stripe_invoice_paid
  path "/webhooks/stripe/invoice-paid"
  verify "./integrations/stripe.go"
  idempotency payload.provider_event_id
  policy system
  handler "./integrations/record_stripe_invoice_paid.go" returns BillingWebhook
  emits invoice_paid
```

Do not make webhook writes magical. Real webhook handling usually needs provider-specific verification, idempotency, upsert, and conflict-resolution behavior.

Jobs are internal asynchronous work:

```lazuli
job process_import
  trigger event import_uploaded
  queue customer_imports
  handler "./jobs/process_import.go"

job recompute_scores
  trigger schedule "0 2 * * *"
  handler "./jobs/recompute_scores.go"
```

`trigger event` means event-consumer work. `trigger schedule` means cron-like recurring processing. `queue` is an execution lane, not the source of truth for why the job runs.

Jobs and webhooks declare their required implementation inline with `handler` (and `verify` for webhooks). Do not duplicate those handlers in `extensions`; reserve `extensions` for reusable UI renderers, hooks, validators, query modifiers, and domain functions that are referenced by name from multiple constructs.

A job chooses one body style:

```lazuli
job record_customer_created
  trigger event customer.customer_created
  idempotency event.id
  creates AuditEvent
    payload = event

job recompute_score_after_invoice
  trigger event billing.invoice_paid
  idempotency event.id
  target query.by_id(id: event.customer_id)
  let new_score = ext.risk_score(self)
  updates Customer
    score = new_score
  emits customer_score_recomputed
    score = new_score
    reason = "invoice_paid"
```

Use the declarative body for small reactions that bind targets, create resources, update resources, or emit events without custom control flow. `target` makes the loaded resource available as immutable `self`, regardless of the resource name. Resource creation belongs under `creates <Resource>` assignment blocks, resource mutation belongs under `updates <Resource>` assignment blocks, and event payload values belong under `emits <event>`. Use `let` for derived values that are used by both mutation and event payloads; do not rely on `self` changing timing between lines.

Use `handler` when the job mutates state through non-trivial IO, loops over batches, calls providers, handles partial failure, or needs custom code. A handler-backed job may still declare `emits` so the event graph remains visible, but it should not also declare `target`, `creates`, `updates`, or `deletes`.

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
| `job <name> handler`               | `features/<feature>/jobs/<name>.go`         |
| `resource <name> validate`         | `features/<feature>/domain/validate_<name>.go` |
| `resource <name> validates <field>` | `features/<feature>/domain/validate_<name>_<field>.go` |
| `block <name>: ViewBlock[X]`       | `features/<feature>/ui/<name>.tsx`          |
| `webhook <name> verify`           | `features/<feature>/integrations/<name>.go` |

Use `at` only when the implementation lives outside the convention. Missing implementation status is not encoded by `at`: `lazuli inspect` and `lazuli check --strict` determine whether the conventional file exists and whether a stub should be generated.

The convention is part of the IR ABI (see `ir-abi.md`): changing a default path is a major bump. Adding a contract type is a minor bump.

## Identity Across Renames

When a command, transition, query, field, or resource is renamed, downstream artifacts that relied on its identity (event lineage, deploy plans, semantic diffs, persisted job data) lose continuity. `previously` declares continuity explicitly:

```lazuli
command register previously create
  policy create
  creates Customer
  ...

workflow lifecycle on Customer.status
  ship previously deliver: ready -> shipped

resource Account previously Customer
  ...

resource Customer
  lifecycle_stage: CustomerStatus previously status = lead
```

`previously` is universal for renameable identifiers: resources, fields, queries, commands, workflows, workflow transitions, views, jobs, webhooks, and extension symbols may all carry it when the compiler needs identity continuity.

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
