# Lazuli Invariants

This document lists language invariants that tools should enforce before code
generation. It is intentionally shorter than the full canonical semantics spec:
agents and alternative implementations can read this first to avoid producing
source that only fails later.

## Source And Derived Views

- Canonical `.lzi` source is authored. IR, inspect JSON, generated summaries,
  manifests, and generated code are derived.
- `summary` is never authored in `.lzi`; use
  `lazuli inspect --expand=summary`.
- `refs` is optional and documentary. When present, it must match namespaces
  used by that feature; it does not change resolution.
- Do not author `refs` merely to list core `@...` namespaces; those prefixes
  are self-describing and `inspect --expand=refs` can generate the manifest.
- `lazuli inspect --format=json` is the stable inspect contract. Human
  projections such as `--format=lazuli` are derived views over that contract.
- `summary`, `refs`, `events`, `policies`, `locators`, and `dependencies`
  inspect expansions are the recommended context pack for agents editing a
  feature.
- Every inspect expansion fact must carry provenance through an `origin` field
  or an equivalent typed source marker.

## Namespaces

- `@...` references use the closed namespace catalog: `@role`, `@scope`,
  `@actor`, `@policy`, `@semantic`, `@cap`, `@pii`, `@key`, `@client`, `@fn`, `@hook`,
  `@validator`, `@adapter`, `@query_modifier`, and `@anchor`.
- Unknown `@...` namespaces are errors.
- Extension declaration keywords match call-site namespaces: `fn risk_score`
  resolves as `@fn.risk_score`, `validator verify_totp` resolves as
  `@validator.verify_totp`, and so on.
- `@semantic.*` is for built-in validation or formatting behavior.
- `@cap.*` is for runtime capabilities that require platform handling in at
  least two targets, such as storage, redaction, upload, hashing, encryption,
  token storage, or secret handling.
- `@pii.*` marks data classification for redaction, events, retention, export,
  and erasure behavior.
- `@key.*` marks cryptographic key scope; crypto primitives remain runtime
  library concerns, not DSL implementations.

## Policies

- Policy atoms in `policies` are namespaced by semantic category: `@role.*`,
  `@scope.*`, or `@actor.*`.
- Feature-local policy categories are referenced with `@policy.*`.
- `policy @policy.update` resolves inside the current feature unless a
  feature-qualified policy reference is used.
- Commands and workflows use `policy @policy.*`; put direct `@role.*`,
  `@scope.*`, and `@actor.*` atoms in the `policies` dictionary. Jobs,
  webhooks, escape routes, and defaults may use direct atoms where appropriate.
- Do not replace canonical command/workflow policy references with bare
  `policy create`/`policy update`. The `@policy.*` prefix is deliberate: it
  distinguishes feature-local authorization categories from write effects,
  verbs, actors, roles, and scopes, and gives tools a clear syntactic boundary.
- Commands declare `policy` explicitly. There is no implicit
  `creates -> @policy.create` or `updates -> @policy.update` rule.
- Workflow `policy` is a transition default. A transition uses
  `requires @policy.<name>` for stronger authority; transition-level `policy`
  is not canonical v0.

## Targets And Bindings

- `target` is the immutable record loaded by a command or declarative job
  `target` lookup.
- `self` is reserved for rule and workflow transition snapshots.
- A mutating command may omit `target query.by_id(id: route.id)` only when it
  has `route id: ID`, exactly one local mutating effect, and a local
  `query.lookup by_id`.
- Cross-feature targets are always explicit and feature-qualified.
- `route.*`, `input.*`, `params.*`, `payload.*`, `envelope.*`, `schedule.*`,
  `ctx.*`, `target`, and `self` are distinct locator spaces.
- Query `params` are read arguments, command `route` slots are path/context
  locators, and command `input` slots are submitted body fields.
- Short-list `input name, email` is allowed only as a resource-field shorthand
  for the single local `creates` or `updates` resource. All other caller shapes
  use typed input blocks.

## Events

- `event_group <pattern> on <Resource>` is a same-feature payload template for
  nested concrete `event` and `event.trace` declarations.
- Nested event names are appended to the group prefix:
  `event created` under `event_group customer_*` declares
  `customer_created`.
- Concrete events do not restate their matching `event_group`; use
  `lazuli inspect --expand=events` for isolated event documentation with
  inherited payload provenance.
- `event.trace` is outside the feature-to-feature reaction graph. Jobs should
  not use trace events as `trigger event`; promote the trace to ordinary
  `event` first.
- `emits` can publish either `event` or `event.trace`; the distinction affects
  graph/subscriber analysis, not publishing syntax.
- Event-triggered jobs use `envelope.*` for bus metadata and `payload.*` for
  producer-authored fields.
- Event consumers may only reference payload fields declared by the producer
  event contract, including fields inherited from matching `event_group`
  templates.
- If an event contract includes `org_id`, event-triggered jobs should declare
  `tenant_from payload.org_id` so generated handlers run with a fixed tenant
  context.
- Use `idempotency by envelope.id` for per-delivery processing and
  `idempotency by payload.<business_key>` for business-key dedupe.
- `retry <count>` means retry attempts after the initial failed attempt.

## Queries And Relations

- `paginate <n>` is the generated default page size for list queries, not a
  hard product maximum.
- `query.list` defaults to `order created_at desc`; explicit `order` is used
  only when a query intentionally differs.
- Simple `query.list` equality filters derive language-managed indexes,
  tenant-prefixed when the feature has a single tenant axis.
- Search filters, collection `has`, inequality, nil checks, `scope override`,
  `query.sql`, and modifiers do not derive indexes.
- `scope override` is an absolute replacement of inherited safety scope. Use it
  only for explicitly cross-tenant or admin queries.
- Queries that use `scope override` must declare explicit `policy @policy.*`.
- Queries that use `scope override` should include a `reason "..."` child
  explaining why inherited tenant/soft-delete scope is intentionally replaced.
- The default hard-delete behavior for resource references is
  `on_delete restrict`. Soft delete does not cascade automatically.
- SQL-backed query return types must resolve to a resource, `record`, or
  registered external contract before code generation; Lazuli does not infer
  result shape from SQL text.

## Security And Crypto

- Commands whose effective policy includes `@scope.public` should declare a
  command-level `rate_limit`.
- `@cap.Secret` is legacy; choose an explicit tier:
  `@cap.Hashed(algorithm:<name>)`, `@cap.Encrypted(key:@key.<scope>)`,
  `@cap.E2ee(key:@key.<scope>)`, or
  `@cap.Token(ttl:<duration>,single_use:true|false,store:hashed)`.
- `@cap.Hashed` declares an algorithm.
- `@cap.Encrypted` and `@cap.E2ee` declare a `@key.*` scope.
- `@cap.Token` declares TTL, single-use behavior, and storage strategy.
- Declarative webhook verification (`verify hmac sha256`, with `secret` and
  `header`) is preferred for common HMAC providers; custom `verify "./path.go"`
  remains the escape hatch.

## Metadata

- `non_goals` entries are grouped under `delegated_to` or `out_of_scope`.
- `delegated_to` entries may reference feature ids but do not count as `uses`.
- `out_of_scope` entries are product/design boundaries, not dependencies.

## Validation

- Whole-resource inline validators use `validates resource "./path.go"`.
- Field-scoped inline validators use
  `validates field <name> "./path.go"`.
- Legacy `validate "./path.go"` and `validates <field> "./path.go"` are
  compatibility forms only.

## Tests

- `tests` blocks are optional and inline.
- Tests are allowed only on commands, workflow transitions, rules, and
  extensible views.
- Tests cover decisions with inference: rule predicates, transition validity,
  and anchor allowlists. They do not restate effects or emitted events.
- Command actor-matrix tests are generated from the effective
  `policy @policy.*`; authored command tests should cover only predicate
  behavior beyond policy.
- Generated command actor-matrix tests use `permits` and `forbids`; authored
  predicate and transition tests use `allows` and `denies`.
- Keep tests vocabulary scoped by construct. Rule predicates, workflow edges,
  workflow actor checks, and anchor allowlists are different semantic decisions;
  do not flatten them into one generic `allow/deny` dialect unless it preserves
  the same static rejection power and readability.
- Command tests use `target` when a loaded target exists. Rule and workflow
  tests use `self`.
- Tests use the same closed predicate language as rules and filters; no
  fixtures, mocks, or `given/when/then` framing.

## Authored Shape

- Features are product capabilities, not entity buckets.
- `uses` is strict: every listed feature should be referenced by a semantic
  edge, not just mentioned conceptually.
- `previously` is a migration continuity marker. Remove it once all supported
  baselines no longer contain the old identity.
- `escape_route` is explicit and still declares its route, policy, tenant
  boundary, and source path.
