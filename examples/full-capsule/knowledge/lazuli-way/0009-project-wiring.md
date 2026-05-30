---
title:   "Project wiring"
slug:    project-wiring
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, app, registry, profiles, workspace, namespace]
---

# Project wiring

Features describe *what* the app does. The project layer describes *how the app
runs*: which features participate, which targets get generated, which
environments exist, and which adapters back the capabilities features depend on.
Four files own this layer, and only the first two are mandatory.

- **`app.lzi`** — the provider-neutral operational contract: `uses`, `targets`,
  `environments`, `urls`, `bindings`, `runtime` units, `deploy` gates, plus
  cross-cutting blocks (`cors`, `env`, `locale`, `logging`, `tracing`).
- **`registry.lzi`** — the package catalog: env groups, `capabilities`,
  `integrations`, `packs`. This is *where adapters are named* — the single point
  where provider provenance enters the project.
- **`profiles.lzi`** *(optional)* — per-environment overlays that keep
  environment-specific intent out of `app.lzi`.
- **`workspace.lzi`** *(optional, distributed-only)* — the multi-app / external-
  service / gateway contract. A single-app project never needs it, and nothing
  may make it mandatory.

The cardinal rule of this layer: **the language stays provider-neutral; only the
registry adapters know a vendor's name.** Get that backwards and you've leaked
Stripe into the grammar.

## `app.lzi` — the operational contract

`app.lzi` is one block per concern. The two load-bearing dependency statements
are `uses` (which features the app activates) and `bindings` (which integration
slot resolves to which registry integration). Everything else configures the
runtime.

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

A few gaffes the parser and doctor catch immediately:

- **Env type names are a closed catalog: `Secret | Text | Url | Boolean |
  Integer`.** Write `Url`, **not** `URL` — the casing is exact. Each entry is
  `server|client|mobile NAME: <Type> required|optional [in <environment>]`. The
  scope prefix (`server`/`client`/`mobile`) decides whether the value reaches the
  Go server, the web client bundle, or the mobile bundle.
- **`deploy` values are closed-catalog, not booleans.** `migration_lock
  required` (not `true`), `migrations before_deploy|manual|disabled`,
  `destructive_migrations require_approval|forbidden`, `rollback
  on_failed_healthcheck|manual|disabled`, `strategy rolling|blue_green|canary`.
- **`runtime` units name the process topology**: `serves` lists what a unit
  answers (`queries`, `commands`, `webhooks`, `apis`, `surfaces web|mobile`);
  `runs` lists background work (`jobs *`, `schedules *`). `serves surfaces web`
  requires `web` to be in `targets`.

## `uses` is strict — declare only what you reference

`uses` is not a convenience import list; it is a *semantic* dependency edge. List
a feature in `uses` only when this app (or feature) actually references its
domain, events, or operations. An unused `uses` entry is dead wiring the doctor
will flag, and a referenced-but-unlisted feature is an unresolved reference. The
same discipline applies at the feature level: a feature's own `uses org, user`
line names exactly the sibling features whose types it touches.

## The dependency model: slots, not instances

Lazuli never lets a feature `new()` a database client or `inject()` a CRM SDK
(see [the-three-operators](0003-the-three-operators.md) for why the language has
no construction syntax). Instead a feature *declares a need* and the project
*satisfies it by binding a slot*:

```lazuli
feature invoice_import
  purpose "Ingest invoices from an external CRM via a bound integration slot."

  uses invoice

  requires integration crm: CRMProvider

  domain
    resource ImportBatch
      total_rows: Integer = 0
```

`requires integration crm: CRMProvider` says "I need *some* provider satisfying
the `CRMProvider` capability, exposed under the local slot name `crm`." The
feature never knows which vendor fills it. The app wires it in `bindings`:

```text
  bindings
    invoice_import.crm = integrations.crm
```

The left side is `<feature>.<slot>`; the right side is `integrations.<name>` from
the registry. The doctor checks both directions: every `bindings` key must match
a real `requires integration` slot, and the bound integration must exist in
`registry.lzi`. This is the same capability-binding pattern fields use for typed
resources — see [resources-and-fields](0011-resources-and-fields.md).

## `registry.lzi` — where adapters get a name

The registry is the *only* place a provider name legitimately appears, and even
then it appears as an **adapter reference**, never as a keyword. It catalogs env
groups, capabilities, integrations, and packs:

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

Watch the small shapes the doctor enforces:

- **`capabilities` kinds are a closed catalog**: `database`, `queue`,
  `object_storage`, `mailer`, `event_bus`, `tracing`, `cache`, `search`, plus
  `integration <slot>`. Inventing a new capability kind warns — capability kinds
  move with a language cut, not a registry edit.
- **`packs` use `<name> from @scope/package`**, then `version`, `provides`,
  `requires` children. Not `pack <name>` with a nested `name` line.
- **`credentials` takes a tier label** (`platform`, `tenant`, or `actor`) and its
  children bind an env var with a *space*, not `=`: `webhook_secret
  env.CRM_WEBHOOK_SECRET`.

## The namespace policy — the gaffe magnet

This is where most cold-write mistakes happen. Adapter provenance is a closed
set, and the prefix encodes *who owns the thing*:

- **`@runtime/<name>`** — OSS commodity infrastructure with an open spec or a
  de-facto-standard layer: Postgres, Redis, S3-protocol signing, SMTP, Kafka,
  NATS. These live in the Lazuli runtime itself.
- **`@plugin/<name>`** — a *named vendor SaaS or specific product*, even if it's
  open source: Stripe, MercadoPago, Sendgrid, Twilio, Algolia, Meilisearch. These
  live in separate plugin repos.
- **`@adapter.<local>`** — a local adapter you author in the app for a contract
  the runtime/plugins don't cover (e.g. a homegrown or legacy CRM).

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

Three rules that turn this from a guideline into reflex:

1. **A provider name is NEVER a core keyword.** There is no `stripe`, `postgres`,
   `aws`, or `kafka` keyword. The provider only ever appears as the trailing
   segment of an `@runtime/` / `@plugin/` adapter ref. If you find yourself
   wanting a vendor keyword, you've found a scope violation — keep the wire thin
   (see [wire-not-reimplement](0001-wire-not-reimplement.md)).
2. **The adapter is named after the *provider*, not the consuming product.**
   MercadoPago is `@plugin/mercadopago`, never `@plugin/<your-app>/mercadopago`.
   The adapter is generic and reusable; the product is incidental.
3. **"Is this commodity infra or a named product?"** Ask it before every new
   adapter. Open spec / de-facto-OSS layer → `@runtime`. A specific named
   SaaS/tool → `@plugin`. Per-vendor business glue that fits neither →
   [an escape hatch](0002-five-escape-hatches.md) in your own Go, not the
   framework.

## `profiles.lzi` — per-environment overlays

Profiles keep environment-specific intent out of `app.lzi`. Each `profile
<env>` block overlays `urls`, `bindings`, `integrations`, and `deploy` for that
environment, so the base manifest stays a single clean contract:

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

The `local` profile binding a `@adapter.fake_crm` is the canonical way to swap a
real vendor for a fake in dev without touching feature code — the slot is the
same, only the adapter changes.

## `workspace.lzi` — optional, distributed only

A single app must not have a `workspace.lzi`. Reach for it only when you have
multiple Lazuli apps, external (non-Lazuli) services, or a shared gateway. It
declares the apps, the cross-service event `boundaries`, workspace-wide
`communication` defaults, and a provider-neutral `gateway`:

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

`boundaries` makes the cross-service event graph statically visible: which app
produces each event class and which consumes it (the `invoice.*` patterns are the
same event names features emit — see
[events-and-event-groups](0012-events-and-event-groups.md)). The `gateway` owns
only the *shape* — which app mounts which route — never the proxy/mesh
mechanics, which are runtime concerns.

When the wiring confuses you, ask the compiler rather than guessing: `lazuli
check <file>` validates a manifest standalone, and `lazuli inspect` shows the
resolved graph (see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md)).

Authoritative spec: `docs/grammar.app.md`, `docs/grammar.registry.md`,
`docs/grammar.workspace.md`, and the namespace policy in `CLAUDE.md`.
