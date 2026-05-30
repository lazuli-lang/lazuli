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
- The word "gate" appears at two unrelated scopes in Lazuli and authors must
  not conflate them. **App-level deploy gates** are free-form English in
  `app.lzi deploy { ... }` and `Lazurite.toml` prose (release-promotion
  policy: migrations, destructive_migrations, rollback). **Callable-scope
  `gate` directives** (`gate behind plan.feature: ...` /
  `gate quota plan.limit: ...`) are children of a single
  command/query/job/webhook/poller/api block and bind to the package-wide
  `plan` catalog. The two never collide syntactically: `gate` is a child
  keyword of a callable, never of `deploy`; `deploy` does not accept a
  `gate` child. Doctor `PLAN-FEATURE-UNDECLARED-001` and friends only fire
  inside callable bodies. See the `plan-and-gate-vocab` proposal (operational archive).
- Top-level `.lzi workspace` is optional and owns distributed-system contracts:
  local app entrypoints, external service contracts, shared registry path,
  event publication/consumption edges, context propagation defaults, and public
  gateway routes. It is not required for normal apps.
- Workspace apps may be Lazuli packages or external implementations in
  another language. External apps must reference a contract; Lazuli validates
  the contract graph, while the Go runtime materializes transport bindings.
- Top-level `.lzi contract <name>` owns imported or authored external service
  schemas. It may import OpenAPI, AsyncAPI, Proto, JSON Schema, or Avro and may
  author records, operations, and events. It does not describe SDK generation,
  provider clients, broker endpoints, or implementation code.
- Workspace gateways route to app ids and should declare `auth propagate` and
  `tenant propagate`. Gateway/proxy providers, service mesh, broker providers,
  repo URLs, branches, ports, and deploy mechanics stay outside Lazuli.
- Top-level `.lzi registry` owns package-level env schema, capabilities,
  integrations, adapters, packs, and global bindings. Small apps may keep
  registry-shaped blocks in `app.lzi`, but `registry.lzi` is the preferred
  package convention once the app root starts getting noisy.
- Env declarations may use `group <name>` for organization, but env names
  are still explicit global schema entries. `required in <environment>` scopes
  requiredness to named app environments without creating provider-specific
  config inside the manifest.
- Top-level `.lzi profile <environment>` owns environment-specific overrides:
  public URLs, binding overrides, integration environment/adapter selection,
  and provider-neutral deploy topology/gates. Profile names must match app
  `environments`. Profiles do not contain secret values or cloud/provider
  infrastructure details.
- Registry `integrations` declare provider-neutral external integration registry
  entries: name, capability kind, adapter reference, environments, and
  credential scope. They do not declare provider HTTP operations, provider
  client/SDK methods, or cloud secret storage.
- Integration adapter references must declare provenance through their shape:
  `@runtime/...`, `@plugin/<publisher>/<name>`, `@adapter.<local>`, or a local
  path. Inspect exposes derived `adapter_provenance`; the runtime owns construction,
  lifetimes, test doubles, and runtime dependency injection mechanics.
- Registry `packs` declare reusable package entries: name, source, optional
  version, provided artifacts, and abstract requirements. They do not inline
  product implementation, provider payload schemas, generated files, or runtime
  adapter mechanics.
- App `packs` enable registry packs with `<alias> from registry.packs.<name>`.
  Enabled packs may satisfy app `uses` entries and may add abstract integration
  requirements that app/profile bindings must resolve.
- Feature `requires integration <slot>: <CapabilityType>` declares dependency
  inversion at the feature boundary. It names an abstract slot the feature can
  call later; it must not import or select a concrete provider directly.
- App `bindings` resolve abstract feature slots to concrete integration
  registry entries with `<feature>.<slot> = integrations.<name>` or
  `<feature>.<slot> = registry.integrations.<name>`. The integration kind must
  match the required capability type.
- Lazuli integration constructs are contracts only. the runtime materializes them as
  Go runtime wiring, and Go adapters perform real HTTP/RPC/event/webhook work.
  React and Expo clients should consume generated app APIs, not provider
  integrations directly.
- `calls <slot>.<operation>` is valid inside commands and jobs only when
  `<slot>` is declared by `requires integration <slot>: <CapabilityType>`.
  Call children bind named arguments with `name = expression`. The surrounding
  operation should declare `timeout`, `retry`, and, for job side effects,
  `idempotency by ...` so generated Go transport bindings have explicit
  failure behavior.
- App `services` declare logical ownership boundaries. They do not by
  themselves require separate processes; the runtime decides whether the same
  boundary graph runs as a monolith, modular monolith, or split services.
- Abstract `.lzx` owns the experience/view model and imports `.lzi`
  capabilities.
- Top-level `.lzx route` owns concrete web paths/mobile route patterns. `path`
  is canonical for both web and mobile routes. Dynamic path segments such as
  `:id` or `[id]` declare typed `route <name>: <Type>` slots and bind those
  slots into an abstract view through `to ...(name: route.<name>)`.
- App `auth_failed_redirect` and `not_found` reference top-level `.lzx route`
  names declared in this package. `lazuli doctor` rejects references to routes
  that do not exist.
- **Route guard alignment**: A view's resolved guard policy must be at least as
  strict as every backend command/query its `submit` or `source` references.
  Codified by ROUTE-GUARD-002.
- Resource fields may use `<name>: <Type> derived from <expression>` to declare
  read-time computed values. Derived fields are not persisted, must not declare
  `default`, `required`, or `optional`, and must not appear as input/effect
  targets in `creates`/`updates`.
- Commands, queries, jobs, and webhooks may declare an explicit `audit` child:
  `audit` for default fields, `audit <field>, <field>` for specific fields, or
  `audit none` to opt out. Audit declarations surface in
  `lazuli inspect --expand=security` so audit-log generation has a typed
  contract instead of relying on event-name conventions.
- Resources may declare collection edges with
  `has_many <name>: <Type> [inverse <field>]`. The optional `inverse` names the
  field on the target resource that owns the foreign key. the runtime generates the
  inverse query and FK contract; Lazuli only owns the relationship contract.
- Queries declare their kind in the header: `query.list <name>`,
  `query.lookup <name>`, or `query.sql <name>`. The bare `query <name>` form
  is rejected so cold-readers see the kind before processing the body.
  Inspect locators report the kind as `query.<list|lookup|sql>`.
- `agent <name>` declares an LLM-powered capability with typed input, context,
  policy, rate limit, output, model reference, prompt template, optional tool
  list, optional safety classifier list, optional eval cases, and discriminated
  output forms. Required children: `policy @policy.<name>`, `output <form>`
  (one of `stream <Type>`, `discriminator <Enum>`, or a bare `<Type>` —
  the bare form lowers to `text` and is promoted to `discriminated_record`
  by the analyzer when the type resolves to a record carrying a
  `discriminator` field), `model @llm.<name>`, `prompt "./path"`. Optional
  model config siblings: `temperature` (0.0-2.0), `top_p` (0.0-1.0),
  `max_tokens` (positive integer), `seed` (integer). Optional Cut A
  children:
  - `tools` — indent-6 list of `<feature>.<kind>.<name>` /
    `<kind>.<name>` shorthand / `@tool.<dotted>` references. Effect
    (`read | write`) is derived from the underlying capability; the
    proposal forbids re-declaring at the binding. Doctor cross-checks
    policy compatibility, write-tool guarding by `safety`, and PII
    propagation from the registry-side `@tool.*` entries.
  - `evals` — indent-6 `case <name>` blocks with `allows`/`denies`
    assertions over the closed predicate language extended (only inside
    evals) with `<ref> contains <"literal"|@semantic.<Type>>` and
    `tools.calls includes|excludes <tool-ref>`. Eval cases gate CI only
    when the agent declares both `temperature 0` and `seed <int>`; doctor
    warns `eval_nondeterministic_warning` otherwise.
  - `safety` accepts one or more `@validator.<name>` references (Cut A
    sees the first; Cut A.5 widens to PII coverage union check).
  - The runtime auto-emits the canonical built-in trace event
    `agent_run` per dispatch (Cut A.8). The event name + payload
    schema is reserved by the IR; authored `event.trace agent_run`
    declarations are rejected, and subscriber jobs referencing
    payload fields that don't exist in the canonical schema get
    `agent_run_subscriber_payload_drift_diagnostics`. The runtime
    instruments dispatch and captures tokens/duration/cost;
    adapters export to OpenTelemetry/file/stdout.
  - `expose http` (Cut A.7) auto-mounts the agent as an HTTP endpoint
    with the agent's policy / rate_limit / output applied at the
    gateway. Required children: `method <GET|POST|PUT|PATCH|DELETE>`,
    `path "<url>"`. Optional: `route <slot>: <Type>` (one per URL
    placeholder), `audience <name>`, `rate_limit "<override>"`.
    Authors who used `api customer_summary_stream` style boilerplate
    next to a trivially-dispatching agent should collapse it into
    `expose http` on the agent. Doctor cross-checks path collisions
    across features and against `api` blocks; LSP catches local path
    duplicates, unbound `:slot` placeholders, `input`/`route` slot
    misuse, and `method GET` paired with `output stream`.
  Lazuli owns the contract; the runtime wires the LLM transport,
  prompt-template loading, and tool dispatch.
- `command <name>` may declare an optional `approval` block that
  gates dispatch on conditional human sign-off (Cut A.9). Required
  children: `by @role.<name>[, @role.<name>]+`, `timeout "<duration>"`,
  `then deny | proceed`. Optional child: `required_when <predicate>`
  (omission means "always required"). The closed catalog rejects `by`
  entries that aren't `@role.<name>` references — approvers are roles,
  not scopes. Doctor verifies every `@role.<name>` resolves against
  the workspace policy atom set. Cut A.9 extends
  `agent_tool_write_unguarded_diagnostics`: write-effect tools whose
  target command carries `approval` satisfy the guard without the
  agent's own `safety` validator. The runtime owns the approval UX +
  persistence; adapters handle transport.
- `app.lzi` may declare an optional `cors` block (Cut A.11) carrying
  per-environment allowlists. Required children: `allow_origins
  <environment> "<origin>"[, "<origin>"]+` (one or more lines).
  Optional: `allow_credentials true | false` (defaults `false`),
  `max_age "<duration>"` (adapter default applies when omitted).
  Methods aren't declared — the runtime serves whatever `expose http`
  / `api` declare on the matching path. Doctor cross-checks: every
  environment must appear in `app.environments`; non-wildcard origins
  should match a declared `url <target> <env> ...`; `allow_origins
  ... "*"` paired with `allow_credentials true` rejects (CORS spec).
- `notification <name>` declares a multi-channel outbound notification.
  Required children: `channel <email|push|sms|in_app>[, ...]`,
  `recipient <expression>`, `trigger event <pattern>`, `template "./path"`,
  `policy @policy.<name>`. Optional: `tenant_from`, `idempotency by`, `retry`,
  `rate_limit`, `emits`, `digest`, `throttle`. Lazuli owns the dispatch
  contract; the runtime generates wiring; adapters
  (Sendgrid/SES/Twilio/APNs/FCM) handle transport.
- `notification.throttle` is a sub-block, distinct from the scalar
  `rate_limit "N per <window>"` slot reserved across
  `agent` / `auth password` / `command` / `expose http` for per-call
  limits. `throttle` keys on the notification's recipient/channel axes
  (`per_recipient`, `per_channel`, `burst <N>`, `max_per
  "<duration>"`), not on the caller — two distinct keywords by design
  so an LLM reading source cold sees the contract axis without
  cross-referencing docs. Doctor: `NOTIF-THROTTLE-001/002/003`.
- `notification.digest` is a sub-block declaring window-based
  aggregation: `every "<duration>"`, `group_by <payload-path>`,
  `max_size <N>` (1..=10000), `template_strategy merge|append`. The
  adapter batches per-trigger payloads keyed on `group_by` and emits
  a single rendered template per window. Doctor:
  `NOTIF-DIGEST-001/002/003`. `delivery_receipt` / `read_receipt`
  remain SPECULATIVE pending pilot pressure (per-provider outcome
  catalogs + cross-channel polysemy block the closed-catalog gate
  today).
- `channel <name>` declares a typed, tenant-scoped, policy-gated
  realtime push stream (realtime bucket cycle MVP). Required
  children: `tenant_from <axis>`, `policy @policy.<name>`,
  `payload <RecordType>`. The payload must resolve to a `record`
  or `resource` in the same feature (doctor:
  `CHANNEL-PAYLOAD-001`). Transport (WebSocket / SSE) is
  adapter-resolved at runtime; the language declares the contract.
  `presence`, `subscription`, `broadcast`, and surface `subscribe`
  locator remain SPECULATIVE pending ≥3-app pilot pressure per
  `docs/scope-discipline.md`. Provider names
  (Pusher / Ably / Supabase Realtime) live in `@plugin/<vendor>`
  repos. The `channel` keyword overlaps with `notification.channel`
  (delivery-list child); the two are disambiguated by indent level
  + parent kind.
- Validators are referenced through `validates @validator.<name>`. The
  validator's `Validator[<scope>]` type in `extensions` declares the scope —
  field (`Validator[Resource.field]`) or whole-resource
  (`Validator[Resource]`). The legacy forms `validates field <name>
  @validator.<name>` and `validates resource @validator.<name>` are still
  parsed but warn because the scope keyword duplicates the validator's typed
  declaration. Inline `"./path.go"` references warn for the same reason.
- `previously migrated|alias <old>` is a child of the block it migrates,
  not inline on the header line. This applies uniformly to fields,
  resources, commands, workflow transitions, and any other named block:
  keep one concept per line so cold-readers process the kind + name first
  and then the migration history. Legacy inline forms still parse but warn.
- Cache invalidation entries accept fully qualified queries
  (`<feature>.query.<name>` or `<feature>.query.*` wildcard) and same-feature
  short forms (`query.<name>` or `query.*`).
- Commands may emit events with payload bindings derived from the surrounding
  effect: `emits <event> from creates|updates|deletes` declares that the Lazuli runtime
  should derive the event payload from the cited effect's bindings by name
  match. The derived form rejects an inline body, since duplicated bindings
  defeat the contract.
- External `contract` operations may declare AI-first transport dimensions on
  top of `transport` / `method` / `path` / `input` / `output` / `auth` /
  `timeout`. Supported children: `output stream <Type>`,
  `retry <count> [backoff <strategy>]`, `idempotency by <field>[, ...]`, and
  `error <Name> [status <code>] [expose <field>...]`. Error fields inside
  `contract` operations expose schema-defined keys, not the
  `message|code|data` envelope used by feature commands.
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

## Cross-Feature Contracts

- In any capsule that may transition to `architecture mode microservices`,
  cross-feature references must be expressible as contracts. Types,
  query return shapes, command input shapes, event payloads, and policy
  `actor.*` references that cross feature boundaries MUST be marked as
  `public contract <Symbol> as v<N>` in their origin feature.
- The doctor enforces this only under `architecture mode microservices`;
  capsules under `monolith` / `modular_monolith` compile unchanged.
  Cross-feature contract violations surface at compile time, not at
  runtime. See the `cross-feature-contracts` proposal (operational archive) §4.
- The compound `public contract` keyword is in the closed reserved-word
  set; `public` has no other use. Versioning is monotonic per symbol;
  the existing `previously alias` / `previously migrated` clauses carry
  rename history independent of the contract version.

## Namespaces

- `@...` references use the closed namespace catalog: `@role`, `@scope`,
  `@actor`, `@policy`, `@semantic`, `@cap`, `@pii`, `@key`, `@client`, `@fn`, `@hook`,
  `@validator`, `@adapter`, `@query_modifier`, `@anchor`, `@llm`, and `@tool`.
- Registry adapter package refs such as `@runtime/...` and `@plugin/...` are
  adapter source markers, not general `@...` extension namespaces.
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

- **One uniform rule across every callable (SPEC-07 A).** A `policy <ref>`
  reference uses the SAME grammar at command, workflow, query, api, job,
  webhook, escape route, lifecycle transition, view/route guard, and
  `policy_for` default. A reference is well-formed iff it is (a)
  `@policy.<category>` resolving to a feature-local category, (b) a namespaced
  catalog atom `@role.*` / `@scope.*` / `@actor.*`, or (c) a structured policy
  expression. There is no per-construct table — the old "commands/workflows
  must use `@policy.*` while jobs/webhooks/escape-routes/defaults may inline
  atoms" asymmetry is retired.
- **Two kinds of reference, named (SPEC-07 B, per SPEC-04's `@`-doctrine).**
  `@policy.<category>` is the **feature-local named reference** (resolves to a
  `policies` block in the current feature unless feature-qualified). `@role` /
  `@scope` / `@actor` are **app-level catalog atoms** (resolve against the
  registry identity catalog). Both stay on the identity/authorization axis; the
  registry records the kind via distinct scope leaves.
- **No CRUD-named categories (SPEC-07 C).** A `policies` category must NOT
  shadow a command effect verb — neither the plural `creates`/`updates`/
  `deletes`/`reads` nor the bare-singular `create`/`update`/`delete`/`read`
  (which read as a write *effect* at a `policy @policy.<x>` site). Use semantic
  authorization names: `author` / `view` / `edit` / `remove` / `manage`.
  Enforced by `POLICY-CATEGORY-SHADOWS-EFFECT-001` (warning under strict for the
  migration window, error under iron-hand + production). The `@policy.` prefix is
  therefore the SPEC-04 named-reference marker — NOT a disambiguation hack
  papering over the collision.
- Commands declare `policy` explicitly. There is no implicit
  `creates -> @policy.author` rule.
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
- **Framework-emitted 4xx errors are rendered through the resolver chain — no
  raw evaluator-internal string reaches the wire.** The resolver consults
  command-level `policy ... when_denied @translation.<key>`, then per-policy
  `policies.<category>.when_denied`, then per-feature `errors <code> message
  @translation.<key>`, then the built-in catalog under
  `runtime/go/lazuli/i18n/builtin.<locale>.json`. The built-in catalog ships
  PT-BR and en-US for every closed-catalog framework error code
  (`policy_denied`, `validation_failed`, `tenant_mismatch`, `not_found`,
  `rate_limited`, `bad_request`, `method_not_allowed`, `integration_error`),
  so even a freshly-scaffolded app with zero `.lzi` authoring of the error
  surface emits a layperson native string instead of evaluator jargon like
  `"no policy atom matches the active actor for @policy.<name>"`. The
  layperson grep test (`TestBuiltinCatalogNoFrameworkJargon` in
  `runtime/go/lazuli/i18n/builtin_catalog_test.go`) bans framework jargon
  from the built-in catalog values, and the regression-guard HTTP test
  (`TestZeroAuthoringPolicyDeniedEmitsBuiltinPTBR` in
  `runtime/go/lazuli/http_error_resolution_smoke_test.go`) asserts the
  legacy jargon string never reaches the wire payload. See
  `lazuli-ops/docs/proposals/ir-error-messages-vocab.md` §2.D and §2.E.
- Sensitive fields marked with `@pii.*`, `@cap.Encrypted`, `@cap.Hashed`,
  `@cap.E2ee`, or `@cap.Token` declare field-level `read` and `write` policy.
- `registry.env` is the canonical home for environment schema. Every
  `env.NAME` reference resolves there. Optional `group <name>` children
  organize related variables without changing the reference name.
  Client-exposed names use `PUBLIC_`; Expo/mobile names use `EXPO_PUBLIC_`.
  Top-level `env` blocks in feature/app `.lzi` sources are legacy and warn;
  the `tools/generate-fixtures.ps1` migrator strips them.
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
  declares the backing resource plus either a legacy `ttl` or the rotation
  discipline.
- **Refresh token columns**: every `auth.sessions` with a `rotation` block has
  the four rotation columns on its session resource:
  `refresh_token_hash`, `parent_session_id`, `theft_detected_at`, and
  `refresh_expires_at`. The migration schema emits
  `refresh_token_hash TEXT NOT NULL DEFAULT ''`, `refresh_expires_at
  TIMESTAMPTZ`, `parent_session_id BIGINT REFERENCES <session_table>(id) ON
  DELETE SET NULL`, and `theft_detected_at TIMESTAMPTZ` when the authored
  resource omits them. Codified by AUTH-REFRESH-003.
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

- `non_goals` entries are either flat quoted strings (preferred for new
  features) or grouped under `delegated_to` / `out_of_scope`. Both
  surface forms lower to the same flat IR list — see
  `docs/canonical-semantics.md#feature-context-vocabulary`.
- `delegated_to` entries may reference feature ids but do not count as `uses`.
- `out_of_scope` entries are product/design boundaries, not dependencies.
- `purpose "<sentence>"` is a single quoted-string line at feature-child
  indent; at most one per feature. `VOCAB-CONTEXT-PURPOSE-001` fires on
  missing / empty strings.
- Feature context prose is resolved by CONVENTION, not a keyword: a
  co-located `<feature>.ctx.md` markdown sidecar next to the feature's
  `.lzi`. The doctor probes a SINGLE base (the `.lzi` directory) — there
  is no path argument, no project-root fallback, and no override.
  `VOCAB-CONTEXT-CTXMD-001` fires on a missing sidecar or stub content
  (< 100 non-whitespace characters). The former `attach_ctx
  "<relative-path>"` meta statement is retired — the parser hard-errors
  `E-ATTACH-CTX-RETIRED`.
- The `tdd-iron-hand` coverage preset is a meta-bundle: it raises the
  numerical coverage bar (six layers blocked at 90/95) AND escalates
  the three `VOCAB-CONTEXT-*` rules from `warning` to `error` under
  one knob. Manifest `[doctor.test_discipline.severity_override]`
  entries win over the preset escalation.

## Validation

- Whole-resource inline validators use `validates resource "./path.go"`.
- Field-scoped inline validators use
  `validates field <name> "./path.go"`.
- Legacy `validate "./path.go"` and `validates <field> "./path.go"` are
  compatibility forms only.
- Reusable validators used inside commands are called through
  `validate @validator.*` or through `let` plus `requires`.

### Inline field constraints (L0 #3 §10, Gap A)

- Closed catalog of six keywords on resource fields and command input
  slots: `min N`, `max N`, `pattern STRING`, `between A and B`,
  `length N`, `in [...]`. Order is free; modifiers (`required`,
  `optional`, `unique`) and `= <default>` may precede or follow.
- Numeric bounds (`min`, `max`, `between`) are `i64`-typed in the IR.
- `pattern STRING` regex syntax is **Rust `regex` crate / RE2**:
  no lookahead, no lookbehind, no backreferences. Go's stdlib
  `regexp` and JavaScript `RegExp` both accept the RE2 subset, so a
  pattern that passes the Lazuli analyzer compiles identically on
  both emit targets.
- Applicability (§10.1, enforced by `INLINE-VALIDATOR-TYPE-MISMATCH`):
  - `min` / `max` → `Text`, `Integer`, `Decimal`, semantic string variants
  - `length` → `Text` + semantic string variants
  - `pattern` → `Text` + semantic string variants
  - `between` → `Integer`, `Decimal`
  - `in` → `Text`, `Integer`, `Decimal` + semantic string variants
- Combination rules (§10.2, enforced by `FIELD-CONSTRAINT-CONFLICT`):
  `length` rejects `min` / `max`; `between` rejects `min` / `max`;
  `in` rejects `pattern`.
- Range invariants (Wave-B-CL4, enforced by
  `INLINE-VALIDATOR-RANGE-INVARIANT`): `min N max M` requires `N ≤ M`;
  `between A and B` requires `A ≤ B`. Equal bounds are valid
  (single-value domain).
- Pattern well-formedness (Wave-B-CL4, enforced by
  `INLINE-VALIDATOR-PATTERN-COMPILE`): the analyzer rejects the
  unambiguous shape errors (unbalanced `[`, `(`, `)`, trailing
  unescaped `\`) without pulling in the `regex` crate; full RE2
  compilation is the runtime's authoritative check.
- Default-value compatibility (§10.3, enforced by
  `FIELD-DEFAULT-VIOLATES-CONSTRAINT`): a `default` literal must
  satisfy every declared constraint at lowering time.

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
  predicate, transition, and view-extensibility tests use `allows` and `denies`.
  This generated-vs-authored split is now the SOLE test-vocabulary axis: it is
  the one distinction that carries information the typed subject cannot (a 1-bit
  "this row is machine-derived from `policy @policy.*`, do not hand-edit"
  signal). Every other dimension is named by the subject, not a new verb.
- Within authored tests the verb pair never changes — the typed subject after
  `allows`/`denies` carries the dimension: `when <pred>` (rule predicate),
  `from <state>` (workflow edge), `as <actor>` (workflow actor check),
  `extension <feature>` (anchor allowlist, `.lzx` view tests), and a bare eval
  predicate inside `agent` `evals` `case`. Keep these subjects distinct for
  static rejection power; do NOT mint a separate verb for any of them.
- Command tests use `target` when a loaded target exists. Rule and workflow
  tests use `self`.
- Tests use the same closed predicate language as rules and filters; no
  fixtures, mocks, or `given/when/then` framing.

### View tests are extensibility, NOT policy (Wave 4)

View tests in `.lzx` use the SAME authored `allows` / `denies` dialect as
command / rule / transition tests — they are NOT a separate verb family. The
typed subject `extension <feature>` is what makes them extensibility tests:
the only admissible shapes are `allows extension <feature>` and
`denies extension <feature>`. They assert which sibling features may extend
the view via its anchor — they are NOT policy resolution, NOT predicate
boundaries, NOT DOM-level interaction tests. The `extension` subject is what
distinguishes them from `when` (predicate), `from` (edge), and `as` (actor);
the verb is shared, the subject names the dimension (see the test-vocabulary
section above).

View tests are validated by two doctor rules:

- `TEST-VIEW-EXTENSIBILITY-001` (warning) — a view that declares
  `extensible_by` MUST author at least one `allows extension` or
  `denies extension` assertion. Without one, the extension surface is
  undocumented and `TEST-VIEW-DRIFT-001` cannot cross-check it.
- `TEST-VIEW-DRIFT-001` (error) — every `allows extension <feature>` must
  resolve to a sibling experience whose `extends @anchor.<X>` clause
  matches the host view's anchor. Two finding shapes: `MissingFeature`
  (the named experience does not exist) and `MissingAnchorExtension`
  (the experience exists but does not extend the host view's anchor).
  `denies extension` is intentionally existence-tolerant: it pre-commits a
  forbidden surface even before the would-be extender ships.

View tests do NOT participate in DOM-level or interaction-level testing.
That layer belongs to the frontend test suite (Playwright per the
Wave 3.5 surface; React Testing Library at the app level), not to the
Lazuli spec.

### Command/rule/transition `denies when` requires IR-level backing (Wave 4 §7.1)

`TEST-COMMAND-ASSERTION-DRIFT-001` (error) — when a `command tests` block
declares `denies when target.<field> = <value>`, the rule cross-checks the
IR for backing: an `Invariant` on the target resource that mentions the
field, a `lifecycle` whose discriminator is the field, or a `triggers`
binding that already filters state through a lifecycle transition. Without
backing the assertion is documentation-only and the handler's WHERE clause
may silently disagree with the declared spec (the `leave_host_reply`
pattern from proposal §7.1 — the resource had a non-lifecycle `status`
field, the test declared the gate, and the handler ignored it).

The rule reads IR only — it does NOT consume codegen output, by design.
The IR-backing surface (invariant / lifecycle / triggers) is stable
across emitters and survives codegen rewrites; reading generated state
would couple doctor to emitter implementation details. The trade-off
is that handler-level WHERE clauses missing despite consistent IR are
out of scope for v0.1 — those land in a future rule when codegen and
runtime grow a shared `effective_filter` projection.

## Authored Shape

- Features are product capabilities, not entity buckets.
- `uses` is strict: every listed feature should be referenced by a semantic
  edge, not just mentioned conceptually.
- `previously` is a migration continuity marker and must say whether the old
  name is `migrated` history or a temporary compatibility `alias`. Remove it
  once all supported baselines no longer contain the old identity.
- `escape_route` is explicit and still declares its route, policy, tenant
  boundary, and source path.

## Lazurite Distro Boundary

The Lazurite distro layer (`Lazurite.toml` manifest + `lazuli new` template +
folder conventions) is a thin opinionated layer **on top of** Lazuli. It
NEVER adds language mechanisms; it only ships project shape conventions. The
invariants below pin where the distro's reach starts and stops.

- `Lazurite.toml` at the project root holds environment glue the DSL does
  not own: framework version pin (`[lazuli]`), distro template lineage
  (`[lazurite]`), plugin module resolution (`[plugins]`), Go codegen
  settings (`[generate.go]`), frontend topology (`[frontends.*]`),
  migration runner policy (`[migrations]`), seed policy (`[seeds]`),
  and local-dev overrides (`[dev]`). Doctor parses the manifest via
  `crates/lazuli_manifest/src/lazurite_manifest/mod.rs` and `lazuli inspect
  --include=manifest` surfaces it in derived JSON.
- `Lazurite.toml` MUST NOT declare environments, URLs, CORS, deploy gates,
  audiences, locale settings, or any other slot already owned by
  `app.lzi`/`profiles.lzi`/`.lzx`. The DSL is the single source of truth
  for declarations and contracts; the manifest is glue. Doctor enforces:
  - `[env.*]` blocks in `Lazurite.toml` are rejected (use `app.lzi
    environments`/`urls`/`cors`).
  - `[deploy]` block is rejected (use `app.lzi deploy { ... }`).
- `Lazurite.toml [plugins]` keys MUST start with `@plugin/`. The
  `@runtime/<name>` namespace lists OSS commodity infrastructure that
  lives in the Lazuli core runtime — it is wired automatically and never
  appears in `[plugins]`. Doctor emits `PLUGIN-NAMESPACE-MISMATCH-001`
  when a wrong-namespace adapter is declared.
- A project that uses any `@plugin/*` reference in `.lzi` MUST have a
  `Lazurite.toml` declaring that plugin. Doctor emits
  `MANIFEST-REQUIRED-001` otherwise. Projects with no `@plugin/*` refs
  may omit the manifest entirely (advisory mode); fixture suites used
  by codegen tests (`examples/full-capsule/`, `examples/smoke-hello/`,
  etc.) continue to pass doctor without one.
- `Lazurite.toml [frontends.<name>]` declares per-frontend audience-scoped
  SDK projection. Each frontend has `target` (closed enum: `tanstack-vite`
  | `expo` | `next` | `tauri` | `cli`), `out` (output dir), and
  `audiences` (list of audience names declared in `.lzx`). Per-frontend
  SDKs only contain commands/queries the listed audiences are allowed
  to call.
- Audience names referenced in `[frontends.*].audiences` MUST be declared
  in at least one `.lzx audience <name>` block. Doctor emits
  `FRONTEND-AUDIENCE-UNKNOWN-001` otherwise. Conversely, an audience
  declared in `.lzx` but listed in no `[frontends.*]` produces
  `AUDIENCE-NO-FRONTEND-001` (warning: orphan audience = dead view code).
- Generated code lives in `dist/` (target-specific subdirs: `dist/go/`,
  `dist/ts-<frontend>/`, etc.). `.lazuli/` is reserved for internal cache
  + manifests (graph.json, source-map.json, manifest.json); it is NEVER
  user-editable and contains no user-facing artifacts.
- `dist/go/go.mod` is a Go sub-module by default
  (`[generate.go].submodule = true`); the scaffold writes a top-level
  `go.work` listing both root and `./dist/go` modules. Doctor cross-checks
  Lazuli runtime version parity between root `go.mod` and `dist/go/go.mod`
  (`SUBMODULE-DRIFT-001`).
- A future Lazuli distro (Lazonyx, Lazpipe, etc.) MUST NOT extend the
  language: no new `@-namespace`, no new `kind` keyword, no shadowing of
  a Lazuli primitive with a distro-specific resolution path. Distros
  ship folder conventions + default plugins + scaffold templates only.
  If a primitive is genuinely needed, it enters Lazuli first (grammar +
  doctor + codegen) and the distro adopts it.
