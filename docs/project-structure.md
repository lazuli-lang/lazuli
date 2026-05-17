# Lazuli Project Structure

> The `.lzi` file is the canonical anchor of every feature. Everything
> else is derived from it, consumes it, or is scoped to a specific
> runtime that implements it.

This document defines where files live in a Lazurite-scaffolded Lazuli
project, the principle behind the layout, and the boundary between
what's portable across stacks, what's specific to a chosen client, and
what's a disposable artifact.

It also serves as the navigation map for new contributors: when you ask
"where does X live?", the answer is one lookup away.

---

## Three durability tiers

Every file in a Lazuli project belongs to exactly one tier, and the tier
tells you what survives a refactor, a stack swap, or a `rm -rf`.

| Tier | Lives in | Survives... | Owner |
|---|---|---|---|
| **Portable** (Lazuli territory) | `app/features/<f>/`, `app/*.lzi`, `contracts/`, `workspace.lzi`, `Lazurite.toml`, `i18n/`, `scripts/` | A stack swap. If you migrate the backend to Rails, this directory comes with you — the `.lzi` is the spec the reimplementation follows. | The Lazuli compiler. |
| **Client-specific** | `apps/<frontend>/src/...`, `packages/shared/` (if it appears) | A refactor of the *current* client stack. Discardable if you swap React → Svelte. | The frontend toolchain (Vite, TanStack, etc.). |
| **Disposable** | `dist/`, `.lazuli/` | Nothing. Always regenerable from portable + client-specific sources. | `lazuli generate`. |

### The meta-principle

> **Each thing has ONE obvious canonical location, defined by
> `(technological layer × consumer cardinality)`. The `.lzi` is the
> conceptual anchor; physical location follows the geometry of
> consumption.**

Two corollaries fall out of this:

- **"The compiler touches it → it's Lazuli territory."** If `.lzi` /
  `.lzx` cites a file (a handler at `@fn.X`, a slot at `@client.<slot>`,
  a template at `@file.welcome`, a SQL query), the file lives in
  `app/features/<f>/`. Doctor cross-checks the path; codegen emits a
  contract the file implements. Anything that isn't cited is not Lazuli
  territory, even if it's "about" the feature.
- **One backend, N frontends.** Backend code (Go handlers) has a single
  canonical home (`app/features/<f>/`). Frontend code has one home per
  frontend (`apps/<frontend>/src/...`). Forcing symmetry by pulling all
  frontend code into `app/features/` would break Vite/TanStack
  conventions, fragment shared UI primitives, and gain nothing.

This rule is enough to answer most "where does X live?" questions. When
it isn't, the answer is in this document.

---

## Maturity status

Sections in this document carry one of three status tags:

- `[stable]` — works end-to-end, covered by tests and doctor rules,
  documented in canonical references.
- `[partial]` — some part of the pipeline exists, others don't. The
  section names what works today and what's still pending.
- `[planned]` — vocabulary and shape are defined, implementation is
  pending. Useful as a forward reference; not yet usable.

The tags are stable across releases — a tier moving from `[partial]` to
`[stable]` is itself an event worth committing.

---

## Canonical location table

| Concern | Where it lives | Tier | Status |
|---|---|---|---|
| Feature DSL (`.lzi`) | `app/features/<f>/<f>.lzi` | Portable | `[stable]` |
| Surface abstract (`.lzx`) | `app/features/<f>/<f>.lzx` | Portable | `[stable]` |
| Surface concrete per target (`.web.lzx`, `.mobile.lzx`) | `app/features/<f>/<f>.<target>.lzx` | Portable | `[stable]` |
| App manifest | `app/app.lzi` | Portable | `[stable]` |
| Design tokens | `app/design.lzi` | Portable | `[stable]` |
| Integration registry | `app/registry.lzi` | Portable | `[stable]` |
| Env profile overlays | `app/profiles.lzi` or `profiles/**/*.lzi` | Portable | `[stable]` |
| External service contracts | `contracts/**/*.lzi` | Portable | `[stable]` |
| Distributed-system root | `workspace.lzi` | Portable | `[stable]` (optional) |
| Lazurite manifest | `Lazurite.toml` | Portable | `[stable]` |
| Go handlers (`@fn.X`, `@hook.X`, `@validator.X`) | `app/features/<f>/<name>.go` (package `<f>`) | Portable | `[planned]` — see [ADR-0001](adr/0001-handler-home-and-portability-tiers.md). Today codegen emits stubs to `dist/go/<f>/<name>.go`; pivot in progress. |
| Resource-local validators | `app/features/<f>/domain/validate_<resource>.go` | Portable | `[stable]` |
| SQL queries (`query.sql @file.X`) | `app/features/<f>/queries/<X>.sql` | Portable | `[stable]` |
| Email/notification templates | `app/features/<f>/templates/<name>.<locale>.tmpl` | Portable | `[stable]` |
| Feature-local i18n | `app/features/<f>/i18n/<name>.<locale>.json` | Portable | `[stable]` |
| Slot implementations (`@client.<slot>` in `.lzx`) | `app/features/<f>/<target>/cells/<slot>.tsx` | Portable | `[partial]` — doctor rule `lzx-cell-missing-impl` checks the path; codegen emits `<slot>.gen.ts` interface; full `.lzx` → views pipeline still in progress. |
| Feature-owned concrete views | `app/features/<f>/<target>/views/<audience>/<view>.tsx` | Portable | `[partial]` — same pipeline as slot impls. |
| App-level UI (routes, layouts, state, theme) | `apps/<frontend>/src/...` | Client-specific | `[stable]` |
| Feature UI mirror in app | `apps/<frontend>/src/features/<f>/` (suggested convention) | Client-specific | `[partial]` — Lazurite template doesn't yet scaffold this; hostpoint reference shows the shape. |
| Cross-frontend TS shared (if it appears) | `packages/shared/` | Outside Lazuli contract | `[planned]` — convention reserved; create when concrete sharing pain appears. |
| App-wide i18n | `i18n/<name>.<locale>.json` | Portable | `[stable]` |
| Custom scripts (CI, deploy, seed) | `scripts/` | Portable | `[stable]` |
| Generated Go | `dist/go/<f>/*.gen.go` (package `<f>gen`) | Disposable | `[partial]` — today package is `<f>` and shares files with user handlers; ADR-0001 pivots to dedicated `<f>gen` package. |
| Generated TS SDK per frontend | `dist/ts-<target>/<f>/*.gen.ts` | Disposable | `[stable]` |
| IR cache + source map | `.lazuli/*` | Disposable | `[stable]` |

---

## Tier 1 — Lazuli territory (`app/`)

This is the portable kernel. Everything here is touched by the Lazuli
compiler in some way — parsed (`.lzi`/`.lzx`), referenced (handler
paths, slot impls, SQL files), validated (doctor), or generated
*against* (the compiler emits contracts that files here must implement).

### Layout

```txt
app/
  app.lzi                       # app entrypoint: envs, urls, uses, deploy gates
  design.lzi                    # design tokens
  registry.lzi                  # integrations, packs, capabilities, bindings
  profiles.lzi                  # optional: env-specific overlays

  features/
    customer/
      customer.lzi              # DSL contract (domain, policies, commands, queries)
      customer.lzx              # abstract experience (views, actions, anchors)
      customer.web.lzx          # web platform projection
      customer.mobile.lzx       # mobile platform projection
      customer.ctx.md           # optional: LLM context pack for this feature

      # Extension code — Go handlers in the feature package
      hash_password.go          # @fn.hash_password    [planned: see ADR-0001]
      before_create.go          # @hook.before_create  [planned: see ADR-0001]

      domain/
        risk_score.go           # domain function extension
        validate_customer.go    # resource-local validator

      queries/
        lifetime_value.sql      # raw SQL referenced via `query.sql @file.lifetime_value`

      integrations/
        stripe.go               # webhook verifier / adapter handler

      templates/
        welcome.en-US.tmpl
        welcome.pt-BR.tmpl

      i18n/
        customer.en-US.json
        customer.pt-BR.json

      web/                      # platform projection of .web.lzx
        cells/
          tag_editor.tsx        # @client.tag_editor slot impl
        views/admin/
          list.tsx              # concrete view override (rare; usually generated)

      mobile/                   # platform projection of .mobile.lzx
        cells/
          tag_editor.tsx
```

### Why Go handlers live next to `.lzi`

Two anchoring reasons:

1. **One backend.** Unlike frontends (where the same feature can have
   different UIs per audience or per app), the Go backend is singular.
   There's no second consumer fighting for handler ownership.
2. **The compiler cites them.** `@fn.X` in `.lzi` resolves to a function
   on disk. Doctor verifies the path. Codegen emits a typed contract
   the handler implements. That's the test for Lazuli territory.

This contradicts what `dist/go/` looks like in projects scaffolded
before ADR-0001 — those have user handlers mixed with `*.gen.go` files
in `dist/go/<f>/`. That layout broke the "`dist/` is disposable"
invariant (a `rm -rf dist/go && lazuli generate` would lose user code)
and was the wrong trade in retrospect. The pivot back to
`app/features/<f>/<name>.go` is in progress; see [ADR-0001].

### Why slot impls (`.tsx`) live next to `.lzx`

When a `.lzx` view declares `cells <field> @client.<slot>`, the slot is
a user-implemented React component the codegen wires up. The compiler
cites the slot by name; doctor verifies the implementation exists;
codegen emits a typed `<Slot>Props` interface in
`dist/ts-<target>/<f>/cells/<slot>.gen.ts` that the implementation
imports for its prop types.

The slot impl is Lazuli territory (compiler cites + doctor verifies) even
though it's React/TypeScript. It lives in `app/features/<f>/<target>/cells/`,
NOT in the frontend app's `src/`.

This is the one TSX exception in the rule "TS UI lives in
`apps/<frontend>/`". Slot impls are the bridge between
declarative-in-`.lzx` and imperative-in-React, and they belong on the
declarative side of the bridge.

---

## Tier 2 — Client-specific (`apps/<frontend>/`)

Each frontend is a self-contained TypeScript application with its own
toolchain (Vite + TanStack today; could be Next, Remix, or RN/Expo).
The app consumes the Lazuli-generated SDK and slot impls but owns
everything else: routes, state, layouts, theming, shared primitives.

### Layout

```txt
apps/
  hostpoint-app/                # one app per audience cluster (host + traveler)
    package.json
    vite.config.ts
    tsconfig.json
    index.html
    public/
    src/
      main.tsx
      App.tsx
      routes/                   # file-based routing (TanStack convention)
        account/
          login.tsx
          register.tsx
      features/                 # suggested: mirror app/features/<f>/ names
        messaging/
          models/
          presentation/
            components/
              ChatExperience.tsx
      shared/                   # cross-feature primitives
        ui/
          Button.tsx
          Modal.tsx
        forms/
        theme/
        application/
          state/
    e2e/                        # Playwright

  hostpoint-os/                 # second app for the operator audience
    src/
      ...
```

### Why frontend code lives outside `app/features/`

- **N frontends per project.** Hostpoint ships two (`hostpoint-app`,
  `hostpoint-os`). Each has its own Vite config, routing tree, state
  shape, and design surface. Forcing them into `app/features/<f>/web.app/`
  vs `web.os/` would create a Frankenstein tree and break Vite/TanStack
  conventions (file-based routing, code splitting, HMR).
- **App-level concerns aren't "about a feature".** Routes, layouts,
  providers, theming, navigation chrome — these are *about the app*, not
  about messaging or account specifically. They have no canonical home
  under `app/features/`.
- **Rails analogy.** Rails groups by responsibility (`app/controllers/`,
  `app/views/`, `app/models/`), not by feature. The convention "things
  with the same lifecycle live together" produces healthier projects
  than "everything related to X lives together". Same logic applies
  here: frontend code's lifecycle is the frontend; backend code's
  lifecycle is the feature contract.

### Suggested convention: mirror feature names

When a frontend has feature-specific code (models, components,
providers), the suggested convention is
`apps/<frontend>/src/features/<f>/` matching the name of
`app/features/<f>/`. The Lazurite scaffold doesn't enforce this
[`[partial]`], but adopting it from day one keeps cross-tier navigation
trivial.

---

## Tier 3 — Disposable (`dist/`, `.lazuli/`)

Generated artifacts. Always regenerable from Tier 1 + Tier 2 sources.
Safe to delete; safe to gitignore (and usually is).

### Layout

```txt
dist/
  go/                           # `lazuli generate go --out dist/go`
    main.go                     # entrypoint (emit_main=true)
    go.mod                      # sub-module (submodule=true)
    customer/
      resource.gen.go           # structs + repository wire
      command.gen.go            # command literals with Effect, Policy, etc.
      query.gen.go              # list/lookup + cache
      api.gen.go                # route bindings (net/http stdlib ServeMux)
      register.gen.go           # lazuli.Register(&cmd, &query, ...) init
      auth.gen.go               # if feature declares auth
      job.gen.go                # River worker registration
      webhook.gen.go            # webhook receiver + HMAC
      types.gen.go              # input/output struct types  [planned per ADR-0001]
    migrations/                 # generated SQL DDL per resource
      001_customer.sql
      001_customer.down.sql
  ts-web/                       # SDK for [frontends.web] target
    customer/
      customer.gen.ts           # interfaces + defineCommand/defineQuery wrappers
      customer.zod.ts           # zod validation schemas
      cells/                    # [partial]
        tag_editor.gen.ts       # <SlotProps> interface for app/features/.../cells/
  ts-mobile/                    # SDK for [frontends.mobile] target
    customer/
      customer.gen.ts
      customer.zod.ts

.lazuli/                        # internal cache + manifests
  graph.json                    # IR snapshot for incremental codegen
  source-map.json               # IR position ↔ generated line map
  manifest.json                 # extension file registry
```

### Conventions

- `dist/` is the canonical output directory (web ecosystem default —
  Vite, esbuild, tsc, etc.). `.lazuli/` is reserved for internal
  **cache** and **manifests** (graph snapshots, source maps, extension
  file registry); never user-facing code.
- Generated files in `dist/` are regen-only. Do not hand-edit. They may
  be committed (vendored/deploy builds) or gitignored (default for
  `lazuli new` scaffold); either way they are not source of truth.
- Generated code is split per feature × category, not one giant file
  per project. This keeps stack traces, diffs, source maps, and future
  granular regeneration anchored to the owning feature.

---

## Cross-tier consumption: the three import paths

The `.lzx` → `.tsx` chain is the most subtle relation in the layout
because it crosses tiers in both directions. The diagram below covers
every consumer relationship a frontend has with Lazuli-generated code.

```txt
app/features/customer/customer.web.lzx   (intent declaration, Tier 1 Portable)
  │
  ├──[1]─► codegen emits views in dist/ts-web/customer/views/         (Tier 3 Disposable)  [planned]
  │           │
  │           └──► app imports:
  │                import { CustomerList } from '@gen/customer/views/list'
  │                (used in apps/hostpoint-app/src/routes/customers.tsx)
  │
  ├──[2]─► codegen emits slot interfaces in dist/ts-web/customer/cells/ (Tier 3 Disposable)  [partial]
  │           │
  │           └──► author implements:
  │                app/features/customer/web/cells/tag_editor.tsx     (Tier 1 Portable)
  │                imports the interface from dist for prop types
  │                │
  │                └──► app imports the .tsx directly:
  │                     import { TagEditor } from
  │                       '@features/customer/web/cells/tag_editor'
  │
  └──[3]─► codegen emits SDK in dist/ts-web/customer/customer.gen.ts  (Tier 3 Disposable)  [stable]
              │
              └──► app imports:
                   import { listCustomers } from '@hostpoint/sdk/customer/customer.gen'
                   (used freely throughout apps/hostpoint-app/src/)
```

Three arrows, three import patterns, three responsibilities:

1. **Generated view → app** — the app imports a complete component the
   compiler synthesised from the `.lzx`. No customization point; if you
   need different UI, declare a different view or override at the slot
   level.
2. **Slot interface → user impl → app** — when a view needs custom UI,
   the `.lzx` declares a slot; codegen emits the prop-type contract;
   the author writes the `.tsx` in `app/features/<f>/<target>/cells/`;
   the app imports that `.tsx` directly. `dist` never sees the impl,
   only the contract.
3. **Generated SDK → app** — for everything outside view scope (manual
   command dispatch, custom query screens, programmatic flows), the app
   imports typed wrappers from the SDK. Most app-level routes today
   take this path.

---

## Lazurite manifest

`Lazurite.toml` at the project root holds environment glue the DSL
doesn't own — framework version pin, plugin module resolution, codegen
settings, frontend topology, migration runner policy, seed policy,
local-dev overrides. The manifest is owned by the **Lazurite distro**
(Lazuli's opinionated distribution); other future distros may ship
different defaults but the schema lives in Lazuli core
(`crates/lazuli_cli/src/lazurite_manifest.rs`).

Required sections: `[project]` (name + module + schema), `[lazuli]`
(runtime version pin). Optional: `[lazurite]`, `[plugins]`,
`[generate.go]`, `[frontends.*]`, `[migrations]`, `[seeds]`, `[dev]`.

**Boundary:** the manifest never duplicates declarations the DSL owns.
App environments, URLs, CORS, audiences, and deploy gates stay in
`app.lzi` (and `profiles.lzi` for overlays); audience scoping per
frontend stays in `.lzx`. The manifest is the *projection over the IR*,
not a parallel source.

Doctor emits `MANIFEST-REQUIRED-001` when `.lzi` references `@plugin/*`
but the manifest is missing. Fixture suites used only for codegen
testing (no `@plugin/*` refs) may omit the manifest entirely.

See the `lazurite-scaffold` proposal (operational archive) for the full
schema and rationale.

---

## Extension path conventions

When `.lzi` cites an extension by name (`@fn.X`, `@hook.X`,
`@validator.X`, `query.sql @file.X`), the file path follows convention.
Doctor enforces the resolution rule.

| Citation | File path | Status |
|---|---|---|
| `@fn.<name>` (custom function) | `app/features/<f>/<name>.go` | `[planned]` per ADR-0001 (today: `dist/go/<f>/<name>.go`) |
| `@validator.<name>` (custom validator) | `app/features/<f>/<name>.go` or `app/features/<f>/domain/validate_<resource>.go` | `[planned]` per ADR-0001 |
| `@hook.<name>` (workflow lifecycle hook) | `app/features/<f>/<name>.go` | `[planned]` per ADR-0001 |
| Domain function extension | `app/features/<f>/domain/<name>.go` | `[stable]` |
| Integration adapter extension | `app/features/<f>/integrations/<name>.go` | `[stable]` |
| Background job handler | `app/features/<f>/<name>.go` | `[planned]` per ADR-0001 |
| Webhook verifier or handler | `app/features/<f>/integrations/<name>.go` | `[stable]` |
| Email/notification template | `app/features/<f>/templates/<name>.<locale>.tmpl` | `[stable]` |
| Feature-local i18n catalog | `app/features/<f>/i18n/<name>.<locale>.json` | `[stable]` |
| SQL query (`query.sql @file.X`) | `app/features/<f>/queries/<name>.sql` | `[stable]` |
| Slot implementation (`@client.<slot>`) | `app/features/<f>/<target>/cells/<name>.tsx` | `[partial]` |
| Concrete view override | `app/features/<f>/<target>/views/<audience>/<view>.tsx` | `[partial]` |

**Filename rule:** filenames inside extension folders must match the
DSL reference name. `@fn.verify_password` → `verify_password.go` with
`func VerifyPassword(...)`. Doctor enforces this.

**Escape hatch:** use `at "./path"` in `.lzi` only when a file
intentionally lives outside convention. Doctor allows it but flags
divergence so reviewers see the exception.

Feature-local files belong to the feature that owns the capability,
even when they extend another feature's UI. For example, `customer_tags`
can extend `@anchor.customer_detail` (a customer-feature anchor), but
its `tag_editor` implementation remains under
`app/features/customer_tags/web/cells/tag_editor.tsx` — the owner is the
feature that declared the slot, not the feature that consumes it.

---

## Experience sources (`.lzi` / `.lzx` / `.<target>.lzx`)

`.lzi` is the domain/capability contract. It owns resources, policies,
commands, workflows, jobs, webhooks, events, security contracts, and
extension contracts. **It does not need a surface file to compile** — a
backend-only feature is valid.

`.lzx` is the abstract experience/view model. It imports one or more
`.lzi` features and declares product-level views, actions, anchors, and
exposure intent without choosing a concrete platform widget.

`.web.lzx` and `.mobile.lzx` are protected compound suffixes for
platform projections. They `use` an abstract experience and declare how
each audience is rendered on that platform. Product axes such as
`audience admin` or `tenant acme` live in the file body. Additional
physical splits such as `customer.public.web.lzx` are organization
only; the protected platform segment remains immediately before `.lzx`,
and the header remains the semantic truth.

Dependency direction is fixed:

```txt
customer.lzi        # no UI dependency
customer.lzx        # uses customer
customer.web.lzx    # uses experience customer
customer.mobile.lzx # uses experience customer
```

Concrete surface variants use whole-view redeclarations. Do not use
cascade operators such as `columns += score`; redeclare the complete
view for the audience/tenant combination.

---

## Registry and contracts

`registry.lzi` is a catalog, not an implementation folder. It may list
available packs, integrations, env schema, and capabilities. A pack
entry such as `customer_import from @runtime/customer-import` points to
reusable source that the Lazuli runtime can materialize; pack
internals, provider payloads, handlers, and adapter mechanics remain in
the pack/adapters.

`workspace.lzi` is optional. Use it at a monorepo/polyrepo/system root
when a product has multiple apps, external services, shared event
contracts, or gateway edges. It points at app entrypoints and external
contracts; repo URLs, branches, local ports, broker providers, proxy
implementations, and deploy mechanics belong in `Lazurite.toml` or
adapter config.

`contracts/**/*.lzi` contains external service contracts, not service
implementations. A contract can import OpenAPI, AsyncAPI, Proto, JSON
Schema, or Avro and can declare Lazuli-native records, operations, and
events for doctor/codegen. The Lazuli runtime consumes those contracts
to wire Go transport bindings; the external service may be implemented
in Python, Java, Node, Rust, or any other stack.

---

## Top-level layout reference

```txt
Lazurite.toml                    # workspace manifest (Lazurite distro)
README.md
.gitignore                       # ignores dist/ .lazuli/ secrets

app/                             # Tier 1 — Lazuli territory (portable)
  app.lzi
  design.lzi
  registry.lzi
  profiles.lzi
  features/
    customer/
      customer.lzi
      customer.lzx
      customer.web.lzx
      customer.mobile.lzx
      <name>.go                  # @fn / @hook / @validator extensions  [planned per ADR-0001]
      domain/
      queries/
      integrations/
      templates/
      i18n/
      web/cells/                 # @client.<slot> impls
      mobile/cells/

apps/                            # Tier 2 — Client-specific (per frontend)
  hostpoint-app/
    src/...                      # routes, state, layouts, shared UI
  hostpoint-os/
    src/...

workspace.lzi                    # optional: distributed-system root
contracts/                       # external service contracts
  acme.ai.v1.lzi
i18n/                            # app-wide translation catalogs
scripts/                         # custom scripts (CI, deploy, seed)

dist/                            # Tier 3 — Disposable
  go/                            # generated Go backend
  ts-web/                        # generated TS SDK for [frontends.web]
  ts-mobile/                     # generated TS SDK for [frontends.mobile]

go.mod                           # root Go module
go.work                          # workspace (root + dist/go)
go.sum
.lazuli/                         # IR cache + manifests (disposable)
```

---

## When in doubt: applying the rule

The framework boundary is enforced by one question: **does the Lazuli
compiler touch this file?**

- **Yes** (cited by `.lzi`/`.lzx`, validated by doctor, used by codegen)
  → Tier 1, lives in `app/features/<f>/` or another `app/` location.
- **No, but it's specific to a chosen client stack** (a React route, a
  Vite plugin, a Tailwind config) → Tier 2, lives in
  `apps/<frontend>/`.
- **Generated by `lazuli generate`** → Tier 3, lives in `dist/` or
  `.lazuli/`, never edited by hand.

This applies recursively. The question "where does X live?" reduces to
"who consumes X?" and "is X regenerable?". The location follows.

When the answer is genuinely ambiguous (shared TypeScript code used by
multiple frontends with no Lazuli dependency, for example),
`packages/shared/` is the reserved location outside the Lazuli
contract — explicitly *not* in `app/features/`, to preserve the
"Lazuli territory" invariant.

---

## See also

- [`docs/adr/0001-handler-home-and-portability-tiers.md`][ADR-0001] —
  decision record for the handler-home pivot away from `dist/go/`.
- [`docs/canonical-semantics.md`](canonical-semantics.md) — full normative spec.
- [`docs/invariants.md`](invariants.md) — closed grammar/IR constraints.
- [`docs/plugin-authoring.md`](plugin-authoring.md) — when adding a
  `@plugin/<name>` adapter.

[ADR-0001]: adr/0001-handler-home-and-portability-tiers.md
