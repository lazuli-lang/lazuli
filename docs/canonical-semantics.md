# Lazuli Canonical Semantics

This is the short spec for canonical `.lzi` files. The goal is a single authoring voice: explicit enough for agents and compilers, still readable by humans.

For agent context and first-read authoring, load `docs/quickref.md` first. This file is the fuller normative reference and includes migration notes, rationale, and tooling detail that should not be required for ordinary feature edits.

Normative wording in this file follows ordinary spec meaning: `must`/`MUST` is required for canonical v0, `should`/`SHOULD` is expected or linted unless a feature has a reason to diverge, `may`/`MAY` is optional, and `reserved` means intentionally outside v0.

## Quick Reference

Canonical feature block order:

```txt
meta -> defaults -> uses -> refs -> domain -> policies -> errors -> auth -> command -> api -> workflow -> job -> webhook -> surface -> extensions -> escape_route
```

Closed reference namespaces:

| Namespace | Meaning |
|-----------|---------|
| `@role.*` | role authorization atoms |
| `@scope.*` | authorization predicates such as same-org, owner, public, none |
| `@actor.*` | executor identities such as user, system, service |
| `@policy.*` | feature-local policy categories such as create, update, import |
| `@semantic.*` | built-in semantic types with validation/formatting |
| `@cap.*` | built-in capability types such as files, hashed values, encrypted values, and tokens |
| `@pii.*` | data-classification markers such as contact, credential, external, derived |
| `@key.*` | cryptographic key scopes such as app, tenant, user, record |
| `@client.*` | UI extension contracts |
| `@fn.*` | pure server-side functions |
| `@hook.*` | lifecycle hooks |
| `@validator.*` | validators |
| `@adapter.*` | local integration adapter extension references |
| `@query_modifier.*` | query modifiers |
| `@anchor.*` | view composition anchors |

Registry adapter package refs such as `@drusa/mercadopago` and
`@plugin/acme/serasa` are adapter source/provenance markers. They are not
general extension namespaces.

Query modes:

| Mode | Use |
|------|-----|
| `query.list <name>` | generated collection query |
| `query.lookup <name> by <field>: <Type>` | generated single-key lookup |
| `query.lookup <name>` with `params`/`key` | generated composite or reshaped lookup |
| `query.sql <name>` | SQL-backed query wrapper |

Workflow transitions may inline scalar clauses:

```lazuli
archive previously migrated deactivate: active -> archived requires @policy.delete emits customer_archived
```

`lazuli fmt --expand` rewrites inline transition clauses as child statements; `lazuli fmt --compact` may inline them again where the compact form is legal.

Execution locator namespaces:

| Context | Locators |
|---------|----------|
| command | `route.*`, `input.*`, `ctx.*`, and `target` after `target ...` |
| query | `params.*`, `ctx.*` |
| event-triggered job | `envelope.*`, `payload.*`, `ctx.*`, and `target` after `target ...` |
| webhook | `payload.*`, `ctx.*` |
| schedule job | `schedule.*`, `ctx.*` |
| rule | `self`, `ctx.*` |
| tests | `target` in command tests; `self` in rule and workflow tests; `ctx.*` |

Event-triggered jobs keep event bus metadata in `envelope.*` and authored event fields in `payload.*`. For example, use `idempotency by envelope.id` and `target query.by_id(id: payload.customer_id)`.

Test quick reference:

Inline assertions, last child of the construct. Optional by default; `--strict-tests` warns on missing tests for non-trivial policy, rules, transitions, and extensible views.

| Construct | Verbs |
|-----------|-------|
| command | authored `allows`/`denies when <predicate>`; generated `permits`/`forbids <actor>` from effective policy |
| workflow transition | `allows`/`denies from <state>`; `allows`/`denies as <actor>`; combined form |
| rule | `allows`/`denies when <predicate>` |
| extensible view | `accepted`/`rejected by <feature>` |

Tests use the same binding as the construct under test, reuse the predicate language, and do not use fixtures or mocks. Command policy matrices are derived from `policy @policy.*`; do not copy them into source. Run `lazuli test` for IR checks, or `lazuli test --runtime` for generated Go/TypeScript checks.

Inspect expansion:

```bash
lazuli inspect feature.lzi --expand=events,targets,locators,dependencies,security --format=json
lazuli inspect feature.lzi --expand=all --format=lazuli
```

`--expand` accepts `none`, `all`, or a comma-separated list of `refs`, `summary`, `locators`, `dependencies`, `security`, `events`, `targets`, `policies`, `tests`, and `defaults`. `none` is the default. `--format=json` is the canonical inspect contract; `--format=lazuli` is a human projection that re-emits an expanded `.lzi` view where possible.

## Canonical Shape

A feature is organized by responsibility:

```lazuli
feature customer
  purpose "CRM customers within an org."

  non_goals
    delegated_to
      invoice: "invoicing"

  defaults
    tenancy org
    timestamps

  uses org, user

  domain
    resource Customer
    record CustomerLtv
    query.list list
    event customer_created

  policies
  auth
  command create
  workflow lifecycle on Customer.status
  job recompute_scores
  webhook crm_customer_upsert
  extensions
  escape_route "/admin/customer-debug"
```

The canonical form avoids compact aliases. Use `domain`, `resource`, `record`, `query.<mode>`, `policies`, `command`, `workflow`, and `extensions` explicitly. `domain` may contain any subset of enums, resources, records, constraints, queries, rules, and events. Resource-less features commonly declare only events under `domain`.

Feature blocks have a canonical lint/format order:

```txt
meta: purpose, non_goals, context
defaults
uses
refs (optional reading aid; omit core namespace lists)
domain (enums, resources, records, constraints, queries, rules, events)
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

Authors may draft in any order, but `lazuli fmt` should reorder blocks and `lazuli check --security-profile strict` should report non-canonical ordering. This is intentionally predictable for LLM context scans and semantic diffs.

Inline `surface` blocks remain accepted while older fixtures migrate, but new
source should put experience/view-model declarations in `.lzx` files. A `.lzi`
feature is a domain/capability contract and should compile without UI source.

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

The kitchen-sink fixture in `examples/full-capsule/full-capsule.lzi` intentionally contains several feature blocks to show these boundaries under pressure.

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
extends @anchor.customer_detail
```

The qualifier is the feature id, not a generated package name. Canonical `.lzi` should not rely on import-like aliasing in v0.

`<feature>.query.<name>` is the canonical cross-feature query path. The short `query.<name>` form always resolves inside the current feature.

Resolution is local-first only for unqualified operation references. Lazuli does not search `uses` in declaration order for `target query.*`, `source query.*`, `submit command.*`, or other operation-like references; write the feature prefix when the target lives outside the current feature. This keeps `uses` as a dependency declaration, not an import-order namespace.

When one feature calls another feature's query, the provider query's effective scope still applies. The caller's policy authorizes the caller's operation; the provider feature's query scope preserves the provider's data boundary. `explain` should show both edges:

```txt
customer_auth.command.enable_mfa
  policy: customer_auth.policies.update
  target: customer.query.by_id
  provider scope: customer.tenancy org + customer.soft_delete
```

Cross-feature event jobs (`job send_archive_survey` with `trigger event customer.customer_archived`) do not move ownership of the producer event. The producer owns the event contract; the consumer owns its reaction.

`uses` is strict in canonical v0. Every listed feature should be referenced by type, query, command, event, view extension, or another semantic edge in the capsule. Do not use `uses` for conceptual prose dependencies; put those in `purpose`, `non_goals`, or `<feature>.ctx.md`.

## Refs And Generated Summary

`refs` is an optional authored namespace manifest for unusual feature-local reading aids. It is not an import system and does not change name resolution. Core capability namespaces are already self-identifying through their prefixes (`@role.*`, `@scope.*`, `@actor.*`, `@policy.*`, `@semantic.*`, `@cap.*`, `@pii.*`, `@key.*`) and should not be restated in source just to make a manifest. Use `lazuli inspect --expand=refs` when a human or agent needs the complete detected namespace set.

```lazuli
refs
  local: @client, @fn, @hook, @query_modifier, @anchor
```

The group names are documentation keys, not grammar. `refs` is per-feature and optional: a feature that omits it still resolves every namespace normally. `lazuli check` should warn when a present `refs` block omits a used namespace or declares a namespace not used by that same feature. If a feature omits `refs`, tooling should not warn by default; `lazuli inspect --expand=refs` can generate the manifest from the body.

`summary` is generated, not authored. It is a table of contents derived from the body and emitted by `lazuli inspect --expand=summary` for humans, agents, and context injection:

```lazuli
# inspect projection, not source
resources: Customer
queries: list, by_id, by_email
commands: create, reassign, update_tier
workflows: lifecycle(activate, pause, resume, archive)
jobs: recompute_score_after_invoice, recompute_scores
events: customer_created, customer_status_changed, customer_archived
surfaces: web/admin, web/public, mobile/sales
anchors: @anchor.customer_detail
extended_by: customer_tags, customer_import
```

Do not write a `summary` block in canonical source; the LSP warns when one appears. This avoids drift while preserving the O(1) overview that agents need.

## Reference Namespaces

Capability references use a closed `@namespace.name` catalog:

- `@role.*` for role-based authorization atoms.
- `@scope.*` for authorization predicates such as same-org, public, owner, or none.
- `@actor.*` for executor identities such as user, system, or service.
- `@policy.*` for feature-local policy categories declared in the `policies` dictionary.
- `@semantic.*` for built-in semantic types that add validation or formatting.
- `@cap.*` for built-in capability types that need runtime behavior such as files, hashed values, encrypted values, and single-use tokens.
- `@pii.*` for data classification such as contact data, credentials, external identifiers, derived risk data, or network identifiers.
- `@key.*` for cryptographic key scopes such as app-wide, tenant, user, or per-record keys.
- `@client.*` for UI extension contracts.
- `@fn.*` for pure server-side functions.
- `@hook.*` for lifecycle hooks.
- `@validator.*` for input or domain validators.
- `@adapter.*` for integration adapters.
- `@query_modifier.*` for query-scope modifiers.

`@anchor.*` is a separate view-anchor namespace for cross-feature UI composition. Any other `@...` namespace is an error unless the framework spec adds it first.

`@actor.*` keeps one vocabulary across positions. In `policy @actor.system`, it means that actor is allowed to invoke the operation. In `when @actor.user`, it is a runtime predicate that checks the current executor kind.

## Context Files

The standard long-form context file is co-located beside the capsule as `<feature>.ctx.md`. It is source prose, not generated frontend/backend output.

Use inline `purpose` and `non_goals` for short metadata. Use `<feature>.ctx.md` for history, gotchas, performance notes, decision logs, and narrative examples. Do not duplicate schema, operations, policies, rules, events, or extension contracts there.

`non_goals` is a small structured boundary section. Use `delegated_to` when another Lazuli feature owns the capability, and `out_of_scope` when the boundary is an intentional product or architecture non-goal:

```lazuli
non_goals
  delegated_to
    customer_auth: "customer login and MFA"
  out_of_scope
    generic_etl: "generic ETL platform"
```

Referenced features under `delegated_to` are validated as feature ids, but they do not count as `uses`. `out_of_scope` entries are intentionally not feature references. These entries document boundaries; they are not semantic dependencies. Earlier drafts used direct keys and `anti_pattern.*`; canonical v0 groups entries explicitly so humans and agents do not need to infer which keys are feature references.

`context` is only an override when the convention is not enough:

```lazuli
feature customer
  context "@docs/customer/customer.ctx.md"
```

The compiler may validate that the referenced file exists and `lazuli inspect` may aggregate it, but Lazuli should not rewrite the markdown file.

## Resources And Relations

Fields are declared in this canonical order:

```txt
<name> [previously migrated|alias <old_name>]: <type> [markers...] [required|optional|= default] [relation modifiers...]
```

`<type>` may itself be a capability type such as `@cap.Encrypted(...)`.
Markers such as `@pii.*` follow the type. Presence/default comes after type and
markers: use `required`, `optional`, or `= <default>`, not both presence and a
default. Relation modifiers such as `on_delete restrict` follow presence
because they qualify the relation, not the stored scalar shape. Keeping this
order stable makes field lines parseable for humans, agents, formatters, and
semantic diffs.

Type names are intentionally a closed catalog unless they resolve to a local resource, enum, or imported type. Plain scalar types include `ID`, `Text`, `Boolean`, `Integer`, `Decimal`, `Date`, `DateTime`, and `JSON`. Types that carry framework behavior are namespaced:

```lazuli
email: @semantic.Email @pii.contact required
revenue: @semantic.Money optional
password_hash: @cap.Hashed(algorithm:argon2id) optional
api_key: @cap.Encrypted(key:@key.tenant) @pii.credential optional
reset_token: @cap.Token(ttl:1h,single_use:true,store:hashed) required
file: @cap.File(max_size:25mb,accept:text/csv) required
owner: User required on_delete restrict
```

`@semantic.*` means Lazuli should apply domain validation or formatting.
`@cap.*` means the type carries platform behavior such as upload storage,
redaction, hashing, encryption, token expiry, or secret handling. `@cap.File`
declares a generated upload/storage contract and should include `max_size` and
`accept` so backend validators, React forms, and Expo upload flows share one
limit. `@pii.*` is classification metadata used by logs, event stores, exports,
and erasure workflows. `@key.*` declares key blast radius for encrypted values.
The analyzer should reject invented built-ins unless they are added to the
closed catalog or resolved through `uses`.

A built-in belongs under `@cap.*` only when it changes runtime handling in at least two target families, such as Go persistence, React forms, Expo/mobile upload flows, generated API serialization, or logs/redaction. Prefer `@semantic.*` for pure validation/formatting and a project extension for single-target behavior.

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
  validates resource "./domain/validate_row.go"

resource Customer
  tier: CustomerTier = free
  validates field tier "./hooks/validate_tier.go"
```

Use `validates resource "./path.go"` for whole-resource validation and `validates field <name> "./path.go"` for field-level validation. The first runs against the resource write as a whole; the second is scoped to writes that touch that field. Earlier drafts used `validate "./path.go"` and `validates <field> "./path.go"`; canonical v0 uses one verb with an explicit scope. Reusable validators should live under `extensions` as `validator <name>: Validator[...]` and be referenced by name where needed.

## Formatting

Canonical `.lzi` style is deliberately boring so agents can copy it safely:

- Separate major child blocks (`resource`, `query`, `event`, `command`, `job`, `view`) with one blank line.
- Keep scalar statements inside one block contiguous unless a nested block follows.
- Inside `query`, put one blank line between `params`, `key`/`filters`, `order`, and `paginate`.
- Inside `command`, keep caller slots first (`route`, `input`), then `target`,
  `let`/`validate`, `policy`, the write effect
  (`creates`/`updates`/`deletes`), and `emits`.
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

Query declarations carry their execution mode in the declaration keyword:

- `query.list <name>` for generated collection queries.
- `query.lookup <name>` for generated single-record lookups with `key`.
- `query.sql <name>` for externally authored SQL wrappers.

Structured queries have four common parts:

- `params`: caller-provided inputs.
- `key`: record identity for single-record queries.
- `scope`: extra safety boundary beyond inherited tenancy/soft-delete scope.
- `filters`: query predicates, either always applied or guarded with `when`.
- `modifier`: optional named query modifier extension for non-declarative scoping or ranking logic.

```lazuli
query.list list
  modifier @query_modifier.query_scope_modifier

  params
    status: IssueStatus optional
    label: Label optional

  filters
    status when params.status
    labels has params.label when params.label

  order updated_at desc
  paginate 100
```

`paginate <n>` declares the default generated page size for list queries. It is
not a hard product maximum by itself; adapters may add project-wide maximums
later. Use `paginate` rather than a generic number so generated APIs, views, and
inspectors agree that the value is pagination shape, not arbitrary limit logic.
`paginate` belongs only to `query.list` and must be a positive integer.

`query.list` defaults to `order created_at desc` when no explicit `order` is
declared. This is a language convention, not project sugar: authors write an
`order` line only when the query intentionally differs from newest-first
resource listing. `lazuli inspect --expand=defaults` reports the generated
query-order default with `origin: "language default"`.

`query.list` also derives a narrow set of language-managed filter indexes from
simple equality filters. The compiler may generate an index for:

- `status when params.status` -> `status`
- `status = params.status` -> `status`
- `customer.id = params.customer_id` -> `customer`

If the feature has a single tenant axis such as `tenancy org`, the generated
index is tenant-aware (`org, status`, `org, customer`). The compiler does not
derive indexes for `has`, `!=`, `nil`, declarative `search`, `scope override`,
`query.sql`, or custom modifiers.
Those cases require explicit index design. `lazuli inspect --expand=defaults`
reports derived filter indexes; authored duplicate `index` lines should be
omitted.

Inherited scope is always applied unless a query explicitly uses `scope override`. Local `scope` extends inherited scope and should be reserved for safety boundaries. `filters` describe data predicates. A filter without `when` is always applied. A filter with `when` is conditional.

```lazuli
filters
  parent.id = params.parent_id
  status when params.status
  labels has params.label when params.label
```

`status when params.status` means "apply `status = params.status` only when the param is present." For collection fields, name the operation: `labels has params.label when params.label`.

Use the short filter form when the field name and param name are the same. Use the explicit form when they differ or when the predicate needs a path or operation:

```lazuli
filters
  lifecycle_stage when params.lifecycle_stage
  parent.id = params.parent_id
```

Path expressions are allowed in query filters:

```lazuli
filters
  parent.id = params.parent_id
```

This means "the related parent record has this id"; it is not a new field named `parent.id`.

Use `search` for textual search instead of disguising a contains query as
equality:

```lazuli
params
  search: Text optional

search params.search over name, email
  mode contains
```

`search` is not a predicate expression. It is a query capability with its own
indexing and adapter requirements. The initial v0 mode is `contains`; adapters
may later add full-text or provider-specific modes explicitly.

### `key` And `scope`

`key` identifies the requested record. Inherited and local scope prove the current actor may see that record.

```lazuli
query.lookup by_id by id: ID
```

`key` alone never means "unscoped lookup." The effective scope for a tenanted, soft-deleted resource still includes the inherited tenant and soft-delete predicates.

Use the one-line lookup shorthand when a lookup has exactly one param and the key field has the same name:

```lazuli
query.lookup by_email by email: @semantic.Email
```

Keep the expanded form when the lookup has a composite key or when param names differ from key fields:

```lazuli
query.lookup assignment_by_customer_tag
  params
    customer_id: ID
    tag_id: ID

  key customer.id = params.customer_id
  key tag.id = params.tag_id
```

Use `filters` for required caller predicates:

```lazuli
query.list sub_issues
  params
    parent_id: ID

  filters
    parent.id = params.parent_id
```

This keeps local `scope` reserved for safety boundaries.

Use `scope override` only for an explicitly cross-tenant or admin query:

```lazuli
query.list global_audit
  policy @policy.global_read

  scope override
    reason "Global audit intentionally crosses tenant scope."
    deleted_at = nil
```

An override disables inherited tenancy scope and requires both an explicit query `policy @policy.*` and a `reason "..."` child under `scope override`.
It is an absolute replacement of inherited safety scope, not a filter reset. If a future syntax such as `scope override(org)` proves necessary, it should be introduced as a stricter spelling of the same dangerous operation rather than as a second scoping model.

Generated list/lookup queries may declare cache shape:

```lazuli
query.list list
  params
    search: Text optional

  cache
    key customer.list(params)
    ttl "5 minutes"
```

`cache` is a cross-stack contract, not a Redis/provider choice. Lazuli owns the
stable key expression and stale-time semantics so React, Expo, server loaders,
and tests can agree on query identity. Drusa/runtime owns the actual cache
implementation.

### SQL Queries

`query.sql` means the query bypasses Lazuli's declarative query builder and is backed by an external SQL file. Lazuli can connect it, type it, and record it in the graph, but it cannot fully analyze the SQL body.

```lazuli
query.sql lifetime_value
  returns CustomerLtv[]

  scope
    org = ctx.user.org

  sql "./queries/customer_lifetime_value.sql"
```

SQL queries still need `params`, `scope`, and a declared return type. The SQL can be hand-written, but the capsule must keep tenant and soft-delete boundaries visible because Lazuli should not silently rewrite arbitrary SQL.

The declaration mode is the semantic fork: `query.list` and `query.lookup` are generated and analyzable, while `query.sql` is an externally implemented query wrapper. `lazuli inspect` should expose the resolved kind as `list`, `lookup`, or `sql` so generators do not infer it from body shape or prose.

`returns` types for SQL queries must resolve before code generation. They may be local `record`/resource types, extension contracts, or adapter-provided external types, but they are not inferred from the SQL file. Use `record` for non-persisted DTO/result shapes owned by the feature:

```lazuli
record CustomerLtv
  customer_id: ID
  amount: @semantic.Money
  currency: Text

query.sql lifetime_value
  returns CustomerLtv[]
  sql "./queries/customer_lifetime_value.sql"
```

`record` is not a resource: it has fields and types, but no tenancy, policies, commands, lifecycle hooks, storage migrations, or generated CRUD. It exists so SQL wrappers, handlers, and UI surfaces have a checked shape instead of a free-floating type name.

`modifier @query_modifier.*` runs after inherited tenancy/soft-delete scope, after local `scope`, and after `filters`. It cannot remove inherited or local safety predicates; use `scope override` when a query intentionally disables inherited scope. Typical uses are ordering, ranking, provider-specific computed fields, or appending extra predicates that the fixed predicate language cannot express.

## Commands

Commands have two caller-facing slots: `route` and `input`.

Queries use `params`; commands do not. `route` declares locator or context values supplied by the invoking route or caller context. `input` describes the fields a user or API caller submits as the operation body. Locator values such as `route.id` belong in `target`, not in `input`, unless the locator is genuinely entered by the caller.

The names intentionally mirror channels rather than types: query `params` are read arguments, command `route` slots are path/context locators, and command `input` slots are submitted body fields. Keep these three namespaces separate even when their field shapes look similar.

Any command expression that references `route.<name>` must declare that slot:

```lazuli
command enable_mfa
  route customer_id: ID
  input
    totp_code: Text
  target customer.query.by_id(id: route.customer_id)
  policy @policy.update
  creates CustomerMfaConfig
    customer = target
    verified_at = ctx.now
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
  policy @policy.create
  creates Customer from input
    owner = ctx.user
  emits customer_created
```

`creates <Resource> from input` copies every named input slot with the same field name onto the created resource. The assignment block remains available for context fields and overrides. The example above expands to:

```lazuli
creates Customer
  name = input.name
  email = input.email
  owner = ctx.user
```

`from input` must consume caller data explicitly. Every input slot must either match a field on the created resource and be copied by `from input`, or be referenced by an assignment in the same `creates` block. Unconsumed inputs are an error so commands do not silently drop submitted data.

Commands that mutate an existing resource declare the resource through `updates X` or `deletes X`. They should bind their target with a lookup expression:

```lazuli
command reassign
  route id: ID
  input
    owner_id: User.ID required
  target query.by_id(id: route.id)
  let resolved_owner = user.query.by_id(id: input.owner_id)
  policy @policy.update
  updates Customer
    owner = resolved_owner
  emits customer_reassigned
    to_owner_id = input.owner_id
```

`updates Customer` names the resource being changed. The `target` line is only the lookup expression. The loaded record is available as immutable `target` inside command and job expressions.

`target` is the snapshot loaded before mutation. It does not change after `updates`. If a value is derived from `target` and later used by both a write and an event payload, bind it with `let`:

```lazuli
target query.by_id(id: payload.customer_id)
let new_score = @fn.risk_score(target)
updates Customer
  score = new_score
emits customer_score_recomputed
  score = new_score
```

`let` values are evaluated after `target` binding and before the write effect. They may be referenced by `creates`, `updates`, and `emits`.

Reusable command validators are blocking when authored with `validate`:

```lazuli
command enable_mfa
  route customer_id: ID from ctx.customer.id
  input
    totp_code: Text required
  target customer.query.by_id(id: route.customer_id)
  validate @validator.verify_customer_totp(customer_id: route.customer_id, code: input.totp_code)
  policy @policy.update
  creates CustomerMfaConfig
    customer = target
    verified_at = ctx.now
```

If a command binds a validator result with `let`, the result must be used by a
`requires <binding>` guard. Computing validator output without using it is a
diagnostic because the command may continue after failed validation.

Commands may also declare temporal write-window guards:

```lazuli
command create_invoice
  input customer, amount_cents, currency, issued_at
  write_window by input.issued_at within billing.open_period
  policy @policy.create
  creates Invoice
    issued_at = input.issued_at
```

`write_window` is deliberately generic. It models "this write is only allowed
for a date that belongs to an open operational window"; it is not fiscal-period,
accounting, payroll, or inventory-specific syntax. Drusa packs define concrete
windows and adapters enforce them. Lazuli keeps the command contract visible to
doctor/check/codegen.

`on <Resource>` is reserved for non-mutating commands that still operate in the context of a resource for policy, route, or authorization purposes. Do not repeat it on mutating commands where `updates X` or `deletes X` already names the resource.

For the common case where a mutating command targets one local resource by `route.id` and the resource has a local `query.lookup by_id`, the lookup can be omitted:

```lazuli
command update_tier
  route id: ID
  input
    tier: CustomerTier required
  policy @policy.update
  updates Customer
    tier = input.tier
```

This expands to:

```lazuli
command update_tier
  route id: ID
  input
    tier: CustomerTier required
  target query.by_id(id: route.id)
  policy @policy.update
  updates Customer
    tier = input.tier
```

This inference is deliberately narrow: it requires `route id: ID`, a local mutating effect (`updates` or `deletes`), and a local `query.lookup by_id`. Use the explicit form when the locator is not `route.id`, the target is cross-feature, the command has multiple locator values, or the lookup query is not `by_id`.

If a resource field is `required`, a create command must provide it through a `creates` assignment, a resource default, or resource-level injection such as `tenancy`. Required fields should not be filled by invisible convention.

`creates Customer` is the create-command counterpart to `target`: it makes the write effect explicit for humans, agents, plans, and generated handlers. In canonical authoring, `creates` uses an assignment block. This removes the older `derive` primitive and gives create and update bodies the same shape.

Update and delete commands should be explicit too:

```lazuli
command update_tier
  route id: ID
  input
    tier: CustomerTier required
  target query.by_id(id: route.id)
  policy @policy.update
  updates Customer
    tier = input.tier
```

```lazuli
command remove_tag
  input
    customer_id: ID required
    tag_id: ID required
  target query.assignment_by_customer_tag(customer_id: input.customer_id, tag_id: input.tag_id)
  policy @policy.update
  deletes CustomerTagAssignment
```

`deletes X` means the command removes the targeted resource. Soft-deleted resources should use the resource's soft-delete mechanism unless the adapter or command explicitly opts into hard delete later.

`updates X` means the command changes the targeted resource. In canonical authoring, use one explicit effect for every mutating command: `creates`, `updates`, or `deletes`. The analyzer should reject mutating commands with multiple effects or none. Non-mutating request/response commands may use `returns` without an effect when documented by their adapter, such as `command login`.

Commands must declare `policy` explicitly. The common mapping (`creates` -> `policy @policy.create`, `updates` -> `policy @policy.update`, `deletes` -> `policy @policy.delete`) is a generator suggestion, not an invisible semantic default. Declare a different policy when the business intent differs from the write shape, such as `assign_tag` using `policy @policy.update` even though it creates a join resource.

`input` has two canonical forms. Use the short list only when every item maps one-to-one to a field on the single local resource named by `creates` or `updates` in the same command:

```lazuli
input name, email, tier
```

Short inputs inherit the resource field's type and requiredness. If the caller-facing shape differs from the stored field, use a typed block instead.

There is no `input from Customer:` syntax in canonical v0. The inference source is already constrained by the command effect (`creates Customer` or `updates Customer`), and `lazuli inspect` can expose the expanded input types without adding another authored annotation that could drift.

Use a typed block when inputs do not map one-to-one to `creates` or `updates` fields, when the command only `returns`, when the command only `deletes`, when multiple resources are involved, when the input contains locator IDs, or when inference would be ambiguous:

```lazuli
input
  customer_id: ID
  customer: Customer
  tag: CustomerTag
```

The analyzer reports short-list inputs that have no local `creates` or `updates` resource, or that are not fields on that resource. This keeps `input name, email` as a compact resource-field shorthand while forcing route ids, provider payloads, passwords, adapter fields, optional overrides, delete locators, and join locators into the typed block form.

Commands may return typed data when the operation is intentionally request/response rather than pure fire-and-redirect:

```lazuli
command login
  input
    email: @semantic.Email
    password: Text
  policy @policy.login
  returns AuthSession
```

This is not auth-specific; generated adapters should expose the return type in the client API. Prefer events for domain side effects and `returns` for immediate caller data.

Use `returns` when the caller needs immediate response data that is not simply the updated resource shape, such as an auth session, generated download URL, preview payload, or import validation summary. Do not use `returns` as a substitute for events.

Commands that affect cached queries may declare invalidation targets:

```lazuli
command reassign
  route id: ID
  input
    owner_id: User.ID required
  target customer.query.by_id(id: route.id)
  policy @policy.update
  updates Customer
    owner = input.owner_id
  invalidates
    customer.query.list
    customer.query.by_id(id: route.id)
```

`invalidates` is intentionally query-shaped. It tells generated clients and
server loaders which query identities become stale after a command succeeds.
It does not choose React Query, Redis, HTTP cache headers, or any provider.

Feature-level error exposure is explicit:

```lazuli
errors
  default hide
  expose client 4xx message, code
  expose client 5xx code
```

Rules or commands may define named public error cases:

```lazuli
rule "deleted customers cannot be archived"
  deny Customer.archive when self.deleted_at != nil
  error CustomerAlreadyDeleted status 409 expose message, code
  message "Cannot archive a deleted customer"
```

The public contract separates developer diagnostics from client payloads.
Adapters may log stack traces and internal details, but generated clients only
receive what the error contract exposes.

## Custom APIs

Use `api` for typed HTTP boundaries that are not semantic commands, queries, or
webhooks: streaming, file downloads, provider proxies, health-ish product
endpoints, and request/response handlers that need raw HTTP shape.

```lazuli
api customer_export
  method GET
  path "/api/customers/export"
  output @cap.File
  policy @policy.global_read
  rate_limit "10 per hour per user"
  handler "./api/export_customers.go"

api customer_summary_stream
  method POST
  path "/api/customers/:id/summary/stream"
  route id: Customer.ID
  input
    prompt: Text required
  output stream Text
  policy @policy.read
  rate_limit "20 per hour per user"
  handler "./api/stream_customer_summary.go"
```

`api` is not a replacement for `command` or `query`. Commands model domain
writes, emit domain events, and participate in policy/rule/effect analysis.
Queries model analyzable reads. Webhooks model verified inbound provider calls.
Custom APIs model explicit HTTP shape and still keep auth, route params, output
shape, rate limits, and handler ownership visible to check/doctor/codegen.

## Workflows

A workflow owns transitions for one resource field:

```lazuli
workflow status on Issue.status
  policy @policy.update
  emits issue_status_changed

  start: todo -> in_progress
  complete: in_review -> done
```

Workflow-level `policy` and `emits` are defaults inherited by every transition.

Policy behavior:

- `policy` on the workflow is the default for all transitions.
- `requires` on a transition adds a stronger authority requirement for that transition.

```lazuli
workflow lifecycle on Customer.lifecycle_stage
  policy @policy.update

  pause: active -> paused
  archive previously migrated deactivate: active -> archived requires @policy.delete
```

`requires @policy.delete` keeps the workflow's normal policy visible while declaring that the transition needs a higher feature-local policy category. Use it for capability upgrades such as archive/delete, publish/admin, or force/manual operations. Do not use transition-level `policy` for this pattern in canonical v0.

Event behavior:

- `emits` on the workflow always fires for every transition.
- `emits` on a transition fires additionally for that transition.
- If the workflow has no `emits`, only transition-specific events fire.

This gives consumers a stable macro event such as `issue_status_changed` while still allowing richer transition events such as `customer_archived`.

Inline transition clauses:

Workflow transitions accept trailing scalar clauses on the header line. The compact form expands mechanically to the canonical form by moving each trailing clause into a child statement.

```lazuli
# compact
archive previously migrated deactivate: active -> archived requires @policy.delete emits customer_archived

# expanded
archive previously migrated deactivate: active -> archived
  requires @policy.delete
  emits customer_archived
```

Trailing clauses appear in the same order as their expanded child form: `previously` before the colon, then the state range, then `requires`, then `emits`. `lazuli fmt` enforces this order.

Only scalar clauses may be inlined. Clauses that introduce a child block, such as `tests`, stay as children:

```lazuli
activate: lead -> active emits customer_activated
  tests
    allows from lead
    denies from active
```

The mixed form is valid: `emits` is trailing because it is scalar, while `tests` remains a child block. Authors may also write the fully expanded form when they prefer. Multiple values for a single scalar clause, such as multiple transition events, require the multi-line form.

Inside a workflow transition, child statements such as `requires`, `emits`, and `tests` stay contiguous with the transition header. Do not separate them from the header with a blank line; `lazuli fmt` removes that blank.

`lazuli fmt --expand` rewrites compact transition headers to the expanded form. `lazuli fmt --compact` may inline scalar transition clauses where the compact form is legal. These are tooling modes over the same IR, not separate semantics.

Unlisted transitions are invalid; if an enum value is reachable, model the transition that reaches it.

## Inspect

`lazuli inspect` exposes effective derived views that humans, agents, dashboards, and generators can use without changing the authored `.lzi` file. It is a read-only projection, not a second source format.

JSON is the canonical inspect output:

```bash
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=events,targets,policies,locators,dependencies,security --format=json
```

The JSON root includes `schema`, `source`, `expand`, and `features`. The current schema id is `lazuli.inspect.v0`. Expansion classes are explicit and stable so agents can request only the context they need:

| Expansion | Meaning |
|-----------|---------|
| `refs` | shows authored `refs`, detected namespace use, and missing/unused namespace entries |
| `summary` | shows the generated feature index: resources, records, queries, commands, workflows, jobs, events, surfaces, anchors, extension edges, and a derived `provides` object |
| `locators` | shows the bindings available to each construct, such as `route.*`, `input.*`, `params.*`, `target`, `envelope.*`, `payload.*`, `schedule.*`, and `ctx.*` |
| `dependencies` | shows feature edges from `uses`, `emits`, `trigger event`, `extends @anchor.*`, and query references |
| `security` | shows security-relevant field and event-payload markers (`@pii.*`, `@cap.*`, `@key.*`), operation policies/rate limits/scope overrides, job `tenant_from`, and webhook verification |
| `events` | shows event payload fields after merging matching `event_group <pattern> on <Resource>` payload groups with event-local fields |
| `targets` | shows explicit and inferred command targets, including local `route id` target inference |
| `policies` | shows operation policies resolved to policy atoms and transition `requires` as additional requirements |
| `tests` | groups authored predicate/transition/anchor tests plus generated command policy authz rows by subject and assertion kind (`authz`, `transition`, `predicate`, `anchor`) |
| `defaults` | shows feature defaults such as `tenancy`, `timestamps`, and scoped `policy_for` with their affected constructs |

Every expanded item carries provenance through an `origin` field. Examples include `event_group:customer_*`, `event:customer_created`, `explicit`, `workflow.policy`, `transition.requires`, `generated from command policy @policy.create`, and `inferred from local route id and query.lookup by_id`. Provenance is part of the inspect contract; do not add expanded facts without explaining where they came from.

Each expansion should answer one bounded question. If a proposed derived fact does not fit an existing expansion class, add a new explicit class instead of silently changing `--expand=all` shape. Future useful expansion classes include `surfaces` and `extensions`; they should follow the same JSON and provenance contract when implemented.

`--format=lazuli` is useful for debugging syntax sugar:

```bash
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=all --format=lazuli
```

It may expand local sugar such as inline transition clauses, single-key lookup shorthand, `creates X from input`, inferred local targets, and inherited event payload fields. The JSON output remains the stable contract for tooling and golden tests.

## Working With Agents

Agents should treat authored `.lzi` files as the only editable source. Generated `summary` output, expanded inspect JSON, capsules, and codegen artifacts are read-only context unless a task explicitly targets those tools.

For feature edits, load the smallest stable context pack that answers the task:

```bash
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=summary,refs,events,policies,locators,dependencies,security --format=json
```

Pair that inspect payload with this spec section and `docs/invariants.md` when the agent is generating or reviewing Lazuli source. `summary` answers "what exists", `refs` answers namespace use, `events` answers payload contracts, `policies` answers effective authorization, `locators` answers which bindings are in scope, `dependencies` answers cross-feature impact, and `security` answers which fields/operations/webhooks carry security obligations.

Agents should prefer explicit source changes over new sugar. If a repeated pattern is only a reading problem, add or improve an inspect expansion before changing the language. If a proposed sugar cannot be expanded mechanically and locally, keep it out of canonical v0.

## Security And Crypto Contracts

Lazuli declares security properties; adapters implement them with audited runtime libraries. Do not implement cryptographic primitives in the DSL or in generated templates beyond wiring to standard libraries/KMS providers.

Security is not authored as a feature-level `security` checklist. It is local
to the operation, field, webhook, auth flow, or escape route where the decision
is made. `lazuli inspect --expand=security` is the generated audit view.

`lazuli check` supports security profiles:

- `prototype` reports missing security contracts as warnings while drafting.
- `strict` is the default; missing security contracts are errors.
- `production` keeps strict errors and treats explicit opt-outs such as
  `verify none` or `rate_limit none` as release blockers unless future deploy
  configuration allowlists them.

`lazuli check <file>` is intentionally file-local. `lazuli doctor <file-or-dir>`
loads the capsule package (`.lzi` plus sibling/descendant `.lzx` files) and runs
cross-file diagnostics. The first package-level contract is `LZX-POL-001`:
platform surface `submit` targets and actions resolved through the abstract
experience must be reachable by the surface `audience` after command
`@policy.*` references expand to policy atoms.

Every command must declare local `policy`. Commands do not inherit permissive
effect-derived policy defaults, and `policy_for` is intentionally scoped to
jobs/webhooks.

Commands that mutate state or whose effective policy resolves to
`@scope.public` must declare a command-level `rate_limit`:

```lazuli
command create
  policy @policy.create
  rate_limit "30 per hour per ip"
```

Intentional opt-out is explicit and local:

```lazuli
command internal_bulk_import
  policy @policy.admin
  rate_limit none
    reason "Internal operator-only batch action behind controlled network."
```

Queries that use `scope override` must declare `policy @policy.*` on the query. The override replaces inherited tenant/soft-delete safety scope, so the authorization boundary must be visible at the same construct:

```lazuli
query.list global_search
  policy @policy.global_read

  scope override
    reason "Global admin search intentionally crosses tenant scope."
    deleted_at = nil
```

The `reason` is part of the authoring contract for dangerous scope replacement. It is not generated into business logic, but it appears in `lazuli inspect --expand=security` so reviewers and agents can distinguish intentional cross-tenant access from an accidental missing tenant predicate.

Sensitive fields marked with `@pii.*`, `@cap.Encrypted`, `@cap.Hashed`,
`@cap.E2ee`, or `@cap.Token` must declare field-level `read` and `write`
policy:

```lazuli
policies
  fields CustomerSession
    refresh_token_hash
      read: @actor.system
      write: @actor.system
```

Data classification uses `@pii.*` markers on fields and event payloads. The initial catalog is open only by spec update, not by ad hoc invention:

- `@pii.contact` for email, phone, address, and similar contact data.
- `@pii.credential` for OAuth tokens, API keys, and credential-like material.
- `@pii.external` for third-party identifiers.
- `@pii.derived` for scores, risk labels, or inferred sensitive facts.
- `@pii.network` for IP addresses and device/network identifiers.

Resources that store `@pii.*` fields declare retention, or inherit it from
`defaults`:

```lazuli
defaults
  retention 7y then anonymize

resource CustomerSession
  provider_access_token: @cap.Encrypted(key:@key.tenant) @pii.credential optional
  retention 30d then delete
```

Retention is a horizontal compliance contract, not an ERP or GDPR-only module.
The canonical form is `retention <duration|forever> then
delete|anonymize|archive`. Export, erasure workflows, and reviewer dashboards
belong in Drusa packs, but the retention decision belongs in source.

Event payloads may also carry `@pii.*`, `@cap.*`, or `@key.*` markers. `lazuli inspect --expand=security` exposes those markers under event payloads so cross-feature consumers can be audited without opening handler code. Consumers may only read `payload.*` fields declared by the producer event contract; the analyzer validates this across features when both producer and consumer are present in the same capsule.

Capability crypto tiers are explicit:

- `@cap.Hashed(algorithm:<name>)` is one-way material such as password hashes and refresh-token hashes.
- `@cap.Encrypted(key:@key.<scope>)` is server-readable encrypted material.
- `@cap.E2ee(key:@key.<scope>)` is ciphertext the server should store but not read.
- `@cap.Token(ttl:<duration>,single_use:true|false,store:hashed)` is generated token material such as password reset, magic link, email verification, or share-link tokens.

Capability arguments use a closed mini-grammar, not arbitrary strings:

- `<duration>` is an integer plus unit with no spaces: `30s`, `10m`, `1h`, `7d`.
- `single_use` is exactly `true` or `false`.
- `store` is `hashed` in canonical v0. Add other strategies only by spec update.
- `algorithm` for `@cap.Hashed` is `argon2id` in canonical v0; adapters may
  accept `bcrypt` only for migration/legacy compatibility.
- `key` is a `@key.*` reference such as `@key.tenant`.

Adapters may map those symbolic choices to concrete library parameters, but
source should not invent new argument keys or free-form values.

Key scopes use `@key.*`:

- `@key.app` for app-wide keys; use sparingly.
- `@key.tenant` for tenant/org isolation.
- `@key.user` for per-user isolation.
- `@key.record` for per-record or per-field data keys.

Environment variables are declared in `registry.lzi`, `app.lzi`, or a top-level
`env` block for small/standalone capsules:

```lazuli
env
  group customer_import
    server CRM_WEBHOOK_SECRET: Secret required in production
  group public_clients
    client PUBLIC_APP_URL: Url required
    mobile EXPO_PUBLIC_API_URL: Url required
```

Any `env.NAME` reference should resolve to this schema. `group <name>` is an
organizational key for humans, agents, inspect output, and Drusa wiring; it is
not a namespace. Variable names stay explicit and globally unique inside the
package env schema. `required in production` narrows the requirement to named
environments declared by the app manifest; plain `required` applies to every
environment. `server` values are not exposed to client bundles. `client`
variables use a `PUBLIC_` prefix and `mobile` variables use an
`EXPO_PUBLIC_` prefix so exposure is visible in source instead of being an
adapter convention.

Webhook verification may be declarative for common signature schemes:

```lazuli
webhook stripe_invoice_paid
  path "/webhooks/stripe/invoice-paid"
  verify hmac sha256
    secret env.STRIPE_WEBHOOK_SECRET
    header "Stripe-Signature"
  tenant_from payload.org_id
  idempotency by payload.org_id, payload.provider_event_id
```

Every webhook must declare verification and idempotency. Use
`verify "./path.go"` for provider-specific protocols that need custom code.
Use `verify none` only as an explicit security opt-out with a `reason` child;
production checks treat such opt-outs as release blockers. Declarative verify
blocks and custom verify handlers both run before idempotency and handler
execution.

`escape_route` is outside generated UI ownership, so it must keep its security
envelope visible in source:

```lazuli
escape_route "/admin/customer-debug"
  at "./pages/customer_debug.tsx"
  policy @role.admin
  tenant org
```

`auth password` must declare the password hash algorithm and credential-guessing
rate limit. `auth sessions` must declare session TTL:

```lazuli
auth
  password
    algorithm argon2id
    rate_limit "5 per 10 minutes"

  sessions
    resource CustomerSession
    ttl "7 days"
```

Queries named `active_sessions` should prove temporal validity in source:

```lazuli
query.list active_sessions
  modifier @query_modifier.active_session_scope

  params
    customer_id: ID

  filters
    customer.id = params.customer_id
    expires_at > ctx.now
```

If the temporal predicate is hidden inside a modifier, the modifier block must
declare a guarantee such as `guarantees expires_at > ctx.now`; the modifier name
alone is not enough evidence for codegen or review.

## Rules

A rule belongs to the feature that owns the command or workflow being denied.

```lazuli
rule "archived customers cannot be reassigned"
  deny Customer.reassign when self.status = CustomerStatus.archived
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
    deny Invoice.create when self.customer.status = CustomerStatus.archived
```

Avoid placing this rule in `feature customer`, because enforcement happens when `Invoice.create` runs.

## Predicate Expressions

Rule predicates, query filter predicates, and command/workflow guards share one small predicate language. The full set:

- Equality: `=`, `!=`
- Membership: `has` (collection contains element)
- Composition: `AND`, `OR`
- Operands: paths (`self.status`, `params.id`, `ctx.user.org`), enum literals (qualified or unqualified where unambiguous), strings, integers, `nil`

Everything else is intentionally rejected:

- `NOT` — invert with `!=` or restate the rule.
- `<`, `>`, `<=`, `>=` — use a server-side validator or a `query.sql`.
- Arithmetic — same as ordered comparisons; this is not an expression engine.
- Functions like `length`, `is_null`, `lower` — same.
- Aggregations — same.

`is_nil` is not a function; use `= nil` and `!= nil`.

The ceiling is fixed by design. If a feature needs richer logic, it is leaving the declarative path; reach for `@validator.*` or a `query.sql`.

## Tests

Tests are inline declarative assertions about the IR. They cover decisions that require inference: who may invoke an operation, which transitions are valid from which states, when a rule denies, and which features may extend an anchor.

Tests run against the IR before code generation. They execute quickly, depend on no fixtures, require no runtime, and are the canonical loop for authors and agents iterating on a feature.

A `tests` block is the last child of a `command`, workflow transition, `rule`, or extensible view (`view ... id @anchor.*`). Tests are not allowed in queries, events, resources, webhooks, escape routes, surfaces, or features themselves. Their content is declaration, not decision.

### Subject And Bindings

Within a `tests` block, the parent construct is the implicit subject. Tests inside `command reassign` are about `command reassign`. Tests inside a rule are about the operation that the rule denies. The subject is never restated.

Command tests use `target` for the loaded command target when the command has one. Rule and workflow tests use `self` for the resource snapshot under the rule or transition. Predicates inside tests reuse the closed predicate language: `=`, `!=`, `has`, `AND`, `OR`, paths, enum literals, strings, integers, and `nil`. Tests add no new operators or functions.

Path expressions are allowed in test predicates the same way they are allowed in query filters. `denies when self.customer.lifecycle_stage = archived` is valid in a rule test when the resource being tested has a `customer` relation; `denies when target.lifecycle_stage = archived` is valid in a command test when the command target has that field.

### Verbs By Category

Command tests authored in source accept predicate assertions:

```lazuli
tests
  allows when target.lifecycle_stage = active
  denies when target.lifecycle_stage = archived
```

`allows when`/`denies when` test rule applicability against the loaded command target.

The command actor matrix is generated from the effective command policy.
For a command with `policy @policy.create`, `lazuli inspect --expand=tests`
and runtime test generation emit derived `permits`/`forbids` rows with policy
provenance. Authors do not restate those rows in source, because they would
only duplicate the `policy @policy.*` contract.

The actor-matrix verbs are intentionally reserved for generated authorization
rows: `permits`/`forbids` always talk about authorization subjects, while
`allows`/`denies` always talk about evaluated predicates or workflow edges.

Workflow transition tests accept three forms with separate semantics:

```lazuli
tests
  allows from active
  denies from paused
  allows as @role.admin
  denies as @role.viewer
  allows from active as @role.admin
  denies from active as @role.sales
```

`from <state>` tests state-machine edges, assuming an authorized actor. `as <actor>` tests policy, assuming the transition's valid source state. The combined form tests both dimensions at the intersection. Use the right form for the property being checked; the test runner uses the workflow's policy default when only `from` is present, and the transition's source state default when only `as` is present.

Do not flatten every test form into a single generic `allow`/`deny` dialect in
canonical v0. The scoped vocabulary is deliberate. `when` means predicate
evaluation, `from` means state-machine edge, `as` means actor authorization,
and `accepted/rejected by` means anchor allowlist membership. Keeping those
shapes distinct gives the parser and LSP useful rejection power and gives
humans/agents a local clue about which semantic dimension is being tested.

Rule tests accept:

```lazuli
tests
  denies when self.tier = enterprise AND self.owner = nil
  allows when self.tier = enterprise AND self.owner != nil
```

The subject is the operation referenced in the rule's `deny` clause. Tests evaluate the rule predicate in isolation; they do not simulate the target operation. A test for the rule "deleted customers cannot be archived" verifies that the predicate fires when `self.deleted_at != nil`, not that the `archive` transition would actually fail. Transition tests in the workflow are separate and cover state-machine behavior. Both perspectives can coexist; they test different things.

Extensible views accept:

```lazuli
view detail SidePanel id @anchor.customer_detail
  extensible_by customer_tags, customer_import
  source query.by_id(id: route.id)
  block @client.activity_timeline

  tests
    accepted by customer_tags
    accepted by customer_import
    rejected by billing
```

The whitelist is owned by the target view, so tests live there. `extends` blocks on the consumer side do not declare tests; they exercise the whitelist by participation.

### Strict Rejections

Tests are intentionally narrow. The analyzer rejects:

- Assertions about emitted events, written `expects emits ...`. The command's `emits` clause is the contract; testing it is tautology.
- Assertions about write effects, written `expects creates ...` or `expects updates ...`. Same reasoning.
- Fixture data, written as `given <resource> { ... }`. Tests at this level run against the IR, not against persisted state.
- Mocks, including `mock @fn.X returns Y`. Extension behavior is tested in the extension's host language.
- `given`/`when`/`then` framing. The construct supplies subject and verb.

Multi-step scenarios that span constructs are reserved for later.

### Optional, With Strict Mode For Production

`tests` is optional in every construct. `lazuli check` accepts features without tests and emits no warning. `lazuli check --strict-tests` emits warnings for commands with target-dependent rule behavior, rules, transitions, and extensible views that lack authored tests. Plain command policy coverage is generated from `policy @policy.*`, so a command does not need an authored `tests` block merely to prove who can call it. Use `--strict-tests` in production-grade features and CI.

A command has target-dependent rule behavior when a matching rule or command-local predicate can deny the operation beyond the policy category. Constructs that inherit `@actor.system` through `policy_for jobs, webhooks` are typically internal and exempt from the strict warning.

### Two Test Layers

Tests run at two layers:

1. IR layer: `lazuli test` evaluates `tests` blocks against the IR. This is the canonical layer authors and agents iterate against.
2. Runtime layer: `lazuli test --runtime` generates `*_test.go` and `*.test.ts` files in `dist/`, then invokes `go test ./...` and the TypeScript test runner. This verifies that generated code respects the same expectations under real execution.

Authors edit only `tests` blocks in `.lzi` files. Generated runtime test files in `dist/` follow the same rule as other generated artifacts: regenerate them, do not edit them by hand.

### Migration: Rule Aliases To `self`

Canonical v0 uses `self` as the snapshot binding inside rules and workflow tests. Commands and declarative jobs use `target` for the loaded target record. Earlier drafts used a lowercased resource name in rules:

```lazuli
# before
deny Customer.reassign when customer.lifecycle_stage = archived

# after
deny Customer.reassign when self.lifecycle_stage = archived
```

## Enum Literals

Enum values may be unqualified where the type is obvious:

```lazuli
status: CustomerStatus = lead

workflow lifecycle on Customer.status
  archive: active -> archived
```

In free predicates, prefer qualified literals:

```lazuli
deny Customer.reassign when self.status = CustomerStatus.archived
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

event_group customer_* on Customer
  payload
    customer_id = id
    org_id = org.id
    by_id = ctx.user.id when @actor.user

  event archived
```

The event contract expands to:

```lazuli
event customer_archived
  customer_id: ID
  org_id: ID
  by_id: ID
```

`event_group <event-pattern> on <Resource>` is explicit sugar, not hidden magic. The name is deliberately different from `event`: the group declares a shared payload template, while nested `event` and `event.trace` children declare concrete event contracts. The pattern must currently be a single trailing-wildcard event-name pattern such as `customer_*`. A nested short event name is appended to the prefix, so `event archived` under `event_group customer_*` declares `customer_archived`. The group applies only to nested events and same-feature legacy sibling events whose names match that pattern, such as `customer_created`, `customer_archived`, and `customer_score_recomputed`. It is not a payload profile name, metadata label, or global base event. Choose event prefixes owned by the local feature/resource, such as `tag_assignment_*` inside a tag-assignment feature, so a reader does not confuse same-feature inheritance with similarly named events elsewhere. `lazuli expand` and `lazuli inspect --expand=events` must show the fully expanded event payload, and the analyzer should warn when an `event_group` pattern matches no events.

Do not repeat the group name on every `event` line, such as `event customer_created : customer_*`. That would create a second source of truth for inheritance. For isolated event documentation, use the inspect expansion; it shows inherited and event-local payload fields with provenance.

Earlier drafts used `events <pattern> on <Resource>` for the same construct and placed concrete matching events as sibling declarations below the group. Tooling may accept both as legacy aliases, but canonical v0 source should use `event_group` with nested concrete events so inheritance is visible where the event is authored.

Payload expressions under `event_group <pattern> on <Resource>` resolve against that resource. The analyzer should warn when a payload expression references a field that does not exist on the resource after defaults such as `tenancy org` are applied. For example, `org.id` is valid only when the resource has an `org` relation from an explicit field or tenancy injection.

Path expressions in payloads follow the same resolution as filter paths: a declared field, a tenancy-injected field, or a built-in resource field such as `id`. For example, `customer.id`, `tag.id`, and `org.id` may come from declared relations and tenancy injection but use the same path syntax.

Inheritance is mandatory for matching events in the same feature. If an event should not carry the shared resource envelope, give it a name outside the pattern or move it to the feature that owns that different event contract. Canonical v0 intentionally has no `no_payload_inheritance` escape hatch because optional inheritance makes the shared block unreliable for readers and generators.

```lazuli
event customer_reassigned
  to_owner_id: ID
```

Use per-event fields for data specific to that event. Prefer `by_id` for the actor, `from_*` and `to_*` for state changes, and plain `*_id` for related entities. Use shared `event_group ... payload` only for stable, repeated envelope fields such as resource id, tenant id, or actor id.

The analyzer should warn about emitted events with no subscribers unless the event is intentionally for logs, audit streams, or external observers:

```lazuli
event.trace customer_webhook_received
  external_id: Text
```

`event.trace` marks a domain signal that is intentionally not part of the feature-to-feature reaction graph. Other features should not subscribe to it unless the event is promoted back to an ordinary `event`. In strict mode, `trigger event <trace-event>` is invalid. This keeps integration logs, webhook receipt markers, and side-effect audit signals visible without making every audit signal look like missing product behavior.

`emits` works the same for `event` and `event.trace` declarations. The distinction affects subscriber warnings and reaction-graph generation, not how a command, workflow, job, or webhook publishes the signal.

Event publication is always authored. Do not infer `emits customer_reassigned` from `command reassign` or `emits customer_archived` from `archive: active -> archived`; the explicit line is the contract that creates a reaction-graph edge. Likewise, shared resource envelopes belong in `event_group ... payload`, not in hidden feature-level payload defaults. If a repeated event-envelope pattern becomes universal, Lazuli should promote it to a named language primitive instead of adding project-local macros or invisible defaults.

## Policies

Policy atoms such as `@role.admin`, `@role.sales`, `@scope.same_org`, `@actor.system`, and `@scope.public` are semantic references. They must resolve through the project policy registry, auth adapter, or another imported feature before code generation.

The capsule can reference them, but the analyzer should validate that they exist.

`policies` is a named dictionary of feature-local policy categories:

```lazuli
policies
  create: @role.admin, @role.sales
  update: @role.admin, @role.sales
  import: @role.admin, @role.sales_ops
  read: @scope.same_org
```

Commands and workflows reference those categories through the `@policy.*` namespace:

```lazuli
command upload
  policy @policy.import
  creates CustomerImportBatch
```

Commands write their policy category explicitly, even when it maps directly to the standard effect category:

```lazuli
command create
  policy @policy.create
  creates Customer

command rename
  policy @policy.update
  updates Customer

command destroy
  policy @policy.delete
  deletes Customer
```

This is intentionally a little more verbose than effect-derived policy inference. A reader should not need to remember a hidden rule to know who may run a command. Any divergent business verb should still state the semantic policy inline, such as `command assign_tag` using `policy @policy.update` even though it creates a join resource.

`@actor.system` is reserved for internal work without an end-user actor: event consumers, webhooks after verification, scheduled jobs, queues, generated maintenance operations, and field writes performed by those operations. A project should not redefine it as an ordinary user role.

Feature-level `defaults` may include `policy_for <families>: <atom>` for repeated internal operations:

```lazuli
feature customer_outreach
  defaults
    policy_for jobs, webhooks: @actor.system
```

`policy_for` is scoped by construct family. The initial v0 families are `jobs` and `webhooks`; future families must be added explicitly to the spec. Local `policy` always wins. Commands should use local `policy`; `policy_for` exists for jobs, webhooks, and resource-less system features so write commands do not accidentally become system operations.

A feature that only consumes events or runs jobs may omit `policies` when `defaults policy_for jobs, webhooks: @actor.system` covers every operation. Add a `policies` block only when the feature needs reusable policy categories.

`lazuli inspect` should show the effective policy for every command, workflow transition, job, webhook, escape route, and generated endpoint after local overrides and feature defaults are applied. Authoring keeps defaults compact; inspection makes the security surface auditable without scanning the whole feature by hand.

Policy names have two layers:

- Project/global atoms such as `@role.admin`, `@scope.same_org`, `@scope.public`, `@scope.none`, and reserved `@actor.system`.
- Feature-local policy categories referenced as `@policy.create`, `@policy.update`, `@policy.import`, `@policy.login`, and `@policy.global_read`.

Commands and workflows should always reference feature-local categories with `@policy.*`. Put `@role.*`, `@scope.*`, and `@actor.*` atoms in the `policies` dictionary, then point commands/workflows at that category. Jobs, webhooks, escape routes, and `policy_for` defaults may still use direct atoms such as `@actor.system` or `@role.admin` when no reusable category is needed.

Do not simplify command/workflow authoring to bare `policy create` or
`policy update` in canonical v0. The `@policy.*` namespace is intentionally
visible even though it costs tokens. It tells humans, agents, and tooling that
the value is a feature-local authorization category, not a write effect, a
command verb, or a terminal actor/role/scope atom. This explicit boundary is
part of Lazuli's safety model: the LSP can reject direct atoms in user-facing
commands, inspect can resolve category-to-atom provenance consistently, and a
reader never has to infer whether `create` means "the create operation" or
"the create authorization category."

`policy @policy.update` inside `feature customer_tags` always refers to that feature's local `update` policy category, even if the command references `Customer` from the `customer` feature. Cross-feature policy references must be feature-qualified:

```lazuli
policy customer.update
```

Feature-qualified policies are an explicit semantic dependency and require the referenced feature to appear in `uses`. Bare local policy names such as `policy update` are a legacy compatibility form; canonical v0 authoring uses `@policy.*` so policy category references are visually distinct from verbs and built-in actors/scopes.

Field-level policies use the same `name: predicate` punctuation as the feature policy dictionary:

```lazuli
policies
  create: @role.admin, @role.sales
  update: @role.admin, @role.sales
  read: @scope.same_org

  fields Customer
    email
      read: @scope.same_org
      write: @role.admin, @role.sales
```

## App Runtime and Routes

`app.lzi` is the project entrypoint. It declares the provider-neutral
operational contract that Drusa materializes into generated app wiring,
runtime units, deploy gates, and adapter requirements. It is not a product
feature and should not hide domain behavior.

```lazuli
app AcmeCRM
  title "Acme CRM"
  version "0.1.0"
  default_locale "pt-BR"
  default_timezone "America/Sao_Paulo"
  auth_failed_redirect public.login
  not_found public.not_found

  uses
    customer
    customer_auth
    customer_tags
    customer_import

  packs
    customer_import from registry.packs.customer_import

  bindings
    customer_import.crm = integrations.crm

  targets
    backend go
    web react
    mobile expo

  environments
    local
    staging
    production

  urls
    web local "http://localhost:3000"
    api local "http://localhost:8080"
    web production "https://app.acme.example"
    api production "https://api.acme.example"

  architecture
    mode modular_monolith
    service_ready true
    enforce_service_boundaries true

  services
    service identity
      owns customer_auth
      exposes
        command customer_auth.command.login
      publishes customer_auth.customer_*

    service crm
      owns customer, customer_tags, customer_import
      exposes
        query customer.query.by_id
        command customer.command.create
      publishes customer.customer_*
      consumes billing.invoice_paid

  communication
    internal sync rpc
    external http
    async event_bus
    propagate actor, tenant, trace_id, request_id
    timeout default "2s"
    retry default 2 backoff exponential

  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"
      readiness "/readyz"

    unit web
      serves surfaces web

    unit worker
      runs jobs *

    unit scheduler
      runs schedules *

  deploy
    migrations before_deploy
    migration_lock required
    destructive_migrations require_approval
    rollback on_failed_healthcheck
```

`registry.lzi` is the package-level catalog consumed by `app.lzi`, Drusa, and
doctor:

```lazuli
registry
  env
    group customer_import
      server CRM_WEBHOOK_SECRET: Secret required in production
    group public_clients
      client PUBLIC_APP_URL: Url required
      mobile EXPO_PUBLIC_API_URL: Url required

  capabilities
    database postgres
    queue background_jobs
    object_storage files
    mailer transactional
    event_bus internal
    tracing optional
    integration crm

  packs
    customer_import from @drusa/customer-import
      version "0.1.0"
      provides feature customer_import
      requires integration crm: CRMProvider

  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments sandbox, production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET
```

`app.lzi` declares what the app needs, not how a specific cloud provider is
configured. Concrete provider choices such as Fly, AWS, Kubernetes, Neon, R2,
Redis, SendGrid, or OpenTelemetry exporters belong in Drusa adapter
configuration.

`profile <environment>` is the environment-specific override contract. It keeps
local/staging/production differences out of `app.lzi` without putting secrets,
cloud resources, or provider operation schemas into Lazuli:

```lazuli
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    customer_import.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
    migrations before_deploy

profile production
  urls
    web "https://app.acme.example"
    api "https://api.acme.example"
  integrations
    crm environment production
  deploy
    topology split_services
    migrations before_deploy
    rollback on_failed_healthcheck
```

Profiles may override public URLs, feature-to-integration bindings, integration
environment/adapter selection, and provider-neutral deploy topology/gates. They
must not contain secret values, cloud account ids, bucket names, Kubernetes
namespaces, concrete broker URLs, provider HTTP paths, or SDK method schemas.
Those remain Drusa/adapters. `lazuli inspect` exposes profiles under
`profiles`; `lazuli doctor` validates profile names against app
`environments`, integration overrides against app/registry integrations, and
profile bindings against feature requirements.

`workspace.lzi` is optional and exists only above one app package. It describes
the semantic contract of a distributed system: local apps, external services,
shared registries, event publication/consumption edges, context propagation,
and provider-neutral public gateways. It is not a repo manager or infra file:
remote repositories, branches, local ports, deploy providers, brokers, and
proxy implementations belong in `drusa.toml` or adapter config.

```lazuli
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    billing at "./apps/billing/app.lzi"
    ai external contract "acme.ai.v1"

  shared_registry "./registry.lzi"

  boundaries
    crm publishes customer.*
    billing publishes billing.*
    ai consumes customer.*

  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus

  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
      timeout "5s"
```

An external app may be Python, Java, Node, Rust, or any other stack. Lazuli does
not require the implementation to use Drusa; it requires a contract. Drusa
materializes workspace edges primarily as Go transport bindings, event
publishers/consumers, gateway wiring, mocks, and contract tests for the Lazuli
apps that participate in the graph.

`architecture` and `services` describe logical service boundaries, not
mandatory process boundaries. `mode modular_monolith` lets Drusa generate one
deployable app with enforced ownership boundaries; `mode microservices` is a
topology choice Drusa may materialize later with RPC clients, event routing, and
separate deploy units. Lazuli owns `owns`, `exposes`, `publishes`, `consumes`,
and context propagation because those facts affect static analysis, generated
clients, policy/tenant propagation, idempotency, and contract tests. Concrete
choices such as gRPC, Connect, Kafka, NATS, Kubernetes, Envoy, or service mesh
providers stay in Drusa adapters.

`packs` in `registry.lzi` declares reusable Lazuli/Drusa packages available to
the app. A registry pack records its package/path source, optional version,
what it provides, and the abstract slots it requires. It does not inline the
pack's domain model, UI, provider payloads, handlers, migrations, or adapter
implementation:

```lazuli
registry
  packs
    payments from @drusa/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
```

`packs` in `app.lzi` enables a registry pack for this app:

```lazuli
app AcmeCRM
  uses
    payments

  packs
    payments from registry.packs.payments

  bindings
    payments.gateway = registry.integrations.mercadopago
```

Enabled packs may satisfy app `uses` entries and may introduce abstract
requirements that must be bound just like local feature requirements. This
keeps `app.lzi` as a composition root and `registry.lzi` as a catalog; concrete
pack internals and generated/runtime behavior remain in Drusa packs and
adapters.

Lazuli itself does not call external systems. It declares the contract that
Drusa turns into Go runtime wiring. The Go backend performs HTTP/RPC/broker
publishing, broker consuming, and webhook handling through generated transport
bindings and adapter implementations. React and Expo clients consume generated
commands, queries, APIs, routes, and UI state; they should not call provider
integrations such as MercadoPago, Serasa, or internal AI services directly.

`integrations` is the external integration registry. It declares provider-
neutral names, capability kinds, adapter references, allowed environments, and
credential scope. It does not describe provider HTTP operations, raw payload
schemas, provider client/SDK methods, sandbox URLs, or cloud secret stores. Use
it to say that the app has `crm: CRMProvider`, `mercadopago: PaymentGateway`,
or `serasa: CreditBureau`; feature `.lzi` files and Drusa packs declare why
and when those integrations are called, and Go adapters declare how they are
called.

Adapter references carry provenance without becoming a dependency-injection
container:

```lazuli
registry
  integrations
    mercadopago: PaymentGateway
      adapter @drusa/mercadopago

    serasa: CreditBureau
      adapter @plugin/acme/serasa

    crm: CRMProvider
      adapter @adapter.crm

    local_ai: AiInference
      adapter "./integrations/local_ai.go"
```

Allowed adapter source classes are:

- `@drusa/<adapter>` for Drusa-maintained adapters.
- `@plugin/<publisher>/<adapter>` for third-party plugin adapters.
- `@adapter.<name>` for local adapter extension references.
- local paths such as `"./integrations/local_ai.go"` when the adapter lives in
  app code.

`lazuli inspect` exposes the authored `adapter` and derived
`adapter_provenance` (`drusa`, `plugin`, or `local`). `lazuli doctor` rejects
adapter references whose provenance cannot be determined. Construction order,
lifetimes, test doubles, logger/database clients, connection pools, provider
base URLs, and optional provider SDK setup remain Drusa/runtime/adapter
mechanics, not Lazuli syntax.

Credential scopes are `platform`, `tenant`, or `actor`. Credential bindings may
reference declared `env.NAME` values or later credential resources, but
provider-specific storage stays outside core Lazuli.

Reusable features declare abstract integration requirements instead of importing
provider entries directly:

```lazuli
feature payments
  purpose "Payment intents and checkout sessions."

  requires integration gateway: PaymentGateway
```

When a feature needs multiple external contracts, use the block form:

```lazuli
feature credit_check
  purpose "Credit bureau consultation."

  requires
    integration bureau: CreditBureau
    integration document_validator: TaxIdValidator
```

`requires integration <name>: <CapabilityType>` means the feature depends on an
abstract capability slot. It does not choose MercadoPago, Stripe, Serasa,
ReceitaWS, or any other provider. `app.lzi` binds that feature slot to a
concrete registry entry:

```lazuli
app AcmeCRM
  uses
    customer_import

  bindings
    customer_import.crm = integrations.crm
```

Binding sources use `integrations.<name>` or `registry.integrations.<name>`.
They reference entries from `app.lzi` or the package-level `registry.lzi`.
Drusa uses those bindings to wire Go interfaces/clients to adapter
implementations. Adapters implement the concrete transport mechanics.
`lazuli inspect` exposes both app bindings and feature requirements, and
`lazuli doctor` rejects missing, unknown, or type-mismatched integration
bindings.

Commands and jobs call those abstract slots with `calls <slot>.<operation>`.
This is still a Lazuli contract, not provider execution. Drusa lowers it to Go
interfaces and typed transport bindings; the Go adapter performs the actual
HTTP/RPC/event call:

```lazuli
feature customer_import
  requires integration crm: CRMProvider

  job process_import
    trigger event customer_import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
      org_id = payload.org_id
    timeout "30s"
    handler "./jobs/process_import.go"
```

Every `calls` header names a feature requirement slot and a provider-neutral
operation. The child lines bind named arguments to existing command/job
expressions. The same command or job block should also make timeout, retry, and
idempotency visible when retries could duplicate side effects. `lazuli inspect`
exposes external calls under `feature.external_calls`; `lazuli doctor` rejects
calls to undeclared slots and calls without timeout, and warns when retry or job
idempotency is missing.

`lazuli inspect app.lzi --format=json` exposes the entrypoint manifest under
`app`; `lazuli inspect registry.lzi --format=json` exposes the package catalog
under `registry`. `lazuli doctor` loads both and checks the combined operational
contract against local features and projections: feature `uses`, `env.*`
references, `@cap.File` storage needs, custom APIs, webhooks, jobs, scheduled
jobs, web/mobile targets, and public URLs must be represented in the app or
registry contract. When `services` are declared, every local feature should be
owned by exactly one service boundary and a service should not expose commands,
queries, APIs, or workflows from features it does not own.

Concrete routes stay in `.lzx` because they bind web URLs and mobile route
patterns to experience views and platform surfaces:

```lazuli
route admin_customer_detail
  path "/admin/customers/:id"
  params id: Customer.ID
  to customer.view.detail(id: path.id)
  surface customer web
  audience admin
  lazy true

route sales_customer_detail
  path "customers/[id]"
  params id: Customer.ID
  to customer.view.detail(id: path.id)
  surface customer mobile
  audience sales
```

Top-level `route` declarations are the source of truth for generated web paths,
mobile route patterns, and type-safe route builders. A dynamic segment such as
`:id` or `[id]` must be declared with `params id: <Type>`. The `to` binding maps
path parameters into an abstract experience view. `surface` and `audience`
make platform routing and authorization context explicit. `path` is canonical
for both web and mobile; legacy `stack` route declarations are accepted only as
compatibility syntax.

## Surfaces

Canonical experience source is split across `.lzx` layers:

```lazuli
experience customer
  imports customer

  view list
    source customer.query.list
    action create -> customer.command.create
    opens detail(id: row.id)

  view detail
    route id: Customer.ID
    anchor @anchor.customer_detail
    source customer.query.by_id(id: route.id)
    action archive -> customer.workflow.lifecycle.archive(id: route.id)

surface customer web
  uses experience customer

  audience admin
    view list Table
      columns name, email, tier
```

`.lzi` does not declare or depend on UI. Abstract `.lzx` declares the product
experience and imports `.lzi` capabilities. Concrete `.web.lzx` and
`.mobile.lzx` files declare protected platform projections and group product
variants under `audience`/`tenant` blocks. The platform segment stays
immediately before `.lzx`; file names organize source, and the header decides
semantics.

Product variants use total override, not cascade. If `audience admin tenant
acme` changes the list view, it redeclares the whole view. Partial operations
such as `columns += account_manager` are invalid because they make the final UI
exist only after merge resolution.

Read views consume query sources. A view does not need to restate `policy @policy.read` if the source query is scoped and the feature has a `read` policy.

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

Platform projections should not use bare submit verbs such as `submit create`.
Use `command.create` for same-experience commands or a feature-qualified target
such as `customer.command.capture_lead` when the command lives in another
feature namespace.

The compiler should surface this derivation in `explain`.

Every view has an implicit stable id: `<feature>.<surface_id>.<view_name>`, where `surface_id` joins the surface words with `_`. For example, `feature customer` + `surface web admin` + `view detail` has the implicit id `customer.web_admin.detail`. Cross-feature composition requires an explicit `anchor @anchor.<name>` plus `extensible_by`; implicit view ids are for inspection and source maps, not an open extension surface. The older inline form `view detail id @anchor.customer_detail` is tolerated as authoring sugar, but the expanded form keeps route, anchor, and source as separate contracts.

Actions in routed abstract views should bind route arguments explicitly. Prefer
`action archive -> customer.workflow.lifecycle.archive(id: route.id)` over a
bare transition or command target, so generators do not infer which route value
identifies the target.

`filter` inside a view describes UI controls. `filters` inside a query describes data predicates. The view filter names should be backed by query params and query filters when they affect server-side data.

Custom view slots may either reference a reusable extension or declare a single-use block inline:

```lazuli
view detail SidePanel
  source query.by_id(id: route.id)
  block @client.activity_timeline

view import_detail SidePanel
  source query.by_id(id: route.id)
  block import_progress: ViewBlock[ImportBatch] at "./ui/progress.tsx"
```

Use `@client.*` when the block or cell renderer is named in `extensions` or reused by multiple surfaces. Use the inline `block <name>: <Contract> at "<path>"` form when the implementation exists only for that slot.

### Cross-Feature View Composition

A feature may extend a view owned by another feature when it owns an adjacent capability:

```lazuli
feature customer_tags
  uses customer

  extends @anchor.customer_detail
    slot aside
      block @client.tag_editor
      platforms web, mobile
      audience admin, sales
```

The target view may declare a shorter stable id:

```lazuli
view detail
  route id: Customer.ID
  anchor @anchor.customer_detail
  extensible_by customer_tags, customer_import
  source query.by_id(id: route.id)
```

Use `extends @anchor.<view_id>` only when the target view declares that exact anchor and whitelists the extending feature with `extensible_by`. Views without `extensible_by` are not extensible, even though they still have implicit stable ids for inspection. The extending feature owns the inserted block and its extension implementation; the target feature still owns the base view.

Extensions should put inserted blocks under an explicit `slot`. A slot names the
target region and may include order relative to another block:

```lazuli
extends @anchor.customer_detail
  slot timeline after activity_timeline
    block @client.import_history
    platforms web
    audience admin
```

The older direct child form is tolerated as authoring legacy:

```lazuli
extends @anchor.customer_detail
  block @client.tag_editor
```

`lazuli check` warns on the direct form because placement, ordering, platform
support, and audience visibility should be deterministic for generators.

`lazuli check --security-profile strict` should warn when a feature listed in `extensible_by` does not declare a matching `extends @anchor.<view_id>` block. The whitelist exists to describe exercised composition, not speculative future permission.

The target view type determines which slots are accepted. For example, a
`SidePanel` might accept `header`, `timeline`, `aside`, and `actions`, while a
`Table` might accept `toolbar`, `columns`, or `row_actions`. The analyzer should
reject unsupported slots with a targeted diagnostic.

Cross-feature view composition should not be used to replace the base view. If a feature needs a completely different screen, create its own view or an explicit `escape_route`.

## Escape Routes

Escape routes register pages Lazuli should know about but should not govern internally. They must still declare where the file lives and the coarse security envelope:

```lazuli
escape_route "/admin/customer-debug"
  at "./pages/customer_debug.tsx"
  policy @role.admin
  tenant org
```

The route implementation remains custom code. Lazuli records the route, policy, tenant axis, and source path in generated manifests so escape hatches do not become invisible security holes.

## Async Work, Webhooks, And Jobs

`job` is the canonical construct for asynchronous work. Its trigger states why the work runs, and the job name is the stable subscription/operation id used for generated code, observability, and per-environment controls.

Jobs do not declare `params`, `route`, or `input`. Their locator namespaces are derived from the trigger: event jobs get `payload.*` and `envelope.*`, schedule jobs get `schedule.*`, and webhooks use verified inbound `payload.*`.

Event-triggered jobs consume feature events:

```lazuli
job send_archive_survey
  trigger event customer.customer_archived
  idempotency by envelope.id
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
    policy_for jobs, webhooks: @actor.system

  uses customer

  domain
    event welcome_email_sent
      customer_id: ID

  job send_welcome
    trigger event customer.customer_activated
    tenant_from payload.org_id
    idempotency by envelope.id
    retry 3 backoff exponential
    handler "./outreach/send_welcome_email.go"
    emits welcome_email_sent
```

Do not fold this kind of capability into `customer` just because it listens to customer events. A feature may be only reactions when that is the product boundary. It may still declare its own events directly under `domain`; resource-backed `event_group ... on <Resource>` inheritance is optional and only applies when the feature has a relevant resource.

Scheduled jobs use a cron-like trigger:

```lazuli
job recompute_scores
  trigger schedule "0 2 * * *"
  fanout tenants org
  idempotency by tenant.org_id, schedule.day
  retry 3 backoff exponential
  handler "./jobs/recompute_scores.go"
```

When a feature or resource is tenant-scoped, scheduled jobs should either
declare tenant fanout or explicitly opt into global execution:

```lazuli
job recompute_scores
  trigger schedule "0 2 * * *"
  fanout tenants org
  idempotency by tenant.org_id, schedule.day
  handler "./jobs/recompute_scores.go"

job global_cleanup
  trigger schedule "0 3 * * *"
  scope global
    reason "Deletes expired non-tenant operational records."
  handler "./jobs/global_cleanup.go"
```

Event jobs can also declare a queue lane when the adapter should enqueue work instead of running it inline:

```lazuli
job process_import
  trigger event customer_import_uploaded
  queue customer_imports
  tenant_from payload.org_id
  idempotency by payload.batch_id
  retry 3 backoff exponential
  handler "./jobs/process_import.go"
  emits customer_import_completed
```

`idempotency by` names the dedupe key for the trigger execution. Event-triggered jobs use `envelope.*` for event-bus metadata and `payload.*` for the producer-authored event payload. `envelope.id` refers to the event-envelope id supplied by the event bus and does not need to be repeated in the authored event payload. Use `envelope.id` when each bus delivery should be processed once; use `payload.<business_key>` when the product needs dedupe by a domain key such as an import batch id. Composite keys are comma-separated, for example `idempotency by payload.org_id, payload.external_id` when an external id is only unique inside a tenant. Consumers may only reference payload fields declared by the producer event contract, including fields inherited from matching `event_group` payloads; `lazuli check` should report `payload.*` references that do not exist in the producer event. If the producer event contract includes `org_id`, event-triggered jobs should declare `tenant_from payload.org_id`; generated handlers should run with that tenant fixed in `ctx` so follow-up queries do not accidentally run cross-tenant. Webhooks use the verified inbound `payload.*` namespace. In tenant-scoped features, a webhook should declare `tenant_from payload.<axis>_id` or explicit `scope global` with a reason; idempotency by tenant key does not itself bind execution context. Do not write bare webhook keys such as `idempotency by external_id`; write `idempotency by payload.external_id` or a composite payload key so the source is explicit. `retry <count> backoff <strategy>` is declarative delivery policy; `retry 3` means up to three retry attempts after the initial attempt fails. Adapters should support at least `fixed` and `exponential` before accepting those strategies in strict mode.

Async snippets that omit `policy` assume the surrounding feature declares an applicable `defaults policy_for ...: @actor.system`; otherwise write `policy @actor.system` inline. `policy_for` is a fallback for constructs without a local policy, primarily jobs, webhooks, and resource-less system features. Commands should keep local policy declarations so a feature-level system default cannot quietly authorize user-facing writes.

`handler` may declare `returns <Type>` when the return value is semantically consumed elsewhere:

```lazuli
handler "./integrations/upsert_customer_from_crm.go" returns Customer
```

For fire-and-consume jobs whose only meaningful result is success or failure, `handler "./path.go"` is preferred. The input type is derived from the trigger envelope/payload or job schedule context, and Go adapters should generate `func <JobName>(ctx, payload) error`-style contracts for event jobs.

Webhook handlers are explicit inbound edges from the outside world. In canonical v0, webhooks should verify and then run a named server-side implementation. The verifier and handler input types are derived from the webhook name by adapter convention; only the return type is written when it matters semantically:

```lazuli
webhook stripe_invoice_paid
  path "/webhooks/stripe/invoice-paid"
  verify "./integrations/stripe.go"
  tenant_from payload.org_id
  idempotency by payload.org_id, payload.provider_event_id
  policy @actor.system
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
  fanout tenants org
  handler "./jobs/recompute_scores.go"
```

`trigger event` means event-consumer work. `trigger schedule` means cron-like recurring processing. `queue` is an execution lane, not the source of truth for why the job runs.

Operational job kind is derived, not authored: `trigger schedule` expands to `scheduled`, `trigger event` without `queue` expands to `reactor`, and `trigger event` with `queue` expands to `queued_worker`. `lazuli inspect` should show that resolved kind for dashboards and generated worker topology. Authors should not write a separate `kind` unless a future adapter needs a genuinely ambiguous trigger.

Jobs and webhooks declare their required implementation inline with `handler` (and `verify` for webhooks). Do not duplicate those handlers in `extensions`; reserve `extensions` for reusable UI renderers, hooks, validators, query modifiers, adapters, and domain functions that are referenced by name from multiple constructs.

A job chooses one body style:

```lazuli
job record_customer_created
  trigger event customer.customer_created
  idempotency by envelope.id
  creates AuditEvent
    source_event = "customer_created"
    subject_id = payload.customer_id

job recompute_score_after_invoice
  trigger event billing.invoice_paid
  idempotency by envelope.id
  target query.by_id(id: payload.customer_id)
  let new_score = @fn.risk_score(target)
  updates Customer
    score = new_score
  emits customer_score_recomputed
    score = new_score
    reason = "invoice_paid"
```

Use the declarative body for small reactions that bind targets, create resources, update resources, or emit events without custom control flow. `target` makes the loaded resource available as an immutable `target` binding, regardless of the resource name. Resource creation belongs under `creates <Resource>` assignment blocks, resource mutation belongs under `updates <Resource>` assignment blocks, and event payload values belong under `emits <event>`. The `emits` child assignments fill event-specific payload fields that are not already supplied by the matching `event_group` envelope. They do not replace the inherited envelope; `lazuli inspect --expand=events` shows the full contract and provenance. Inside a declarative job body, use this order: `target`, zero or more `let` bindings, one write effect (`creates`/`updates`/`deletes`), then `emits`. Use `let` for derived values that are used by both mutation and event payloads; do not rely on `target` changing timing between lines.

Use `handler` when the job mutates state through non-trivial IO, loops over batches, calls providers, handles partial failure, or needs custom code. A handler-backed job may still declare `emits` so the event graph remains visible, but it should not also declare `target`, `creates`, `updates`, or `deletes`.

## Auth

`auth` is a block because authentication is a family of related subcontracts: identity, password verification, OAuth adapters, MFA, session storage, refresh behavior, and rate limits.

```lazuli
auth
  identity Customer.email

  password
    hash @fn.hash_customer_password
    verify @fn.verify_customer_password
    rate_limit "5 per 10 minutes"

  sessions
    resource CustomerSession
    ttl "7 days"
    refresh false
```

Use a separate feature such as `customer_auth` when authentication is its own product capability. `auth identity` may reference one identity resource from the current feature or a directly listed `uses` feature; session and MFA storage should be owned by the auth feature. Do not model multiple identity domains, such as Customer and Staff, inside one auth block. Split them into separate auth features.

## Extensions

An extension without `at` uses feature-local convention:

```lazuli
extensions
  client status_cell: CellRenderer[Customer]
  hook before_create: Hook[CreateCustomer]
  fn risk_score: Function[Customer, Integer]
```

The extension declaration keyword is the namespace used at call sites. References use capability namespaces, not the old catch-all `ext.*` namespace:

```lazuli
cells
  status @client.status_cell

let score = @fn.risk_score(target)
validates field tier @validator.validate_tier
```

This invariant keeps the lookup mechanical: `fn risk_score` resolves as `@fn.risk_score`, `hook before_create` resolves as `@hook.before_create`, and `adapter google_oauth` resolves as `@adapter.google_oauth`.

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
| `hook <name>: Hook[X]`             | `features/<feature>/hooks/<name>.go`        |
| `validator <name>: Validator[X]`   | `features/<feature>/hooks/<name>.go`        |
| `fn <name>: Function[X, Y]`        | `features/<feature>/domain/<name>.go`       |
| `adapter <name>: IntegrationAdapter[X]` | `features/<feature>/integrations/<name>.go` |
| `query_modifier <name>: QueryModifier[X]` | `features/<feature>/queries/<name>.go` |
| `job <name> handler`               | `features/<feature>/jobs/<name>.go`         |
| `resource <name> validates resource` | `features/<feature>/domain/validate_<name>.go` |
| `resource <name> validates field <field>` | `features/<feature>/domain/validate_<name>_<field>.go` |
| `block <name>: ViewBlock[X]`       | `features/<feature>/ui/<name>.tsx`          |
| `webhook <name> verify`           | `features/<feature>/integrations/<name>.go` |

Use `at` only when the implementation lives outside the convention. Missing implementation status is not encoded by `at`: `lazuli inspect` and `lazuli check --security-profile strict` determine whether the conventional file exists and whether a stub should be generated.

The convention is part of the IR ABI (see `ir-abi.md`): changing a default path is a major bump. Adding a contract type is a minor bump.

## Identity Across Renames

When a command, transition, query, field, or resource is renamed, downstream artifacts that relied on its identity (event lineage, deploy plans, semantic diffs, persisted job data) lose continuity. `previously` declares continuity explicitly:

```lazuli
command register previously migrated create
  policy @policy.create
  creates Customer
  ...

workflow lifecycle on Customer.status
  ship previously migrated deliver: ready -> shipped

resource Account previously migrated Customer
  ...

resource Customer
  lifecycle_stage previously migrated status: CustomerStatus = lead
```

`previously` is universal for renameable identifiers: resources, fields, queries, commands, workflows, workflow transitions, views, jobs, webhooks, and extension symbols may all carry it when the compiler needs identity continuity.

The `previously` clause must declare a mode before the prior names:

- `previously migrated <old_name>` means the old name is historical continuity for migration, diff, and stored IR matching. Generated public APIs do not keep accepting it.
- `previously alias <old_name>` means the old name is still a compatibility alias. This should be temporary and should usually carry explicit deprecation policy in the owning feature docs.

The clause carries one or more prior names. The compiler records them on the IR node as `previous_names`. The planner, MCP, and semantic diff respect the link instead of treating the rename as drop-and-create.

Bare `previously <old_name>` is legacy authoring syntax and should be rewritten to either `previously migrated <old_name>` or `previously alias <old_name>`.

`previously` is a migration tool. Use it when continuity matters. Do not use it as a versioning hint or design prose; commentary belongs in `<feature>.ctx.md`.

Keep `previously` only while the compiler, semantic diff, or migration planner still needs to connect the current node to a deployed or stored prior identity. Once every supported environment has migrated and the stored IR baseline no longer contains the old name, `previously` may be removed in an ordinary cleanup change. Future tooling may warn about stale `previously` aliases when it can prove the old identity is no longer reachable.

`previously` does not chain implicitly. To preserve identity across multiple renames, list each prior name:

```lazuli
command register previously migrated create, signup
```

## Reserved For Later

These are intentionally not solved by the simple canonical syntax yet:

- Project-defined templates, macros, or parameterized includes. Lazuli grows by
  adding language primitives or compiler inference, not by letting each project
  create its own dialect. If repeated source is genuinely universal, promote it
  into the language; if it is project-specific, keep the repetition explicit.
- Many-to-many relations with payload or ordering. Use an explicit join resource.
- SQL query body verification beyond declared params/scope/returns.
- Workflow transition groups such as `any -> canceled`.
- Multiple `event_group <pattern> on <Resource>` blocks are allowed only when patterns do not overlap; overlapping event payload templates are an error.
- Cross-feature event re-emission is intentionally not modeled in v0. Use a new event in the consumer feature; do not re-emit the producer's event from a different feature.
- Schedule jobs currently require an effective `@actor.system` policy through feature defaults or an inline `policy`; making schedule jobs system-only by construction is reserved for a later decision.
- Non-exact rule matching such as matching both `reassign` and `bulk_reassign`.
