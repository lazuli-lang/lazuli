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

## Next Implementation Cuts

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| 1 | Feature-level integration requirements | done | Add `requires integration gateway: PaymentGateway` so reusable features depend on abstract capabilities, not concrete providers. |
| 2 | App bindings | done | Bind `payments.gateway = integrations.mercadopago` or equivalent without making every feature import provider details. |
| 3 | External calls | done | `calls gateway.operation` now works in commands/jobs, appears in inspect, and is checked by LSP/doctor against feature integration slots with timeout/retry/job-idempotency guards. |
| 4 | Integration doctor rules | partial | Missing app binding, undeclared integration, type mismatch, undeclared call slot, missing timeout, missing retry, and missing job idempotency are covered. PII/legal basis/audit waits for external operation data-classification contracts. |
| 5 | Registry layout decision | done | Use native `registry.lzi` package convention with explicit import reserved for future non-standard layouts. |
| 6 | Profiles | pending | Model environment overrides such as local/staging/production URLs, sandbox provider mode, fake adapters, and deploy topology without becoming Terraform. |
| 7 | Pack registry | pending | Decide shape for Drusa packs and provider packs without turning Lazuli into a product-feature catalog. |
| 8 | Adapter binding provenance | pending | Decide how registry entries reference Drusa adapters, third-party plugin adapters, and local inline adapters without becoming a provider operation schema. |
| 9 | Workspace contract | pending | Decide the exact `workspace.lzi` shape for distributed apps spanning monorepos, multiple repos, external services, and sidecars. |
| 10 | External contract imports | pending | Decide how `contract.lzi`, OpenAPI, AsyncAPI, Proto/Buf, JSON Schema, and optional external SDK exports represent non-Lazuli services. Core Drusa should generate Go transport bindings, not make SDK a language concept. |
| 11 | Gateway/proxy contract | pending | Decide whether language uses `gateway`, `proxy`, or both for distributed ingress and service-edge routing. Keep provider proxy mechanics in Drusa/adapters. |
| 12 | Syntax highlighting audit | partial | TextMate scopes include current integration/binding/calls syntax; re-audit again after profile/workspace syntax lands. |
| 13 | IR/inspect coverage audit | partial | App, registry, requirements, bindings, and external calls appear in inspect/doctor. Profile/workspace/contract imports still need stable inspect shape. |
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

`workspace.lzi` is deferred until there is real multi-app pressure.

Intended split:

- `workspace.lzi`: semantic contract for a distributed system or monorepo,
  including apps, external contracts, shared registry, app graph, event edges,
  and gateway contracts.
- `drusa.toml`: operational Drusa config such as remote repo URLs, branches,
  provider ids, CI wiring, deploy providers, local ports, adapter provider
  choices, and other concrete mechanics.

Do not implement `workspace.lzi` before app/registry/profile contracts settle.
When it lands, it should model distributed contract shape, not repository
automation.

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

Future contract inputs may include:

- Lazuli-authored `contract.lzi`.
- OpenAPI for HTTP APIs.
- AsyncAPI for broker/event contracts.
- Proto/Buf for RPC contracts.
- JSON Schema or Avro when an enterprise broker/schema registry requires it.

## Adapter And Container Decision Pressure

`registry.lzi` is the native language-level catalog. It may contain bindings to
adapters supplied by Drusa, third-party plugins, or local app code.

Recommended model:

```lazuli
registry
  integrations
    crm: CRMProvider
      adapter @adapter.crm
```

Allowed adapter sources should eventually include:

- Drusa-maintained package adapters.
- Third-party plugin adapters.
- Local adapters declared by the app or feature package.

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
- Do not implement `workspace.lzi`, `gateway`, or `proxy` before profiles,
  app bindings, service boundaries, and registry contracts settle.
- Any magic package discovery must be visible in `lazuli inspect`, `doctor`, and
  LSP diagnostics so it does not become hidden runtime behavior.
