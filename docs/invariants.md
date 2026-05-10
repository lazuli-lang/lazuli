# Lazuli Invariants

This document lists language invariants that tools should enforce before code
generation. It is intentionally shorter than the full canonical semantics spec:
agents and alternative implementations can read this first to avoid producing
source that only fails later.

## Source And Derived Views

- Canonical `.lzi` source is authored. IR, inspect JSON, generated summaries,
  manifests, and generated code are derived.
- `.lzi` owns the domain/capability contract. It must compile without any UI
  source present.
- Top-level `.lzi app` owns the app entrypoint, generated targets,
  environments, URLs, runtime units, provider-neutral deploy gates, and logical
  service boundaries. It is not a product feature and should not hide domain
  behavior.
- Top-level `.lzi registry` owns package-level env schema, capabilities,
  integrations, adapters, packs, and global bindings. Small apps may keep
  registry-shaped blocks in `app.lzi`, but `registry.lzi` is the preferred
  package convention once the app root starts getting noisy.
- Env declarations may use `group <name>` for organization, but env names
  are still explicit global schema entries. `required in <environment>` scopes
  requiredness to named app environments without creating provider-specific
  config inside the manifest.
- Registry `integrations` declare provider-neutral external integration registry
  entries: name, capability kind, adapter reference, environments, and
  credential scope. They do not declare provider HTTP operations, SDK methods,
  or cloud secret storage.
- Feature `requires integration <slot>: <CapabilityType>` declares dependency
  inversion at the feature boundary. It names an abstract slot the feature can
  call later; it must not import or select a concrete provider directly.
- App `bindings` resolve abstract feature slots to concrete integration
  registry entries with `<feature>.<slot> = integrations.<name>` or
  `<feature>.<slot> = registry.integrations.<name>`. The integration kind must
  match the required capability type.
- App `services` declare logical ownership boundaries. They do not by
  themselves require separate processes; Drusa decides whether the same
  boundary graph runs as a monolith, modular monolith, or split services.
- Abstract `.lzx` owns the experience/view model and imports `.lzi`
  capabilities.
- Top-level `.lzx route` owns concrete web paths/mobile route patterns. Dynamic
  path segments such as `:id` or `[id]` declare typed `params` and bind those
  params into an abstract view through `to ...`. `path` is canonical for both
  web and mobile routes; legacy `stack` is compatibility syntax only.
- Concrete `.web.lzx` and `.mobile.lzx` own platform projections and use an
  abstract experience. Platform suffixes are protected compound suffixes: the
  platform segment stays immediately before `.lzx`. Product axes such as
  `audience` and `tenant` are source syntax, not magic filename suffixes.
- `.lzx` forbids cascade and partial override operators such as `+=` and `-=`.
  Product variants redeclare the whole view they change.
- `lazuli check` is file-local. `lazuli doctor` loads the package set and
  checks invariants that require both `.lzi` and `.lzx`, including surface
  audience reachability against command policy atoms.
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
- Command routes may bind from context with `route <name>: <Type> from ctx.*`;
  otherwise callers must supply required route slots explicitly.
- Short-list `input name, email` is allowed only as a resource-field shorthand
  for the single local `creates` or `updates` resource. All other caller shapes
  use typed input blocks.
- `validate @validator.*` is a blocking command validator. A validator result
  bound with `let` must be used by `requires <binding>`.
- Cross-feature view extensions target explicit anchor slots. Legacy direct
  `block` children under `extends @anchor.*` are accepted for authoring
  compatibility but should warn because placement and ordering are implicit.

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
- `emits` is explicit. Lazuli does not infer event publication from command,
  workflow, or job names because events are contracts and reaction-graph edges,
  not logging conventions.
- Shared event payload belongs in `event_group ... payload`; canonical v0 has
  no hidden feature-level actor or tenant payload defaults. Repeated payload
  shape should become a named language primitive only after it proves universal.
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
- Use comma-separated idempotency keys when uniqueness depends on more than
  one payload value, such as `payload.org_id, payload.external_id`.
- Webhooks in tenant-scoped features declare `tenant_from payload.<axis>_id` or
  explicit `scope global` with a reason.
- Scheduled jobs in tenant-scoped features declare `fanout tenants <axis>` or
  explicit `scope global` with a reason.
- `retry <count>` means retry attempts after the initial failed attempt.

## Queries And Relations

- `paginate <n>` is the generated default page size for list queries, not a
  hard product maximum.
- `paginate` is valid only under `query.list` and must be a positive integer.
- `query.list` defaults to `order created_at desc`; explicit `order` is used
  only when a query intentionally differs.
- Simple `query.list` equality filters derive language-managed indexes,
  tenant-prefixed when the feature has a single tenant axis.
- Declarative `search`, collection `has`, inequality, nil checks,
  `scope override`, `query.sql`, and modifiers do not derive indexes.
- Text matching uses `search params.<name> over <fields...>`; do not encode a
  contains search as `field = params.search`.
- `active_sessions` queries prove temporal validity with `expires_at > ctx.now`
  or a modifier guarantee; a modifier name alone is not enough.
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

- Lazuli grows security by local contracts, not a feature-level `security`
  checklist. The source declares security at the operation/field boundary where
  it matters; `lazuli inspect --expand=security` derives the audit view.
- `lazuli check` defaults to `--security-profile strict`: omitted security
  decisions are errors. `prototype` downgrades those errors to warnings for
  drafting. `production` keeps strict errors and treats explicit opt-outs as
  deployment blockers.
- Every command declares local `policy`; commands do not inherit permissive
  effect-derived defaults.
- Commands that mutate state or whose effective policy includes `@scope.public`
  declare a command-level `rate_limit` or explicit `rate_limit none` with a
  `reason "..."` child.
- Custom `api` declarations expose typed HTTP boundaries and declare `method`,
  `path`, `output`, `policy`, and `handler`. Dynamic path params declare
  matching `route` slots.
- Query cache contracts declare both `key` and `ttl`. Command invalidation
  contracts list explicit query targets under `invalidates`.
- Feature-level `errors` defaults decide public/private client exposure. Named
  error cases declare HTTP status and exposed payload fields.
- Sensitive fields marked with `@pii.*`, `@cap.Encrypted`, `@cap.Hashed`,
  `@cap.E2ee`, or `@cap.Token` declare field-level `read` and `write` policy.
- A top-level `env` block declares every `env.NAME` reference with scope,
  type, and requiredness. Optional `group <name>` children organize related
  variables without changing the reference name. Client-exposed names use
  `PUBLIC_`; Expo/mobile names use `EXPO_PUBLIC_`.
- Registry `integrations` entries use `<name>: <CapabilityType>` with
  `adapter @adapter.<name>` and `credentials platform|tenant|actor`. Provider
  operation details belong in features, packs, or adapters, not the registry.
- File fields use `@cap.File(max_size:<size>,accept:<mime>)`; upload UI and
  providers are framework/adapters, but size and MIME acceptance are language
  contracts.
- Resources that store `@pii.*` fields declare `retention <duration|forever>
  then delete|anonymize|archive`, or inherit a default retention contract.
- Commands may declare `write_window by <date-expression> within
  <window-reference>` for closed-period style protection. The construct is
  temporal and generic; fiscal/accounting periods are packs or adapters.
- Every webhook declares verification and idempotency. `verify none` is an
  explicit opt-out and must carry a `reason "..."` child.
- `escape_route` declares `policy` and `tenant` because it is outside generated
  UI ownership.
- `auth password` declares `algorithm` and `rate_limit`; `auth sessions`
  declares `ttl`.
- `@cap.Secret` is legacy; choose an explicit tier:
  `@cap.Hashed(algorithm:<name>)`, `@cap.Encrypted(key:@key.<scope>)`,
  `@cap.E2ee(key:@key.<scope>)`, or
  `@cap.Token(ttl:<duration>,single_use:true|false,store:hashed)`.
- `@cap.Hashed` declares an algorithm.
- `@cap.Encrypted` and `@cap.E2ee` declare a `@key.*` scope.
- `@cap.Token` declares TTL, single-use behavior, and storage strategy.
- Capability arguments are closed: token TTL is `<integer><s|m|h|d>`,
  `single_use` is `true|false`, token `store` is `hashed` in v0, hash
  `algorithm` is `argon2id` canonically (`bcrypt` only for legacy migration),
  and encryption keys are `@key.*` references.
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
- Reusable validators used inside commands are called through
  `validate @validator.*` or through `let` plus `requires`.

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
- `previously` is a migration continuity marker and must say whether the old
  name is `migrated` history or a temporary compatibility `alias`. Remove it
  once all supported baselines no longer contain the old identity.
- `escape_route` is explicit and still declares its route, policy, tenant
  boundary, and source path.
