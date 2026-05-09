# Lazuli Language Backlog

This document tracks design pressure that is not yet part of the core canonical syntax.

## Closed v0 Decisions

- Canonical `.lzi` is the official authoring format; no compact/expanded dual syntax.
- A feature is a product capability, not an entity bucket. Split auth/import/tags/etc. into separate features even when they all touch the same resource.
- Local references omit the feature prefix; cross-feature references must be feature-qualified and backed by `uses`.
- `uses` is strict; do not list conceptual dependencies that are not referenced semantically.
- `params` and `input` stay separate in commands.
- Commands are not route-owned; commands declare `route` slots for routing/context values and `input` slots for submitted body values. Surfaces pass only submitted input explicitly; route slots bind by name from route context.
- Commands declare exactly one explicit effect: `creates`, `updates`, or `deletes`.
- Any `route.*` reference must be declared by a `route` slot.
- Command policy is explicit; effect-derived policy is a generator suggestion, not an invisible semantic default.
- `derive` is not canonical; `creates` and `updates` use assignment blocks.
- Commands may `returns` typed data when the immediate caller needs response data, not as a substitute for events.
- `policies` is a named dictionary; entries use `name: predicate, predicate`.
- Policy atoms are namespaced by category, e.g. `@role.admin`, `@scope.same_org`, `@actor.system`, and `@scope.public`.
- Capability references use a closed namespace catalog: `@role.*`, `@scope.*`, `@actor.*`, `@semantic.*`, `@cap.*`, `@client.*`, `@fn.*`, `@hook.*`, `@validator.*`, `@adapter.*`, and `@query_modifier.*`; view anchors use `@anchor.*`.
- Extension declarations use the same keyword as their call-site namespace, such as `client`, `fn`, `hook`, `validator`, `adapter`, and `query_modifier`.
- Built-in types with behavior use a closed namespace: `@semantic.*` for validation/formatting and `@cap.*` for platform capabilities.
- Query declaration modes are explicit: `query.list`, `query.lookup`, and `query.sql`.
- Single-key lookup queries use shorthand such as `query.lookup by_id by id: ID`; composite lookups keep `params` and `key`.
- Field-level policy overrides live under `policies fields`; `non_goals` uses the same `name: value` punctuation.
- Feature-level `defaults` can provide repeated `tenancy`, `timestamps`, and async/system `policy` defaults.
- `scope` is reserved for safety boundaries; ordinary caller predicates belong in `filters`.
- Read views use `source query.*`; write forms use `submit command.*`.
- Cross-feature view composition uses `extends @anchor.<view_id>` and requires the target view to whitelist extension features with `extensible_by`.
- `tenancy` and `soft_delete` inject default declarative query scope.
- SQL queries must declare their safety scope explicitly.
- Idempotency declarations use `idempotency by <source>` so the key source is statically visible.
- Webhooks run named server-side handlers after verification; no magical declarative webhook upsert in v0.
- Job/webhook handlers are declared inline with `handler`; event consumers are named `job`s with `trigger event`.
- Single-use view blocks and resource validators may declare their implementation inline instead of creating an `extensions` index entry.
- Workflow-level `policy` is a default; transition-level `requires` declares stronger authority.
- Workflow-level `emits` always fires; transition-level `emits` fires additionally.
- Workflow transitions may use inline trailing clauses for scalar `requires` and `emits`; this is local syntax sugar over the same child statements, not a macro.
- Workflow transition child statements are contiguous with the transition header; no blank line separates the header from `requires`, `emits`, or `tests`.
- `assignment`, `reacts to`, and `crud` are not canonical v0 sugar. They expand across constructs or imply project-specific behavior, so they remain explicit until real usage pressure proves otherwise.
- Event payloads are explicit; shared repeated envelope fields use mandatory inheritance from `events <event-pattern> on <Resource>` for matching events in the same feature.
- Observability-only events use `event.trace <name>` rather than an `observability_only` modifier.
- `self` is an immutable target snapshot; declarative jobs and commands use `let` for derived values shared by writes and emitted payloads.
- `context` is only an override for the co-located `<feature>.ctx.md` convention.
- `on_delete` governs hard delete only; soft-delete cascades must be explicit behavior.

## Missing Constructs Under Pressure Test

### Auth

`examples/user-auth.lzi` exercises password login, OAuth, MFA, sessions, refresh tokens, rate limiting, and account recovery. Likely outcome: adapters plus `fn`/`validator` extensions, not a fully declarative auth language.

### Inbound Webhooks

`examples/billing.lzi` exercises external calls without `ctx.user`: verified inbound payloads, retries, idempotency keys, and event emission.

Canonical shape under implementation pressure:

```lazuli
webhook stripe_payment
  path "/webhooks/stripe"
  verify "./integrations/stripe.go"
  handler "./integrations/record_payment.go" returns Payment
  emits payment_received
```

### Async Jobs

`examples/notification.lzi`, `examples/billing.lzi`, and `examples/import-csv.lzi` exercise cron/schedule/queue semantics and progress/error reporting.

Canonical shape under implementation pressure:

```lazuli
job recompute_scores
  trigger schedule "0 2 * * *"
  handler "./jobs/recompute_scores.go"

job send_archive_email
  trigger event customer.customer_archived
  idempotency by envelope.id
  retry 3 backoff exponential
  handler "./outreach/send_archive_email.go"
```

### Rich Relations

`examples/comment.lzi` and `examples/org-team.lzi` exercise explicit resources for relations that need payload or lifecycle.

### Cascading Soft Delete

Still open. Need a decision for whether related soft-deleted parents automatically hide children, or whether every query must model that relationship explicitly.

### Multi-Surface

`examples/org-team.lzi` and `examples/user-auth.lzi` start this pressure test with `surface web admin`, `surface web public`, and `surface mobile member`.

## Foundation Work

### Ergonomics Checklist

- [x] Namespace policy atoms in docs and fixtures.
- [x] Namespace extension references by capability in docs and fixtures.
- [x] Mark behavioral built-ins with `@semantic.*` and `@cap.*`.
- [x] Split query declarations into `query.list`, `query.lookup`, and `query.sql`.
- [x] Use explicit extension declaration namespaces (`fn`, `hook`, `validator`, `adapter`, `query_modifier`) instead of classifying `server` declarations by contract.
- [x] Add single-key `query.lookup <name> by <field>: <Type>` shorthand.
- [x] Use `idempotency by ...` for event and webhook dedupe keys.
- [x] Split event-triggered job locators into `envelope.*` and `payload.*`.
- [x] Allow omitted `target query.by_id(id: route.id)` for the local `route id` mutation case.
- [x] Share repeated event envelope fields through `events <pattern> on <Resource>`.
- [x] Use `event.trace` for observability-only events.
- [x] Namespace view anchors with `@anchor.*` and whitelist extensions with `extensible_by`.
- [x] Use transition `requires` for stronger workflow authority.
- [x] Add inline tests for commands, workflow transitions, rules, and extensible view anchors.
- [x] Add inline trailing clauses for workflow transition `requires` and `emits`.
- [x] Add initial `lazuli inspect --expand` contract with expansion classes, JSON output, and provenance.
- [x] Add an idempotent fixture generation/check script.
- [ ] Lower the new canonical surface into typed IR instead of LSP-only text diagnostics.
- [ ] Add parser support for canonical indentation syntax beyond the legacy brace MVP.
- [ ] Lower `lazuli inspect --expand` from text projection to typed IR once the canonical parser covers the new indentation syntax.

### Broader Validation

- Expand `canonical-semantics.md` into a proper language spec with syntax tables and generated IR effects.
- Validate `error-contract.md` against Go/React adapters.
- Validate `migrations.md` against semantic diffs from fixture changes.
- Validate `project-structure.md` against extension/SQL/escape conventions.
- Validate `testing-strategy.md` against generated invariant tests.
