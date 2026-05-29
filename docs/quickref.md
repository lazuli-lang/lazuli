# Lazuli Quick Reference

This is the context pack to load first when an agent or a human needs to
author, review, or patch canonical `.lzi`/`.lzx` files. It is intentionally short.
Use `docs/canonical-semantics.md` for the full normative reference and
`docs/invariants.md` for the checker/codegen contract. Use
`docs/capability-layering.md` when deciding whether a capability belongs in the
Lazuli language, Lazuli compiler, the runtime pack, runtime, or adapter.

## Status Legend

| Marker | Meaning |
|--------|---------|
| `[v0]` | canonical authoring syntax now |
| `[lint]` | accepted syntax with LSP/check warning or strict-mode pressure |
| `[tooling]` | derived by `lazuli inspect`, formatter, LSP, or generator |
| `[reserved]` | explicitly not part of v0 |
| `[legacy]` | tolerated only for migration from earlier drafts |

When editing source, prefer `[v0]`. Do not introduce `[reserved]` constructs.

## Minimal Feature

```lazuli
feature ping
  purpose "Store short ping messages inside an org."

  defaults
    tenancy org
    timestamps

  domain
    resource Ping
      message: Text required

    query.list list
      paginate 50

    query.lookup by_id by id: ID

    event ping_created
      ping_id: ID

  policies
    create: @role.admin
    read: @scope.same_org

  command create
    input message
    policy @policy.create
    creates Ping from input
    emits ping_created
```

`app.lzi` is the project entrypoint and operational contract:

```lazuli
app PingApp
  title "Ping"

  uses
    ping

  targets
    backend go
    web react

  environments
    local

  urls
    web local "http://localhost:3000"
    api local "http://localhost:8080"

  architecture
    mode modular_monolith
    service_ready true
    enforce_service_boundaries true

  services
    service core
      owns ping
      exposes
        query ping.query.by_id
      publishes ping.*

  communication
    internal sync rpc
    external http
    async event_bus
    propagate actor, tenant, trace_id, request_id

  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"

    unit web
      serves surfaces web

  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
```

`registry.lzi` is the package catalog:

```lazuli
registry
  env
    group crm
      server CRM_WEBHOOK_SECRET: Secret required in production
    group public_clients
      client PUBLIC_API_URL: Url required
    group mailer
      server MAILER_API_KEY: Secret required in production

  capabilities
    database postgres
    integration crm

  packs
    customer_import from @runtime/customer-import
      version "0.1.0"
      provides feature customer_import
      requires integration crm: CRMProvider

  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET
```

Adapter references declare provenance, not provider operations:

```lazuli
adapter @runtime/mercadopago        # first-party (`@runtime/...`) adapter
adapter @plugin/acme/serasa       # third-party plugin adapter
adapter @adapter.crm              # local adapter extension
adapter "./integrations/ai.go"    # local app code
```

Pack entries belong in `registry.lzi`; `app.lzi` only enables them. A pack may
provide a feature and require abstract slots, but its implementation details
stay in runtime packs/adapters:

```lazuli
app PingApp
  uses
    customer_import

  packs
    customer_import from registry.packs.customer_import

  bindings
    customer_import.crm = integrations.crm
```

Reusable features require abstract integration slots instead of concrete
providers:

```lazuli
feature payments
  purpose "Payment intents and checkout sessions."

  requires integration gateway: PaymentGateway
```

The app binds each abstract slot to a registry integration:

```lazuli
app PingApp
  bindings
    payments.gateway = integrations.crm
```

Profiles hold environment-specific overrides without turning `app.lzi` into
provider config:

```lazuli
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
```

`workspace.lzi` is optional. Use it only above multi-app/polyrepo/external
service systems:

```lazuli
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"

  shared_registry "./registry.lzi"

  boundaries
    crm publishes customer.*
    ai consumes customer.*

  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus

  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
```

Repository URLs, branches, local ports, concrete brokers, gateways/proxies, and
deploy providers belong in runtime/adapters, not `workspace.lzi`.

`contract.lzi` describes a non-Lazuli service or external schema:

```lazuli
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

  record CustomerSummaryResult
    summary: Text required
    generated_at: DateTime required

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    auth service
    timeout "10s"

  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
      summary: Text required
```

Supported import formats: `openapi`, `asyncapi`, `proto`, `json_schema`,
`avro`. the Lazuli runtime should turn contracts into Go HTTP/RPC/event bindings and tests;
SDK export is optional publication tooling, not core Lazuli runtime semantics.

Use a block when a feature needs more than one slot:

```lazuli
  requires
    integration bureau: CreditBureau
    integration gateway: PaymentGateway
```

Commands and jobs call those slots with provider-neutral operation names. Go
adapters execute the real HTTP/RPC/event work:

```lazuli
  job process_import
    trigger event customer_import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls gateway.normalize_import_batch
      batch_id = payload.batch_id
    timeout "30s"
    handler "./jobs/process_import.go"
```

Routes and experiences live in `.lzx`:

```lazuli
route ping_detail
  path "/pings/:id"
  route id: Ping.ID
  to ping.view.detail(id: route.id)
  surface ping web
  audience admin

experience ping
  imports ping

  view list
    source ping.query.list
    action create -> ping.command.create
    opens detail(id: row.id)

  view detail
    route id: Ping.ID
    source ping.query.by_id(id: route.id)
```

```lazuli
surface ping web
  uses experience ping

  audience admin
    view list Table
      columns message, created_at
```

A `policy` route/view guard may also gate on a domain **lifecycle**
state (mutually exclusive forms; see `grammar.lzx.md` §4):
`requires_lifecycle <Resource> = <state>` (exact match) or
`requires_lifecycle_in <Resource> [s1, s2]` (allow-list, the canonical
grep-friendly form). On failure the runtime dispatches via the
resource's `lifecycle_routes`/`@resume` router (doctor:
`ROUTE-GUARD-LIFECYCLE-*-001/2/3`).

## Canonical Order

`.lzi` feature block order:

```txt
meta -> defaults -> uses -> refs? -> domain -> policies -> errors -> auth
-> command -> api -> report -> job -> webhook -> surface -> extensions
-> escape_route
```

`meta` means `purpose`, `non_goals`, `attach_ctx`, and `context`.
[v0] `workflow` is retired (parser hard-errors `E-WORKFLOW-RETIRED`). Express
lifecycle via the resource `lifecycle <field>` block plus the command
`triggers transition <name>` clause.
`refs` is optional and documentary. Do not author it just to list core
`@role.*`/`@scope.*`/`@policy.*` namespaces; use
`lazuli inspect --expand=refs` for that generated manifest.

Experience source family:

```txt
<feature>.lzi          # domain/capability contract
<feature>.lzx          # abstract experience/view model
<feature>.web.lzx      # protected web projection
<feature>.mobile.lzx   # protected mobile projection
```

Extra physical split segments go before the protected platform suffix:
`<feature>.<audience>.web.lzx`, not `<feature>.web.<audience>.lzx`.

`.lzi` does not know `.lzx` exists. Abstract `.lzx` imports `.lzi`
capabilities. Platform `.lzx` files use an abstract experience and group
product variants under `audience`/`tenant` blocks.

`lazuli check <file>` is file-local. Use `lazuli doctor <file-or-dir>` when a
capsule spans `.lzi` plus sibling `.lzx` files. Doctor loads the package set and
checks cross-file contracts such as surface audience reachability against
command policies (`LZX-POL-001`).

[v0] `.lzx` has no cascade or partial override. Do not write
`columns += score`; redeclare the whole view for that audience/tenant.

## Surface Primitives (`.lzx`)

Inspectable surface constructs (see `grammar.lzx.md` §7a for full grammar):

```lazuli
view.list customers
  filter created: date_range          # paired from/to picker -> created_from / created_to
  view_mode { table; kanban }         # user-toggleable render modes
  view.inline_table on_change @command.update_row   # inline-editable rows
  wizard_steps 3 current registration_step          # step indicator bound to an enum field
  tab_group derived_from vehicle_type {             # runtime-data-driven tabs
    case TV, RADIO -> tab "Broadcast"
    case PRINT     -> tab "Print"
  }

view.board activity_board
  lanes derived_from status           # kanban lanes per enum variant

repeatable input installments group { days: Int; percentage: @semantic.Percentage }
  validates sum(percentage) = 100     # repeatable row group with cross-row sum guard
```

Static `tabs { tab "Name" -> view X }` and `wizard <name> steps { step 1: <ref> }`
containers also exist (`grammar.lzx.md`).

## Policy Vocabulary

Lazuli has three distinct policy concepts. Do not collapse them:

| Shape | Meaning | Example |
|-------|---------|---------|
| `policies` block | feature-local policy category dictionary | `update: @role.admin, @role.sales` |
| `policy ...` statement | construct references one category | `policy @policy.update` |
| policy atom | terminal auth predicate/executor/role | `@role.admin`, `@scope.same_org`, `@actor.system` |

[v0] Commands and queries with local policy should reference
`@policy.*`, not raw `@role.*` or `@scope.*`. Defaults and escape routes may
still use atoms directly when they are the actual authority boundary.

[v0] `policy_for` is the only feature-default policy form. It is scoped to
construct families so the fallback cannot be mistaken for a command default:

```lazuli
defaults
  policy_for jobs, webhooks: @actor.system
```

Use it primarily for jobs, webhooks, queues, maintenance, and resource-less
reaction features. User-facing commands should keep explicit local
`policy @policy.*`; forgetting one is a diagnostic, not silent system
authorization.

## Closed Namespaces

| Namespace | Meaning |
|-----------|---------|
| `@role.*` | role authorization atoms |
| `@scope.*` | authorization predicates such as same-org, owner, public, none |
| `@actor.*` | executor identities such as user, system, service |
| `@policy.*` | feature-local policy categories |
| `@semantic.*` | semantic types with validation/formatting |
| `@cap.*` | platform capabilities: files, hashes, encryption, tokens |
| `@pii.*` | data classification markers |
| `@key.*` | cryptographic key scopes |
| `@client.*` | UI extension contracts |
| `@fn.*` | pure server-side functions |
| `@hook.*` | lifecycle hooks |
| `@validator.*` | validators |
| `@adapter.*` | local integration adapter extension references |
| `@query_modifier.*` | query modifiers |
| `@anchor.*` | view composition anchors |

Adapter package refs such as `@runtime/mercadopago` and
`@plugin/acme/serasa` are registry adapter sources, not general extension
namespaces. Unknown namespaces are errors unless the spec adds them.

## Binding Namespaces

| Construct | Available bindings |
|-----------|--------------------|
| `query.*` | `params.*`, `ctx.*` |
| `command` | `route.*`, `input.*`, `ctx.*`, `target` after explicit or inferred target |
| declarative event job | `envelope.*`, `payload.*`, `ctx.*`, `target` after `target ...` |
| schedule job | `schedule.*`, `ctx.*` |
| webhook | `payload.*`, `ctx.*` |
| rule | `self`, `ctx.*` |
| lifecycle transition tests | `self`, `ctx.*` |
| command tests | `target`, `ctx.*` |

`target` is the immutable entity loaded by a command or declarative job.
`self` is the snapshot evaluated by rules and lifecycle transition predicates.

## Name Resolution

[v0] Local operation references omit the feature prefix:

```lazuli
target query.by_id(id: route.id)
source query.list
submit command.create
```

[v0] Cross-feature operation references must be feature-qualified and backed
by `uses`:

```lazuli
feature customer_auth
  uses customer

  command enable_mfa
    target customer.query.by_id(id: route.customer_id)
```

Lazuli does not search `uses` in declaration order for operation references.
Unqualified `query.*`, `command.*`, and `@anchor.*` references are local unless
the syntax explicitly says otherwise.

Query declaration mode is not repeated at the call site. A declaration such as
`query.lookup by_id by id: ID` is consumed as `query.by_id(id: ...)`; use
`lazuli inspect --expand=summary,dependencies` when you need the resolved kind.

## Generated Provides

[tooling] Do not author a `provides` block in v0. Use generated summary instead:

```bash
lazuli inspect feature.lzi --expand=summary --format=json
```

The summary is the source of truth for exported resources, queries, events,
surfaces, anchors, and extension edges. This answers the same question a manual
`provides` header would answer without adding drift.

The JSON summary includes a derived `provides` object:

```json
{
  "provides": {
    "types": ["Customer", "CustomerLtv"],
    "queries": ["list", "by_id", "lifetime_value"],
    "events": ["customer_created"],
    "anchors": ["@anchor.customer_detail"]
  }
}
```

Anchor declarations are intentionally local to the view:

```lazuli
view detail
  route id: Customer.ID
  anchor @anchor.customer_detail
  extensible_by customer_tags, customer_import
```

Use `inspect --expand=summary` to list provided anchors and
`inspect --expand=dependencies` to list features that extend them.

For the typed IR projections (commands, apis, resources, queries,
records, defaults), use the axis flags directly:

```bash
lazuli inspect feature.lzi --expand=commands,apis --format=json
lazuli inspect feature.lzi --expand=resources,queries,records --format=json
lazuli inspect feature.lzi --expand=defaults --format=json
```

Each axis projects its lifted IR slice verbatim. The `apis` axis also
accepts the singular token `--expand=api`.

View extensions should target explicit slots:

```lazuli
extends @anchor.customer_detail
  slot aside after activity_timeline
    block @client.tag_editor
    platforms web, mobile
    audience admin, sales
```

## Canonical Sugar Table

| Compact form | Expands to | Legal when | Not legal when |
|--------------|------------|------------|----------------|
| `creates Resource from input` | assignments for every matching input field | every input slot is consumed by matching field or explicit assignment | input has unconsumed fields |
| `query.lookup by_id by id: ID` | lookup with one param and matching key | single-key lookup | composite key or param/key names differ |
| omitted local target | `target query.by_id(id: route.id)` | command has `route id: ID`, local `updates`/`deletes`, and local `query.lookup by_id` | cross-feature target, non-`route.id`, multiple locators |
| inline transition clauses | child `requires`/`emits` statements | scalar `requires`/`emits`, canonical order | child blocks such as `tests`; multiple values |
| unqualified enum literal in tests | enum value for subject field | field type makes enum unambiguous | ambiguous or unrelated enum |
| `event_group prefix_* on Resource` with nested `event name` | inherited payload for matching same-feature events | single trailing wildcard, no overlap | cross-feature inheritance or overlapping groups |

Sugar is local notation. If a proposed shortcut creates constructs elsewhere,
it is a macro, not v0 sugar.

## Queries

| Mode | Use |
|------|-----|
| `query.list <name>` | generated collection query |
| `query.lookup <name> by <field>: <Type>` | generated single-key lookup |
| `query.lookup <name>` with `params`/`key` | generated composite or reshaped lookup |
| `query.sql <name>` | SQL-backed query wrapper |

`params` belongs to queries. `input` and `route` belong to commands.
`paginate <n>` is the generated default page size, not a hard maximum.
`paginate` is valid only on `query.list` and must be a positive integer.
`query.list` defaults to `order created_at desc`; declare `order` only when a
query intentionally differs from newest-first listing.
Simple equality filters derive language-managed indexes. With `tenancy org`,
`status when params.status` derives `org, status`, and
`customer.id = params.customer_id` derives `org, customer`. Search, `has`,
`!=`, `nil`, `scope override`, and SQL queries do not derive indexes.

Use `search` for text matching instead of an equality-looking filter:

```lazuli
params
  search: Text optional

search params.search over name, email
  mode contains
```

`query.sql` return types such as `CustomerLtv[]` must resolve through local
`record` declarations, resources, extension contracts, or adapter-provided
external types before codegen. They are not inferred from SQL text in v0.
`record` is a typed projection/DTO, not persisted domain state: no tenancy,
soft delete, lifecycle, policies, or generated commands.

```lazuli
record CustomerLtv
  customer_id: ID
  amount: @semantic.Money
  currency: Text

query.sql lifetime_value
  returns CustomerLtv[]
  sql "./queries/customer_lifetime_value.sql"
```

## Domain Primitives

Resource-body modifiers, relations, and field decorators beyond the basics:

| Construct | Meaning | Example |
|-----------|---------|---------|
| `append_only` | insert-only resource; rejects update/delete commands | `append_only` |
| `many_through <J> to <P> { … }` | M:N junction carrying payload metadata | `many_through JobMember to User` |
| `polymorphic_ref <type> <id> targets [A, B]` | polymorphic FK over a target set | `polymorphic_ref entity_type entity_id targets [Job, Activity]` |
| `unique <field> when <pred>` | partial/conditional unique index | `unique is_default when is_default = true` |
| `<f>: ID target @feature.<feat>.<Res>` | cross-feature FK annotation (needs `uses`) | `dept_id: ID target @feature.org.Department` |
| `@slug` | auto-unique URL slug column | `slug: Text @slug` |
| `@full_text` | tsvector source for `fts on (...)` | `body: Text @full_text` |
| `@owner_axis(through: <col>)` | ownership-scope projection FK | `host: Host @owner_axis(through: org_id)` |
| `computed_date from <base> offset <n>` | derived `Date` = base field + days | `due: Date computed_date from start offset 30` |
| `schedule_rule from @fn.<r>(<arg>) offset <n>` | rule-driven derived `Date` | `due: Date schedule_rule from @fn.rule(input.kind) offset 7` |

> `computed_date from <field>` anchors on a **same-row** field only.
> **Cross-row** date anchors (e.g. a `StepEnd` due date that depends on a
> *sibling/previous* row's `completed_at`) use `schedule_rule from
> @fn.<rule>(...)`: the registered `@fn` resolves the base date from the
> related rows. There is no `prev(order).field` primitive — cross-row
> recalc lives in the binding `@fn`, not core syntax.

```lazuli
resource Job
  title: Text @slug required
  many_through JobMember to User
    role_in_job: Text required
  polymorphic_ref entity_type entity_id targets [Customer, Activity]
  unique (org, title) when archived = false
```

`@semantic.HexColor` (Text-backed `#RRGGBB`/`#RGB`) and `@semantic.Percentage`
(Decimal-backed, `0..=100`) join the closed `@semantic.*` scalar catalog:

```lazuli
brand_color: @semantic.HexColor required
completion: @semantic.Percentage = 0
```

## Reports

`report <name>` projects a `query.*` source through a closed `columns` catalog
into one or more `formats`. `source` + a non-empty `formats` list are required;
`input` threads request-time params to the source query.

```lazuli
report monthly_audit
  input
    period_start: Date required
  source customer.query.list
  columns
    id from row.id
    ltv from @fn.lifetime_value(row.id) label "Valor de vida"
  formats csv, xlsx
  storage object_storage.files
  visibility signed
  signed_ttl 1h
  policy @policy.global_read
  audit actor, ctx.now
```

## Agents (Cut A)

`agent <name>` declares an LLM-powered capability. Required children:
`policy @policy.<name>`, `output <form>`, `model @llm.<name>`,
`prompt "./path"`. The closed namespace catalog enforces `@llm.*`.

| Form | Meaning |
|------|---------|
| `output stream <Type>` | streaming text-shaped response of the type |
| `output discriminator <Enum>` | LLM returns one enum variant; downstream branches statically |
| `output <Record>` | record output; record's `discriminator` field disambiguates |

Optional Cut A children:

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
  policy @policy.read
  output stream Text
  model @llm.default
  temperature 0
  seed 1
  prompt "./prompts/summarize.md"
  safety @validator.pii_scrub
  tools
    customer.query.lookup.by_id
    customer.query.list
    @tool.web_search
  evals
    case redacts_email
      requires customer.email = "ada@example.com"
      forbids output contains @semantic.Email
    case uses_lookup_when_id_known
      requires input.customer_id = "cus_123"
      requires tools.calls includes customer.query.lookup.by_id
```

- `tools` lists every capability the LLM may invoke. Effect (`read |
  write`) is derived; the underlying capability is the source of truth.
  Adapter tools (`@tool.*`) pin `effect` in `registry.lzi`.
- `evals` gates CI only when the agent declares both `temperature 0`
  and `seed <int>`. Otherwise `eval_nondeterministic_warning` fires and
  cases run as informational results.
- Predicate extensions inside `evals`: `<ref> contains "<literal>"`,
  `<ref> contains @semantic.<Type>`, `tools.calls includes|excludes
  <tool-ref>`. Outside evals the predicate language is unchanged.
- `safety` accepts a list of `@validator.<name>` references (Cut A.5
  promotes the PII-coverage union check; Cut A reads the first).
- Doctor cross-checks: tool policy lattice, write-tool guards by
  `safety`, PII propagation from `@tool.*` registry entries,
  discriminator target/field validity, eval ordered-op operand types,
  eval determinism pin.

`lazuli inspect <file> --expand=tools` emits the per-agent dispatch
graph; `--expand=summary` extends with `evals`, `output_kind`,
`output_discriminator`, `eval_determinism`.

A `case` may also reference a golden file (Cut A.10):

```lazuli
evals
  case golden_quality
    requires output contains "active"
    golden "./evals/summarize_golden.jsonl" min_score 0.85
```

The runtime adapter loads the file and scores the agent's output;
`min_score` (0.0–1.0) gates the case. Omitting `min_score` falls
through to the adapter's default (0.85 by convention). Golden refs
coexist with `requires`/`forbids` assertions — both run; failing
either fails the case.

### `expose http` (Cut A.7)

Trivial agent-dispatch endpoints land directly on the agent — no
need for a sibling `api` block:

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
  policy @policy.read
  output stream Text
  model @llm.default
  prompt "./prompts/summarize.md"
  expose http
    method POST
    path "/api/customers/:customer_id/summary"
    route customer_id: Customer.ID
```

The agent's `policy`, `rate_limit`, and `output` apply to the exposed
endpoint without restating. `lazuli inspect --expand=expose` projects
the unified HTTP route table across every `api` block and every agent
with `expose http`. Doctor rejects cross-feature `(method, path)`
collisions and unknown audience references; LSP catches local
duplicates and slot binding mistakes.

`api` blocks remain for handlers that do meaningful work beyond agent
dispatch (multi-step orchestration, format transformation, calling
several agents). The boundary: "does the handler do work beyond
translating HTTP to agent dispatch?" If yes, keep `api`. If no,
`expose http` is the shortcut.

## CORS (Cut A.11)

`app.lzi` declares the browser-side CORS allowlist alongside `urls`.
The runtime materialises middleware from this block; doctor catches
origin/environment drift and the CORS-spec wildcard+credentials trap.

```lazuli
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"
    api production "https://api.example.com"

  cors
    allow_origins production "https://app.example.com", "https://*.example.com"
    allow_origins local "*"
    allow_credentials true
    max_age "1h"
```

Children: `allow_origins <env> "<origin>"[, "<origin>"]+` (required,
one or more lines), `allow_credentials true|false` (default `false`),
`max_age "<duration>"` (adapter default `1h`). Methods aren't
declared — runtime serves whatever `expose http` / `api` declare on
the matching path.

`allow_origins ... "*"` plus `allow_credentials true` rejects per
CORS spec (browsers refuse the combination). Per-endpoint CORS
overrides defer until pilot evidence shows the global allowlist
fails — the 80% case is one block matching declared URLs.

## Approval (Cut A.9)

Commands that need conditional human sign-off declare an `approval`
block. The runtime gates dispatch on the approval decision; agents
dispatching the command via `tools` satisfy the write-tool guard
without their own `safety` validator.

```lazuli
command reassign
  route id: ID
  input
    owner_id: User.ID required
  policy @policy.update
  approval
    required_when target.tier = enterprise
    by @role.admin
    timeout "24h"
    then deny
  updates Customer
    owner = resolved_owner
```

Required children: `by` (single `@role.<name>`) OR `chain` (ordered
approvers — not both), and `then` (`deny`, `allow`, or `escalate`).
Optional: `required_when` (closed predicate; omission means "always
required"), `timeout` (duration string), `sequential`.

For a multi-approver chain, use `chain [@role.a, @role.b]` with an
optional trailing `sequential` to enforce strict order:

```lazuli
approval
  required_when target.amount > 10000
  chain [@role.manager, @role.finance_admin] sequential
  timeout "48h"
  then escalate
```

Three guards now satisfy `agent_tool_write_unguarded_diagnostics`:
- agent `safety @validator.<name>` (Cut A baseline)
- target command `approval` block (Cut A.9 extension)
- `idempotency by ...` on the command (Cut B; reserved)

The three are not subsets of each other — `safety` is pre-flight
input scrub, `approval` is runtime gating, `idempotency` is replay-
safety. Pick by the threat shape, not by which is fewer keystrokes.

## Command Verbs (effects)

Beyond `creates`/`updates`/`deletes`, two more effect verbs:

- `reorder <Resource> by <position_field>` — batch position update; rewrites
  the integer position column across rows in one statement (`<Resource>` may
  be feature-qualified). Cardinality 0..1 per command.
- `audit ... materialize @feature.<f>.<OperationLog>` — sink the audit record
  into an `append_only` resource in another feature (reachable via `uses`).

```lazuli
command reorder_steps
  policy @policy.update
  reorder JobStep by position
  audit actor, ctx.now
    materialize @feature.audit.OperationLog
```

## Tests

Tests are inline IR assertions. They are optional by default and strict in
`lazuli check --strict-tests`.

| Construct | Verbs | Binding |
|-----------|-------|---------|
| command | authored: `allows`/`denies when <predicate>`; generated: `permits`/`forbids <actor>` from `policy @policy.*` | `target` |
| lifecycle transition | `allows`/`denies from <state>`; `allows`/`denies as <actor>`; combined | `self` |
| rule | `allows`/`denies when <predicate>` | `self` |
| extensible view | `accepted`/`rejected by <feature>` | none |

Do not copy command policy matrices into source. `lazuli inspect --expand=tests`
and runtime test generation derive `permits`/`forbids` from the effective
command policy. Authors write command tests only for rule/predicate behavior
that is not already stated by `policy @policy.*`.

No fixtures, mocks, event emission assertions, effect assertions, or
given/when/then framing in v0 tests.

## Security Checklist

Use these in source, not just Go/runtime. `lazuli check` defaults to
`--security-profile strict`, where missing security decisions are errors.
`--security-profile prototype` downgrades them to warnings while drafting.
`--security-profile production` additionally rejects explicit security
opt-outs such as `verify none` without a deployment allowlist.

- Every `command` requires explicit `policy`.
- Commands that mutate state or whose effective policy includes `@scope.public`
  require `rate_limit` or `rate_limit none` with a `reason "..."` child.
- Custom APIs declare `method`, `path`, `output`, `policy`, and `handler`.
- Query `cache` blocks declare `key` and `ttl`; command `invalidates` blocks
  point at explicit query targets.
- Feature `errors` blocks define client exposure; named error cases use
  `error <Name> status <http-status> expose message, code, data`.
- Command validators should be blocking: use `validate @validator.*`, or use
  `let result = @validator.*` plus `requires result`.
- Sensitive fields with `@pii.*`, `@cap.Encrypted`, `@cap.Hashed`,
  `@cap.E2ee`, or `@cap.Token` require field-level `read` and `write` policy.
- Top-level `env` declares every `env.NAME` reference with scope, type, and
  requiredness. Optional `group <name>` children organize related variables;
  client values use `PUBLIC_`, mobile values use `EXPO_PUBLIC_`.
- File fields use `@cap.File(max_size:<size>,accept:<mime>)`; the framework
  may generate upload UI later, but the language owns the storage contract.
- PII resources declare retention, e.g. `retention 7y then anonymize`.
- Closed-period protection is generic: use
  `write_window by input.issued_at within billing.open_period` on a command.
- `scope override` requires `policy @policy.*` and `reason "..."`.
- Every `webhook` requires `verify` and `idempotency by payload.*`; use a
  comma-separated payload key when uniqueness is tenant-scoped, e.g.
  `idempotency by payload.org_id, payload.external_id`.
- Webhooks in tenant-scoped features should declare `tenant_from payload.<axis>_id`
  or explicit `scope global` with a reason.
- Event-triggered jobs whose producer event declares `org_id` should declare
  `tenant_from payload.org_id`.
- Scheduled jobs in tenant-scoped features should declare `fanout tenants <axis>`
  or explicit `scope global` with a reason.
- Queries named `active_sessions` should prove temporal validity with
  `expires_at > ctx.now` or a modifier guarantee; a modifier name alone is not
  enough evidence.
- Event consumers may only read fields declared by the producer event contract,
  including inherited `event_group` payload fields.
- `escape_route` requires `policy` and `tenant`.
- `auth password` requires `algorithm` and `rate_limit`; `auth sessions`
  requires `ttl`.
- `auth sessions` may declare a `cookie` child block to override the
  session-cookie transport envelope. Six optional attributes — `name`,
  `same_site` (`lax` | `strict` | `none`), `secure`, `http_only`, `domain`,
  `path`. Any attribute you omit keeps the runtime default for that axis;
  set `secure true` whenever `same_site none`. The attribute vocabulary is
  shared with app-level `app.cookie` profiles.
- Mark sensitive fields and event payloads with `@pii.*`, `@cap.*`, and
  `@key.*`.
- Use canonical capability arguments: `@cap.Hashed(algorithm:argon2id)`,
  `@cap.Encrypted(key:@key.tenant)`, and
  `@cap.Token(ttl:1h,single_use:true,store:hashed)`.
- Prefer declarative webhook verification:

```lazuli
webhook crm_customer_upsert
  verify hmac sha256
    secret env.CRM_WEBHOOK_SECRET
    header "X-CRM-Signature"
  tenant_from payload.org_id
  idempotency by payload.org_id, payload.external_id
```

Crypto in Lazuli is a contract. Runtime adapters implement the primitives.

## Identity Hints

`previously` is a migration continuity hint, not permanent design prose:

```lazuli
resource Customer previously migrated Account
  lifecycle_stage previously migrated status: CustomerStatus = lead
```

Keep it inline while the compiler, semantic diff, or migration planner still
needs to connect a deployed/stored old identity to the current one. Remove it
after all supported environments have migrated and the stored IR baseline no
longer contains the old name. Do not move rename continuity to comments.
Use `previously alias <old_name>` only when generated compatibility surfaces
still accept the old name.

## Event Kinds

| Kind | Meaning |
|------|---------|
| `event` | domain event; may be used by `trigger event` |
| `event.trace` | observational signal; must not be used as a job trigger |
| `event_group` | same-feature payload template for matching concrete events |

`emits` works for both `event` and `event.trace`; only reaction graph behavior
differs.
Child assignments under `emits <event>` fill event-specific payload fields.
They do not replace fields inherited from `event_group`; inspect events to see
the full payload with provenance.

## Non-Goals

[v0] `non_goals` is a boundary dictionary:

```lazuli
non_goals
  delegated_to
    customer_auth: "customer login and MFA"
  out_of_scope
    generic_etl: "generic ETL platform"
```

`delegated_to` entries document ownership by another feature and may be
validated as feature ids. `out_of_scope` entries document design boundaries
that are not semantic dependencies. Direct keys and `anti_pattern.*` are legacy.

## Attach Context

[v0] `attach_ctx "<path>"` is a feature-header directive (alongside `purpose`
/ `non_goals`) that points the feature at a sidecar markdown context file the
agent / strict profile reads as authoring guidance. Quoted relative path,
cardinality 0..1.

```lazuli
feature catalog
  purpose "Discover and book lodging."
  attach_ctx "./ctx.md"
```

## Inspect Context Pack

Default agent context for editing a feature:

```bash
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=summary,refs,locators,dependencies,security --format=json
lazuli inspect examples/full-capsule/app.lzi --format=json
lazuli doctor examples/full-capsule
```

Use `--expand=events,policies,targets,tests` only when the task touches those
areas. JSON is the stable machine contract; `--format=lazuli` is a readable
projection.

## Do Not Add In v0

- `crud`, `assignment`, or `reacts to` macros.
- Cross-feature event re-emission.
- Open-by-default UI anchors.
- Predicate operators outside the closed predicate language.
- New `@...` namespaces without updating the closed catalog.
