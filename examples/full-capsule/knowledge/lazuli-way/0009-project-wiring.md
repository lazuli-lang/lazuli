---
title:   "Project wiring"
slug:    project-wiring
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, app, registry, profiles, workspace, namespace]
read_when: "writing app.lzi / registry.lzi / profiles / integrations / @runtime vs @plugin"
---

# Project wiring

Features describe *what* the app does; the project layer describes *how it runs*: which features participate, which targets generate, which environments exist, which adapters back capabilities. Four files own it — only the first two are mandatory.

- **`app.lzi`** — provider-neutral operational contract: `uses`, `targets`, `environments`, `urls`, `bindings`, `runtime` units, `deploy` gates, plus `cors`, `env`, `locale`, `logging`, `tracing`.
- **`registry.lzi`** — package catalog: env groups, `capabilities`, `integrations`, `packs`. The *only* place adapters are named — where provider provenance enters.
- **`profiles.lzi`** *(optional)* — per-env overlays; keeps env-specific intent out of `app.lzi`.
- **`workspace.lzi`** *(optional, distributed-only)* — multi-app / external-service / gateway contract. A single-app project never needs it; nothing may make it mandatory.

Cardinal rule: **the language stays provider-neutral; only registry adapters know a vendor's name.** Get it backwards and you've leaked Stripe into the grammar.

## `app.lzi` — the operational contract

One block per concern. The two load-bearing dependency statements: `uses` (features the app activates) and `bindings` (which integration slot resolves to which registry integration). The rest configures the runtime.

```lazuli
app Billing
  title "Billing"
  version "0.1.0"
  default_timezone "America/Sao_Paulo"

  uses
    invoice
    invoice_import

  bindings
    invoice_import.crm = integrations.crm

  targets
    backend go
    web react
    mobile expo

  environments
    local
    staging
    production

  env
    server STRIPE_API_KEY: Secret required
    client PUBLIC_APP_URL: Url required

  urls
    web local "http://localhost:3000"
    api local "http://localhost:8080"
    web production "https://billing.example"
    api production "https://api.billing.example"

  cors
    allow_origins production "https://billing.example"
    allow_credentials true
    max_age "1h"

  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"
      readiness "/readyz"
    unit web
      serves surfaces web
    unit worker
      runs jobs *

  deploy
    migrations before_deploy
    migration_lock required
    strategy rolling
    destructive_migrations require_approval
    rollback on_failed_healthcheck
```

Gaffes the parser/doctor catch:

- **`env` type names are a closed catalog: `Secret | Text | Url | Boolean | Integer`** — exact casing (`Url`, **not** `URL`). Each entry: `server|client|mobile NAME: <Type> required|optional [in <environment>]`. The scope prefix decides whether the value reaches the Go server, web bundle, or mobile bundle.
- **`deploy` values are closed-catalog, not booleans**: `migration_lock required` (not `true`); `migrations before_deploy|manual|disabled`; `destructive_migrations require_approval|forbidden`; `rollback on_failed_healthcheck|manual|disabled`; `strategy rolling|blue_green|canary`.
- **`runtime` units = process topology.** `serves` lists what a unit answers (`queries`, `commands`, `webhooks`, `apis`, `surfaces web|mobile`); `runs` lists background work (`jobs *`, `schedules *`). `serves surfaces web` requires `web` in `targets`.

## `uses` is strict — declare only what you reference

`uses` is a *semantic* dependency edge, not a convenience import. List a feature only when this app/feature references its domain, events, or operations. Unused entry → dead wiring (doctor flags); referenced-but-unlisted → unresolved reference. Same at feature level: a feature's `uses org, user` names exactly the siblings whose types it touches.

## Dependency model: slots, not instances

Lazuli never lets a feature `new()` a db client or `inject()` a CRM SDK (see [the-three-operators](0003-the-three-operators.md) — no construction syntax). A feature *declares a need*; the project *satisfies it by binding a slot*:

```lazuli
feature invoice_import
  purpose "Ingest invoices from an external CRM via a bound integration slot."

  uses invoice

  requires integration crm: CRMProvider

  domain
    resource ImportBatch
      total_rows: Integer = 0
```

`requires integration crm: CRMProvider` = "I need *some* provider satisfying `CRMProvider`, under local slot `crm`." The feature never knows the vendor. The app wires it in `bindings`:

```text
  bindings
    invoice_import.crm = integrations.crm
```

Left = `<feature>.<slot>`; right = `integrations.<name>` from the registry. Doctor checks both directions: every `bindings` key must match a real `requires integration` slot, and the bound integration must exist in `registry.lzi`. Same capability-binding pattern fields use for typed resources — see [resources-and-fields](0011-resources-and-fields.md).

## `registry.lzi` — where adapters get a name

The *only* place a provider name legitimately appears — and then as an **adapter reference**, never a keyword. Catalogs env groups, capabilities, integrations, packs:

```lazuli
registry
  env
    group crm_import
      server CRM_WEBHOOK_SECRET: Secret required in production
    group public_clients
      client PUBLIC_APP_URL: Url required

  capabilities
    database postgres
    queue background_jobs
    object_storage files
    mailer transactional
    cache shared
    integration crm

  packs
    invoice_import from @runtime/crm-import
      version "0.1.0"
      provides feature invoice_import
      requires integration crm: CRMProvider

  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments sandbox, production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET
```

Shapes the doctor enforces:

- **`capabilities` kinds are a closed catalog**: `database`, `queue`, `object_storage`, `mailer`, `event_bus`, `tracing`, `cache`, `search`, plus `integration <slot>`. A new kind warns — kinds move with a language cut, not a registry edit.
- **`packs` use `<name> from @scope/package`**, then `version`, `provides`, `requires` children. Not `pack <name>` with a nested `name` line.
- **`credentials` takes a tier label** (`platform`, `tenant`, or `actor`); children bind an env var with a *space*, not `=`: `webhook_secret env.CRM_WEBHOOK_SECRET`.

## Namespace policy — the gaffe magnet

Most cold-write mistakes land here. Adapter provenance is a closed set; the prefix encodes *who owns the thing*:

- **`@runtime/<name>`** — OSS commodity infra with an open spec or de-facto-standard layer: Postgres, Redis, S3-protocol signing, SMTP, Kafka, NATS. Live in the Lazuli runtime itself.
- **`@plugin/<name>`** — a *named vendor SaaS or specific product*, even if open source: Stripe, MercadoPago, Sendgrid, Twilio, Algolia, Meilisearch. Live in separate plugin repos.
- **`@adapter.<local>`** — a local adapter you author for a contract runtime/plugins don't cover (homegrown/legacy CRM).

```lazuli
registry
  capabilities
    database postgres
      adapter @runtime/postgres
    object_storage files
      adapter @runtime/s3
    search index
      adapter @plugin/meilisearch/search
```

Three rules → reflex:

1. **A provider name is NEVER a core keyword.** No `stripe`, `postgres`, `aws`, `kafka` keyword. The provider appears only as the trailing segment of an `@runtime/` / `@plugin/` ref. Wanting a vendor keyword = a scope violation; keep the wire thin (see [wire-not-reimplement](0001-wire-not-reimplement.md)).
2. **Adapter named after the *provider*, not the consuming product.** MercadoPago is `@plugin/mercadopago`, never `@plugin/<your-app>/mercadopago`. The adapter is generic and reusable.
3. **"Commodity infra or named product?"** — ask before every new adapter. Open spec / de-facto-OSS → `@runtime`. Specific named SaaS/tool → `@plugin`. Per-vendor business glue fitting neither → [an escape hatch](0002-five-escape-hatches.md) in your own Go, not the framework.

## `profiles.lzi` — per-environment overlays

Each `profile <env>` block overlays `urls`, `bindings`, `integrations`, `deploy` for that env, keeping the base manifest one clean contract:

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
    migrations before_deploy

profile production
  urls
    web "https://billing.example"
    api "https://api.billing.example"
  integrations
    crm environment production
  deploy
    topology split_services
    migrations before_deploy
    rollback on_failed_healthcheck
```

`local` binding `@adapter.fake_crm` is the canonical way to swap a real vendor for a fake in dev without touching feature code — same slot, different adapter.

## `workspace.lzi` — optional, distributed only

A single app must not have one. Reach for it only with multiple Lazuli apps, external (non-Lazuli) services, or a shared gateway. Declares apps, cross-service event `boundaries`, workspace-wide `communication` defaults, a provider-neutral `gateway`:

```lazuli
workspace Acme
  apps
    billing at "./billing/app.lzi"
    ai external contract "acme.ai.v1"

  shared_registry "./registry.lzi"

  boundaries
    billing publishes invoice.*
    ai consumes invoice.*

  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus

  gateway public_api
    route "/api/invoices/*" to app billing
      auth propagate
      tenant propagate
      timeout "5s"
```

`boundaries` makes the cross-service event graph statically visible: which app produces each event class, which consumes it (the `invoice.*` patterns are the same event names features emit — see [events-and-event-groups](0012-events-and-event-groups.md)). The `gateway` owns only the *shape* — which app mounts which route — never proxy/mesh mechanics (runtime concerns).

When wiring confuses you, ask the compiler: `lazuli check <file>` validates a manifest standalone; `lazuli inspect` shows the resolved graph (see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md)).

Authoritative spec: `docs/grammar.app.md`, `docs/grammar.registry.md`, `docs/grammar.workspace.md`, and the namespace policy in `CLAUDE.md`.
