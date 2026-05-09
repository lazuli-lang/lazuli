# Lazuli Quick Reference

This is the context pack to load first when an agent or a human needs to
author, review, or patch canonical `.lzi`/`.lzx` files. It is intentionally short.
Use `docs/canonical-semantics.md` for the full normative reference and
`docs/invariants.md` for the checker/codegen contract.

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

```lazuli
experience ping
  imports ping

  view list
    source ping.query.list
    action create -> ping.command.create
```

```lazuli
surface ping web
  uses experience ping

  audience admin
    view list Table
      columns message, created_at
```

## Canonical Order

`.lzi` feature block order:

```txt
meta -> defaults -> uses -> refs? -> domain -> policies -> auth -> command
-> workflow -> job -> webhook -> surface -> extensions -> escape_route
```

`meta` means `purpose`, `non_goals`, and `context`.
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

[v0] `.lzx` has no cascade or partial override. Do not write
`columns += score`; redeclare the whole view for that audience/tenant.

## Policy Vocabulary

Lazuli has three distinct policy concepts. Do not collapse them:

| Shape | Meaning | Example |
|-------|---------|---------|
| `policies` block | feature-local policy category dictionary | `update: @role.admin, @role.sales` |
| `policy ...` statement | construct references one category | `policy @policy.update` |
| policy atom | terminal auth predicate/executor/role | `@role.admin`, `@scope.same_org`, `@actor.system` |

[v0] Commands, workflows, and queries with local policy should reference
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
| `@adapter.*` | integration adapters |
| `@query_modifier.*` | query modifiers |
| `@anchor.*` | view composition anchors |

Unknown namespaces are errors unless the spec adds them.

## Binding Namespaces

| Construct | Available bindings |
|-----------|--------------------|
| `query.*` | `params.*`, `ctx.*` |
| `command` | `route.*`, `input.*`, `ctx.*`, `target` after explicit or inferred target |
| declarative event job | `envelope.*`, `payload.*`, `ctx.*`, `target` after `target ...` |
| schedule job | `schedule.*`, `ctx.*` |
| webhook | `payload.*`, `ctx.*` |
| rule | `self`, `ctx.*` |
| workflow transition tests | `self`, `ctx.*` |
| command tests | `target`, `ctx.*` |

`target` is the immutable entity loaded by a command or declarative job.
`self` is the snapshot evaluated by rules and workflow predicates.

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
view detail SidePanel id @anchor.customer_detail
  extensible_by customer_tags, customer_import
```

Use `inspect --expand=summary` to list provided anchors and
`inspect --expand=dependencies` to list features that extend them.

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
`query.list` defaults to `order created_at desc`; declare `order` only when a
query intentionally differs from newest-first listing.
Simple equality filters derive language-managed indexes. With `tenancy org`,
`status when params.status` derives `org, status`, and
`customer.id = params.customer_id` derives `org, customer`. Search, `has`,
`!=`, `nil`, `scope override`, and SQL queries do not derive indexes.

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

## Tests

Tests are inline IR assertions. They are optional by default and strict in
`lazuli check --strict-tests`.

| Construct | Verbs | Binding |
|-----------|-------|---------|
| command | authored: `allows`/`denies when <predicate>`; generated: `permits`/`forbids <actor>` from `policy @policy.*` | `target` |
| workflow transition | `allows`/`denies from <state>`; `allows`/`denies as <actor>`; combined | `self` |
| rule | `allows`/`denies when <predicate>` | `self` |
| extensible view | `accepted`/`rejected by <feature>` | none |

Do not copy command policy matrices into source. `lazuli inspect --expand=tests`
and runtime test generation derive `permits`/`forbids` from the effective
command policy. Authors write command tests only for rule/predicate behavior
that is not already stated by `policy @policy.*`.

No fixtures, mocks, event emission assertions, effect assertions, or
given/when/then framing in v0 tests.

## Security Checklist

Use these in source, not just Go/runtime:

- `scope override` requires `policy @policy.*` and `reason "..."`.
- Commands whose effective policy includes `@scope.public` should declare
  `rate_limit`.
- Event-triggered jobs whose producer event declares `org_id` should declare
  `tenant_from payload.org_id`.
- Event consumers may only read fields declared by the producer event contract,
  including inherited `event_group` payload fields.
- Mark sensitive fields and event payloads with `@pii.*`, `@cap.*`, and `@key.*`.
- Use canonical capability arguments: `@cap.Hashed(algorithm:argon2id)`,
  `@cap.Encrypted(key:@key.tenant)`, and
  `@cap.Token(ttl:1h,single_use:true,store:hashed)`.
- Prefer declarative webhook verification:

```lazuli
webhook crm_customer_upsert
  verify hmac sha256
    secret env.CRM_WEBHOOK_SECRET
    header "X-CRM-Signature"
```

Crypto in Lazuli is a contract. Runtime adapters implement the primitives.

## Identity Hints

`previously` is a migration continuity hint, not permanent design prose:

```lazuli
resource Customer previously Account
  lifecycle_stage previously status: CustomerStatus = lead
```

Keep it inline while the compiler, semantic diff, or migration planner still
needs to connect a deployed/stored old identity to the current one. Remove it
after all supported environments have migrated and the stored IR baseline no
longer contains the old name. Do not move rename continuity to comments.

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

## Inspect Context Pack

Default agent context for editing a feature:

```bash
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=summary,refs,locators,dependencies,security --format=json
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
