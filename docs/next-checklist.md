# Lazuli Next Checklist

This is the live working checklist for upcoming language cuts. Keep it small,
practical, and updated after each implementation so design pressure does not
get lost in chat history.

## Current Position

- `app.lzi` is the app entrypoint and operational contract.
- `registry.lzi` is the package-level catalog for env groups, capabilities,
  integrations, adapters, packs, and other global bindings.
- `env group` exists to organize app env schema without changing `env.NAME`
  references.
- `integrations` exists as a provider-neutral registry contract, not as a
  provider operation spec. It may live in `registry.lzi`, or temporarily in
  `app.lzi` for small apps.
- Service boundaries are logical ownership contracts; Drusa may materialize the
  same graph as a monolith, modular monolith, or split services.
- Adapter and dependency injection mechanics are Drusa/runtime concerns. Lazuli
  owns the registry contract and typed bindings; it should not grow a
  `container.lzi` until real plugin/runtime pressure proves that `registry.lzi`
  cannot express the contract.
- `workspace.lzi` is the optional distributed-system contract for multi-app,
  polyrepo, external-service, and gateway graphs. It is not required for normal
  apps and does not replace per-app `app.lzi`.

## Next Implementation Cuts

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| 1 | Feature-level integration requirements | done | Add `requires integration gateway: PaymentGateway` so reusable features depend on abstract capabilities, not concrete providers. |
| 2 | App bindings | done | Bind `payments.gateway = integrations.mercadopago` or equivalent without making every feature import provider details. |
| 3 | External calls | done | `calls gateway.operation` now works in commands/jobs, appears in inspect, and is checked by LSP/doctor against feature integration slots with timeout/retry/job-idempotency guards. |
| 4 | Integration doctor rules | partial | Missing app binding, undeclared integration, type mismatch, undeclared call slot, missing timeout, missing retry, and missing job idempotency are covered. PII/legal basis/audit waits for external operation data-classification contracts. |
| 5 | Registry layout decision | done | Use native `registry.lzi` package convention with explicit import reserved for future non-standard layouts. |
| 6 | Profiles | done | `profile <environment>` now models URL, binding, integration environment/adapter, and provider-neutral deploy topology overrides with inspect and doctor coverage. |
| 7 | Pack registry | done | `registry.lzi` catalogs packs and app `packs` enables them; doctor lets enabled packs satisfy `uses` and requires bindings for pack integration slots. |
| 8 | Adapter binding provenance | done | Adapter sources now derive `drusa`, `plugin`, or `local` provenance from `@drusa/...`, `@plugin/publisher/name`, `@adapter.<local>`, or local paths; doctor rejects unknown source shapes. |
| 9 | Workspace contract | done | `workspace.lzi` now models local/external apps, shared registry, event boundaries, communication propagation, and provider-neutral gateways with IR/inspect/doctor/LSP coverage. |
| 10 | External contract imports | done | `contract <name>` now models imported OpenAPI/AsyncAPI/Proto/JSON Schema/Avro plus authored records, operations, and events with IR/inspect/doctor/LSP coverage. Core Drusa should generate Go transport bindings, not make SDK a language concept. |
| 11 | Gateway/proxy contract | partial | Workspace `gateway` now covers provider-neutral ingress to apps. Raw proxy, sidecar, service mesh, and provider routing mechanics stay in Drusa/adapters. |
| 12 | Syntax highlighting audit | partial | TextMate scopes include current integration/binding/calls/profile/pack/workspace/contract syntax and adapter package refs; re-audit again after final vocabulary cleanup. |
| 13 | IR/inspect coverage audit | partial | App, registry, packs, requirements, bindings, external calls, profiles, workspace, and contracts appear in inspect/doctor. |
| 14 | Final vocabulary cleanup | pending | Revisit `route` vs URL route, `path` vs route param, audience nesting, and other naming friction only after core contracts stabilize. |

## Registry Decision Pressure

The open question is whether a root `registry.lzi` should be a native Lazuli/
Drusa package artifact or just an arbitrary file imported by `app.lzi`.

### Option A: Native `registry.lzi` Convention

`registry.lzi` lives next to `app.lzi` and is discovered by the package loader.

Pros:

- Keeps `app.lzi` thin without adding import noise.
- Fits Lazuli's opinionated, token-efficient style.
- Gives Drusa and `lazuli doctor` a stable place for global env, capabilities,
  integration registry, adapter bindings, and pack registry.
- Avoids top-of-file import boilerplate in every `.lzi` and `.lzx`.

Cons:

- Introduces filename convention as semantics.
- Needs clear package root rules in monorepos.
- Needs an escape hatch for non-standard layouts.

### Option B: Explicit Imports Everywhere

Every source file imports what it needs.

Pros:

- Fully explicit dependency graph.
- Easy to understand with no package-level conventions.
- Flexible for unusual layouts.

Cons:

- Pollutes files with boilerplate and harms token economy.
- Pushes Lazuli toward a general module language instead of an opinionated
  contract language.
- Makes reusable feature files visually noisier and easier for agents to edit
  inconsistently.

### Option C: Hybrid Package Convention

Use native package discovery for conventional files and allow explicit imports
only as an override.

Recommended default:

```text
app.lzi
registry.lzi
features/*.lzi
experiences/*.lzx
profiles/*.lzi
```

Rules:

- `app.lzi` is the composition root.
- `registry.lzi` is a package-level catalog of capabilities, env groups,
  integrations, packs, adapters, and other global bindings.
- Feature files do not import provider registries. They declare abstract
  requirements such as `requires integration gateway: PaymentGateway`.
- `app.lzi` or `registry.lzi` binds abstract requirements to concrete registry
  entries.
- Explicit `import` may exist later for non-standard package layouts, generated
  libraries, or monorepo cross-package dependencies, but it should not be the
  default authoring style.

Decision: **Option C**. It preserves opinionated defaults and token economy
while still leaving room for a deterministic future escape hatch.

## Workspace Decision Pressure

`workspace.lzi` is the optional semantic coordination artifact for real
multi-app pressure.

Intended split:

- `workspace.lzi`: semantic contract for a distributed system or monorepo,
  including apps, external contracts, shared registry, app graph, event edges,
  and gateway contracts.
- `drusa.toml`: operational Drusa config such as remote repo URLs, branches,
  provider ids, CI wiring, deploy providers, local ports, adapter provider
  choices, and other concrete mechanics.

It models distributed contract shape, not repository automation.

Expected ownership model:

- A small or medium app has one `app.lzi` and one package-level `registry.lzi`.
- A monorepo with multiple deployable apps may have one `app.lzi` /
  `registry.lzi` pair per app package.
- A distributed system spanning multiple repos may add a root `workspace.lzi`
  that references apps, external services, sidecars, shared registries, event
  edges, and public ingress/gateway contracts.
- `drusa.toml` remains operational glue: repo URLs, branches,
  provider ids, CI/deploy wiring, and concrete mechanics.

Do not make `workspace.lzi` mandatory for normal apps. It is a semantic
coordination artifact for distributed systems, not a replacement for `app.lzi`.

Naming decision:

- Use `workspace.lzi` for the semantic distributed-system contract.
- Use `drusa.toml` for Drusa's operational/tooling configuration.
- Avoid `drusa-workspace.toml` as the default name because it competes with
  `workspace.lzi` and makes the source of truth less obvious.

Polyglot contract rule:

Lazuli does not require every service to be implemented with Lazuli or Drusa.
It requires every service participating in the workspace graph to have a
contract.

Examples:

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

The `ai` service might be Python/FastAPI, Java, Node, Rust, or another stack.
Lazuli owns the API/event/schema contract and context propagation guarantees.
Drusa materializes that contract mostly as Go runtime wiring: typed HTTP/RPC
clients, event publishers/consumers, webhook receivers, mocks, gateway config,
and contract tests. Adapters own HTTP, gRPC/Connect, Kafka, NATS, RabbitMQ,
SQS, Pub/Sub, Envoy, Kubernetes ingress, and other concrete transports.

SDK exports for Python/TypeScript/etc. are optional contract-publication
artifacts for external teams or partners. They are not the central runtime
model for Lazuli apps.

Contract inputs now include:

- Lazuli-authored `contract.lzi`.
- OpenAPI for HTTP APIs.
- AsyncAPI for broker/event contracts.
- Proto/Buf for RPC contracts.
- JSON Schema or Avro when an enterprise broker/schema registry requires it.

Canonical authoring:

```lazuli
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

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

## Adapter And Container Decision

`registry.lzi` is the native language-level catalog. It may contain bindings to
adapters supplied by Drusa, third-party plugins, or local app code.

Canonical model:

```lazuli
registry
  integrations
    crm: CRMProvider
      adapter @adapter.crm

    payments: PaymentGateway
      adapter @drusa/mercadopago

    bureau: CreditBureau
      adapter @plugin/acme/serasa

    ai: AiInference
      adapter "./integrations/ai.go"
```

Allowed adapter sources:

- `@drusa/<adapter>` for Drusa-maintained adapters.
- `@plugin/<publisher>/<adapter>` for third-party plugin adapters.
- `@adapter.<name>` for local adapter extension references.
- Local paths for app-owned adapters.

`lazuli inspect` exposes `adapter_provenance` as `drusa`, `plugin`, or
`local`. `lazuli doctor` rejects unknown source shapes.

Do not add a `container.lzi` yet.

Reason:

- Dependency inversion belongs in the language contract: features require
  abstract integrations/capabilities, and app/registry bindings choose concrete
  implementations.
- Dependency injection mechanics belong in Drusa: construction order,
  lifetimes, logger/database/client instances, test doubles, and runtime
  wiring.
- Provider details belong in adapters/config: HTTP endpoints, optional provider
  SDK setup inside Go adapters, connection pools, logger sinks, database driver
  settings, and cloud ids.

If real adapters need static checks that cannot be expressed through
`registry.lzi`, promote the missing part as a small registry primitive before
creating a broad container language.

## Gateway And Proxy Decision Pressure

Distributed apps will need a way to model ingress and cross-service edges, but
the language should avoid becoming Envoy/Kubernetes config.

Likely split:

- Lazuli language: `gateway` or `proxy` contract for public ingress, route
  ownership, auth propagation, tenant propagation, timeout/retry policy, and
  service exposure.
- Drusa framework: generated gateways, service clients, local dev routing,
  request context propagation, and reverse proxy/runtime wiring.
- Adapters: Envoy, Kubernetes ingress, Cloud Run, Fly proxy, service mesh,
  gRPC/Connect transport, and provider-specific routing.

Keep the word `proxy` under consideration, but prefer a higher-level language
term such as `gateway` if the construct represents application ingress rather
than raw proxy mechanics.

## Guardrails

- Do not put concrete providers such as MercadoPago, Serasa, Stripe, AWS, or
  Kubernetes into core syntax.
- Do not make every `.lzi` file repeat imports for common package context.
- Do not let `app.lzi` become an implementation file. It composes the app.
- Do not let `registry.lzi` become a provider operation schema. It catalogs
  what exists and how global bindings resolve.
- Do not introduce `container.lzi` as a runtime DI config unless registry
  contracts fail under real adapter/plugin pressure.
- `workspace.lzi` and provider-neutral `gateway` are now implemented. Keep raw
  `proxy`, sidecar, service mesh, and provider routing mechanics in
  Drusa/adapters unless future static-analysis pressure justifies a language
  primitive.
- Any magic package discovery must be visible in `lazuli inspect`, `doctor`, and
  LSP diagnostics so it does not become hidden runtime behavior.
