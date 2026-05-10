# Capability Layering

This note records the decision boundary between Lazuli as a contract language
and Drusa as the batteries-included framework that materializes Lazuli IR.

Use the names consistently:

- Lazuli language: `.lzi`/`.lzx` syntax.
- Lazuli compiler: parser, resolver, checker, doctor, expand, and IR.
- Drusa framework: runtime, codegen, packs, generated app wiring, and default
  implementations.
- Drusa adapters: concrete providers and infrastructure integrations.

The short rule is:

- The language owns contracts that must be statically checked, lowered into IR,
  reflected in generated APIs, or enforced across backend/frontend boundaries.
- Drusa owns reusable product capabilities: generated resources, UI
  flows, adapters, runtime wiring, queues, providers, and defaults.
- Adapters own concrete infrastructure choices such as Redis, S3, Stripe,
  OpenTelemetry exporters, search engines, OAuth providers, and KMS.

Lazuli is not an integration runtime. It says that a command/job/webhook needs
an external contract with specific policy, PII, tenant, idempotency, timeout,
and event guarantees. Drusa turns that contract into Go backend wiring. The Go
backend performs real HTTP/RPC/broker/webhook work through adapters. React and
Expo clients stay behind generated app APIs and should not call provider
integrations directly.

Do not promote every useful product feature into language syntax. Lazuli should
stay small enough to read and strict enough to prove dangerous behavior. The
Drusa framework can be broad because its packs are authored on top of the
language.

ERP is a pressure test, not a namespace. If an ERP-shaped requirement is really
a horizontal operational contract, give it a generic name and prove it outside
ERP too. If it is a vertical module such as chart of accounts, fiscal tax rules,
or procurement, keep it as a Drusa pack or adapter.

## Pipeline

Drusa should consume Lazuli IR, not re-parse source and infer semantics on its
own:

```text
.lzi/.lzx
  -> Lazuli parser/resolver
  -> Lazuli check/doctor/expand
  -> Capsule IR
  -> Drusa packs/codegen/runtime
  -> Go backend + React web + Expo mobile + jobs + webhooks
  -> Go adapters/transports: HTTP/RPC/brokers/storage/auth/OTel/search/etc.
```

This keeps one semantic source of truth. Drusa packs may add templates,
runtime behavior, and optional doctor rules, but they should not hide product
invariants outside the IR/checking path.

## Decision Test

When a candidate capability appears, classify it by asking:

1. Does this change static analysis, policy reachability, tenancy, generated
   API shape, migration identity, or security proof? If yes, it is language or
   IR.
2. Is it a reusable product module with screens, jobs, resources, templates,
   and adapters? If yes, it is a Drusa capability pack.
3. Is it a provider choice or runtime implementation detail? If yes, it is an
   adapter.
4. Can the capability be written as ordinary `.lzi`/`.lzx` plus extensions? If
   yes, prefer a pack over new syntax.

Use these layer names in tables and engine manifests:

- `language`: core syntax/IR/checker semantics.
- `language-light`: a small contract visible to Lazuli, with most behavior in a
  pack or runtime.
- `pack`: reusable Drusa product module authored in Lazuli plus code.
- `runtime`: framework execution machinery, usually not product-owned source.
- `adapter`: concrete provider/infrastructure binding.

Composition is allowed, for example `language + pack`, `language-light + pack`,
or `pack + adapter`.

## Core Language Surface

The language should keep these cross-cutting contracts first-class:

- actors, auth identity context, route/input/params binding
- app manifest, typed app routes, route builders, environment schema, external
  integration registry, runtime units, and provider-neutral deploy gates
- policy, RBAC atoms, scopes, field policy, audience reachability
- tenancy, tenant binding, tenant fanout, tenant-scoped idempotency
- soft delete and generated query scope
- audit and PII classification
- idempotency
- jobs, scheduled jobs, webhooks, events, event bus contracts
- rate limits
- storage capabilities such as `@cap.File(max_size:<size>,accept:<mime>)`
- declarative search contracts
- pagination
- money/currency types
- retention and temporal write-window contracts
- extension contracts and view composition anchors

These are not all equally mature today, but they are language-shaped because
generators and checkers must understand them.

## Language Implementation Queue

The first language-level pass should harden existing syntax before inventing
new blocks. Drusa packs stay deferred unless a row says "pack later".

| Capability | Language artifact | Status |
|------------|-------------------|--------|
| App/runtime manifest | `app.lzi` with targets, locale/timezone, fallback routes, used features, environments, URLs, runtime units, and deploy gates | implemented as app operational contract in IR/inspect/doctor |
| Service boundaries | `app.lzi` `architecture`, `services`, and `communication` with logical ownership, exposures, published/consumed events, and context propagation | implemented as microservice-ready contract; Drusa may materialize as monolith, modular monolith, or split services |
| Type-safe app routes | top-level `.lzx route <name>` with canonical `path`, `params`, `to`, `surface`, and `audience`; legacy `stack` remains compatibility syntax | implemented as route-builder contract |
| Env/secrets schema | `registry.lzi` or `app.lzi` `env`, plus top-level `.lzi env`, with optional `group <name>` and `server|client|mobile NAME: Type required|optional [in environment]` | implemented as source contract; groups are organizational, not namespaces |
| External integration registry | `registry.lzi` `integrations` with provider-neutral names, capability kind, adapter reference, environments, and credential scope | implemented as package registry contract; provider operations remain pack/adapter |
| Deploy/runtime contract | `app.lzi` `runtime`, `capabilities`, and `deploy` blocks | implemented as provider-neutral operational contract; doctor cross-checks package usage; Drusa/adapters materialize it |
| Custom HTTP APIs | `api <name>` with method, path, route/input, output, policy, handler | implemented as language-light endpoint contract |
| Error exposure | feature `errors` plus command/rule `error <Name> status ... expose ...` | implemented as public/private error contract |
| Cache/invalidation | query `cache` and command `invalidates` | implemented as client/server cache contract |
| RBAC/policy | `@role.*`, `@scope.*`, `@policy.*`, field policies, audience reachability | implemented, keep hardening doctor checks |
| Auth context | `auth`, actors, `ctx.user`, `ctx.customer`, route-from-context | implemented as contract, framework flows later |
| Multi-tenancy | `tenancy`, `tenant_from`, tenant fanout, tenant-aware idempotency | implemented, keep moving text checks into IR |
| Soft delete | `soft_delete`, generated query scope | implemented, cascading soft delete still explicit/open |
| Audit/GDPR | `@pii.*`, event payload markers, retention contracts, security inspect output | implemented as language markers/contracts; export/erasure workflows as packs later |
| Idempotency | `idempotency by ...` for jobs/webhooks and future public commands | implemented |
| Jobs/cron | `job`, `trigger event`, `trigger schedule`, `retry`, `fanout` | implemented as source contract |
| Webhook receivers | `webhook`, `verify`, `tenant_from`, `idempotency`, `handler` | implemented as source contract |
| Rate limits | `rate_limit` on public/mutating commands and auth flows | implemented |
| Storage | `@cap.File(max_size:<size>,accept:<mime>)` | implemented as language contract; upload UI/adapters later |
| Search | `search params.q over ... mode contains` | implemented as query contract |
| Pagination | `paginate <positive-int>` under `query.list` | implemented as language contract |
| Money | `@semantic.Money` plus explicit currency field when needed | implemented as semantic type; precision/rounding/currency scalar later only if adapters require it |
| Temporal write windows | `write_window by <date-expression> within <window-reference>` | implemented as language-light command contract; period-closing packs later |
| Geolocation | future `@semantic.GeoPoint`/shape decision | deferred language-light primitive |
| Event bus | `event`, `event_group`, `event.trace`, `trigger event` | declarations and emissions are language; routing runtime and broker adapters later |
| Tracing | `event.trace` | implemented as language signal; OTel runtime later |
| Feature flags | possible future `when flag.*` | deferred until repeated source pressure |

Microservice readiness follows the same boundary: Lazuli owns service
ownership, exposed contracts, event edges, and context propagation because those
facts are checkable and affect generated APIs. Drusa owns whether the graph runs
as a monolith, modular monolith, or split services. Concrete transports and
infra such as gRPC, Connect, Kafka, NATS, SQS, Kubernetes, Envoy, and service
mesh settings are adapters.

For non-Lazuli services, such as a Python AI service, Lazuli should import or
author the contract. Drusa should generate/wire typed Go transport bindings and
contract tests. The external service implements HTTP/RPC/broker semantics in
its own stack. Optional SDK exports for other languages are publication
artifacts, not the core Drusa runtime path.

## Drusa Capability Packs

Drusa packs should be ordinary Lazuli features plus optional generators and
runtime adapters. A pack may ship:

- `.lzi` resources, policies, commands, jobs, webhooks, and events
- `.lzx` experiences and platform projections
- reusable UI components under `@client.*`
- server functions, validators, hooks, query modifiers, and adapters
- migrations, seed data, test fixtures, and docs
- optional provider bindings

Examples: `auth_pack`, `teams_pack`, `notifications_pack`, `billing_pack`,
`comments_pack`, `kyc_pack`, and `storage_pack`.

A pack must not rely on hidden semantics. If a pack needs an invariant that
cannot be expressed in `.lzi`/`.lzx`, it must either provide a doctor rule or
propose a language primitive. For example, a billing pack should not secretly
assume that posting an invoice creates balanced ledger entries; it should model
that as source, generated checks, or a pack doctor rule such as
`DRUSA-BILLING-001`.

## Promotion Lifecycle

Capabilities should graduate only when repeated usage proves the need:

1. Custom code or extension.
2. Reusable Drusa pack.
3. Pack with doctor rules.
4. `language-light` contract.
5. Core `language` primitive.

Examples:

- `comments`: likely custom feature -> pack, and probably never core syntax.
- MFA: auth pack -> doctor rules -> language-light challenge/validator contract.
- `tenant_from`: runtime need -> core language because it affects security.
- ERP ledger: pack -> doctor rules -> possible language-light/core primitive if
  many modules need the same accounting correctness contract.

## Heavy ERP Implication

ERP modules should mostly ship as packs. ERP invariants that affect security,
auditability, accounting correctness, or cross-module consistency may graduate
into `language-light` contracts or core primitives.

Initial placement:

- Chart of accounts: pack.
- Ledger entry balancing: pack with doctor rules, language-light candidate.
- Period closing: pack plus doctor rules.
- Closed periods/write windows: generic `write_window`, not fiscal-only syntax.
- Approval workflows: language workflow plus pack screens/defaults.
- Segregation of duties: language policy/scope/rule contract.
- Fiscal provider integration: adapter.
- Tax calculation: pack + adapter, often jurisdiction-specific.

## Capability Classification

| Key | Label | Engine flag | Primary layer | Notes |
|-----|-------|-------------|---------------|-------|
| `auth` | Authentication | `auth` | language + pack | Language owns actors, identity context, auth contracts; Drusa owns flows, session runtime, screens. |
| `rbac` | RBAC | `rbac` | language | `@role.*`, `@scope.*`, `@policy.*`, field policies, and reachability are checker/codegen contracts. |
| `2fa` | Two-factor auth | `twoFactor` | language-light + pack | Language owns challenge/validator contracts; Drusa ships TOTP/SMS/passkey flows. |
| `oauth` | OAuth | `oauth` | language-light + pack + adapter | Language declares identity/session contracts; Drusa owns callback flow; adapters own providers. |
| `magic-link` | Magic link | `magicLink` | pack | Auth flow pack using token, mail, rate limit, and session contracts. |
| `kyc` | KYC | `kyc` | pack | Domain-specific workflow built from storage, policy, jobs, webhooks, and audit. |
| `multi-tenancy` | Multi-tenancy | `multiTenancy` | language | Core: `tenancy`, `tenant_from`, query scope, indexes, job fanout, webhook binding. |
| `invites` | Team invites | `invites` | pack | Pack over users, teams, tokens, email, expiry, and roles. |
| `presence` | Presence | `presence` | runtime + adapter | Needs realtime transport and ephemeral state; language may expose events only. |
| `soft-delete` | Soft delete | `softDelete` | language | Affects delete semantics, restore, query scope, audit, and generated endpoints. |
| `audit-log` | Audit log | `auditLog` | language + pack | Language marks audit requirements; Drusa stores, views, and exports logs. |
| `idempotency` | Idempotency | `idempotency` | language | Required for jobs, webhooks, and eventually public commands. |
| `cqrs` | CQRS | `cqrs` | pack pattern | Prefer queries, projections, SQL, and events before adding core syntax. |
| `event-sourcing` | Event sourcing | `eventSourcing` | runtime pattern | Advanced persistence mode; keep out of core until real adapters demand it. |
| `notifications` | Notifications | `notifications` | pack + adapter | Pack for templates, channels, preferences, delivery jobs, and provider adapters. |
| `realtime` | Realtime | `realtime` | runtime + adapter | Events may be language; sockets/subscriptions/transports are runtime. |
| `chat` | Chat | `chat` | pack | Product module on realtime, storage, notifications, comments, and policies. |
| `comments` | Comments | `comments` | pack | Reusable resource/module, not core syntax. |
| `reactions-ux` | Reactions | `includeReactions` | pack | Reusable UI/domain behavior, not core syntax. |
| `background-jobs` | Background jobs | `backgroundJobs` | language | `job trigger event`, queue, handler, policy, idempotency. |
| `cron-jobs` | Cron jobs | `cronJobs` | language | `trigger schedule`, retry, fanout, system policy. |
| `cache` | Cache layer | `cache` | runtime + adapter | Hints may become language later; Redis/provider behavior is adapter. |
| `rate-limit` | Rate limiting | `rateLimit` | language | Security surface and generated endpoint behavior need static visibility. |
| `tracing` | Tracing (OTel) | `tracing` | language-light + runtime + adapter | `event.trace` is language-shaped; OTel exporters and spans are adapter/runtime. |
| `feature-flags` | Feature flags | `featureFlagsRuntime` | language-light + runtime | Runtime owns evaluation; language may later add `when flag.*` if needed. |
| `storage` | File storage | `storage` | language + pack + adapter | Language declares `@cap.File(max_size:<size>,accept:<mime>)` and policies; Drusa owns upload flows; adapters own providers. |
| `search` | Full-text search | `search` | language + pack + adapter | Language declares `search params.q over ...`; Drusa/adapters implement engines. |
| `i18n` | Internationalization | `i18n` | pack | Labels/messages/runtime formatting first; syntax only if repeated source contracts demand it. |
| `money` | Money + currency | `money` | language | Amount/currency/precision/rounding affect validation, storage, APIs, and UI; Stripe/subscriptions stay packs/adapters. |
| `geolocation` | Geolocation | `geolocation` | language-light + adapter | Coordinates/geotypes are language-light; maps/geocoding are adapters. |
| `pagination` | Pagination | `pagination` | language | Query/API/UI contract: page size, cursor, limits. |
| `webhook-receivers` | Webhook receivers | `webhookReceivers` | language | `webhook`, verification, `tenant_from`, idempotency, handler contract. |
| `event-bus` | Event bus | `eventBus` | language + runtime + adapter | Event declaration/emission is language; routing/broker is runtime; Kafka/NATS/SQS are adapters. |
| `gdpr` | GDPR / LGPD | `gdpr` | language + pack | `@pii`, retention/export/delete contracts in language; workflows/tools in packs. |
| `subscriptions` | Subscriptions | `subscriptions` | pack + adapter | Billing/subscription pack using money, webhooks, jobs, policy, and adapters such as Stripe. |

## Product Direction

The language should be conservative. Drusa packs can move faster.

When a pack repeatedly needs custom analyzer logic or hidden runtime behavior,
that is evidence for a new language primitive. Until then, the pack remains
authored Lazuli source plus adapters. This keeps Lazuli useful for heavy ERPs
and traditional web/mobile/backend apps without turning the DSL into a giant
catalog of product features.
