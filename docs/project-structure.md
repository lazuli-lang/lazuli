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
| **Client-specific** | `app/web/src/...` (default) or `app/clients/<name>/src/...` (opt-in multi), `packages/shared/` (when real cross-client sharing appears) | A refactor of the *current* client stack. Discardable if you swap React → Svelte. | The frontend toolchain (Vite, TanStack, etc.). |
| **Disposable** | `dist/`, `.lazuli/` | Nothing. Always regenerable from portable + client-specific sources. | `lazuli generate`. |

### The meta-principle

> **Each thing has ONE obvious canonical location, defined by
> `(technological layer × consumer cardinality)`. The `.lzi` is the
> conceptual anchor; physical location follows the geometry of
> consumption.**

Three corollaries fall out of this:

- **"The compiler touches it → it's Lazuli territory."** If `.lzi` /
  `.lzx` cites a file (a handler at `@fn.X`, a slot at `@client.<slot>`,
  a template at `@file.welcome`, a SQL query), the file lives in
  `app/features/<f>/`. Doctor cross-checks the path; codegen emits a
  contract the file implements. Anything that isn't cited is not Lazuli
  territory, even if it's "about" the feature.

- **Tier 2 holds every product delivery surface — Lazuli-aware *or*
  polyglot.** A client is any self-contained TS/JS application that
  ships to a user-visible deploy target. Most consume the generated
  Lazuli SDK (`dist/ts-<name>/`) and are declared in
  `Lazurite.toml [frontends.<name>]`. Some don't — an Astro marketing
  site, a Storybook deploy, a pure-content microsite. **Polyglot
  clients still live in `app/clients/<name>/`** for visual
  consolidation; they just aren't declared in the manifest, and
  Lazuli ignores them (passes the "compiler doesn't touch" test).

- **Plural when the client has a contract distinct enough to justify
  its own bundle, deploy, and release cadence — not just a different
  audience.** Audience alone (admin vs. end-user) is solved by routes
  + authorization inside one client. A second client is justified
  when *two or more* of the following diverge:
  1. **Runtime** — browser SPA vs. Expo/RN mobile vs. Electron desktop
     vs. browser extension vs. embedded webview.
  2. **Data surface** — operator dashboards expose audit tables,
     metrics, admin controls that an end-user never touches; the
     queries/commands the two clients consume are materially different,
     not just role-gated variants of the same screens.
  3. **Release cadence** — internal tooling ships continuously without
     release-note discipline; consumer-facing product ships on a
     versioned cadence with QA and changelog.
  4. **Distribution** — app store vs. public URL vs. intranet/VPN-only
     vs. enterprise SSO portal. Different signing, different update
     mechanism, different telemetry posture.

  One criterion alone is usually solvable with routes + authorization
  inside the same client. Two or more start to justify the cost of a
  separate bundle. `dist/ts-web/` vs. `dist/ts-mobile/` is plural at
  the generated tier because runtime alone is unambiguous (different
  runtime → different generated contracts); at the client tier the
  threshold is higher because the cost is higher.

- **Default is a single client; multi is opt-in.** Most projects ship
  one frontend (`app/web/`). When a real second client appears — meeting
  two-or-more of the criteria above — the project migrates
  `app/web/` → `app/clients/<name>/` and adds the second slice
  alongside. The migration is mechanical (rename + Lazurite.toml
  update), and the cost is proportional to the complexity being added.
  **Lazuli has no opinion on the client *name*; only on the structural
  rule.** A project might call its clients `customer-app` and
  `operator-tools`, `marketplace` and `admin`, or `web` and `mobile` —
  Lazuli only requires that each lives at `app/clients/<name>/` and
  that `Lazurite.toml [frontends.<name>]` declares it.

These three rules answer most "where does X live?" questions. When they
don't, the answer is in this document.

### Prior art for this layout

The "default singular, opt-in plural with explicit migration" pattern is
borrowed directly from
[Hanami 2.0's slices model](https://guides.hanamirb.org/v2.0/app/slices/) —
Hanami 1.x forced the "single app vs multi-app container" choice
upfront, found it added friction with no payoff for typical projects,
and moved to a singular default (`app/`) with optional `slices/<name>/`
that share infrastructure. Lazuli follows the same shape: `app/web/`
default, `app/clients/<name>/` opt-in.

The "separate by responsibility, not by audience" rule echoes
[Phoenix's `MyApp` / `MyAppWeb`](https://hexdocs.pm/phoenix/directory_structure.html)
split: business logic and web layer are different namespaces *in the
same project*, never different apps. When Phoenix needs to serve a web
UI + JSON API + LiveView all at once, they live as sub-modules of the
same `MyAppWeb` package, not as separate deployments. The same principle
keeps Lazuli's `app/web/` singular when an "operator dashboard" and a
"customer marketplace" share the same browser bundle but live under
different routes.

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
| Go handlers (`@fn.X`, `@hook.X`, `@validator.X`) | `app/features/<f>/handlers/<name>.go` (package `<f>handlers`) | Portable | `[planned]` — today codegen emits stubs to `dist/go/<f>/<name>.go`; pivot to canonical location is in the framework roadmap. |
| Resource-local validators | `app/features/<f>/domain/validate_<resource>.go` | Portable | `[stable]` |
| SQL queries (`query.sql @file.X`) | `app/features/<f>/queries/<X>.sql` | Portable | `[stable]` |
| Email/notification templates | `app/features/<f>/templates/<name>.<locale>.tmpl` | Portable | `[stable]` |
| Feature-local i18n | `app/features/<f>/i18n/<name>.<locale>.json` | Portable | `[stable]` |
| Slot implementations (`@client.<slot>` in `.lzx`) | `app/features/<f>/<target>/cells/<slot>.tsx` | Portable | `[partial]` — doctor rule `lzx-cell-missing-impl` checks the path; codegen emits `<slot>.gen.ts` interface; full `.lzx` → views pipeline still in progress. |
| Feature-owned concrete views | `app/features/<f>/<target>/views/<audience>/<view>.tsx` | Portable | `[partial]` — same pipeline as slot impls. |
| App-level UI (routes, layouts, state, theme) | `app/web/src/...` (default) or `app/clients/<name>/src/...` (multi) | Client-specific | `[partial]` — Lazurite scaffold currently emits to `apps/<frontend>/`; pivot to `app/web/` default is in the framework roadmap. |
| Feature UI mirror in app | `app/web/src/features/<f>/` (or `app/clients/<name>/src/features/<f>/` in multi) | Client-specific | `[partial]` — suggested convention; scaffold will enforce on next pivot. |
| Cross-client TS shared (when multi appears) | `packages/<name>/` | Outside Lazuli contract | `[stable]` — pnpm/Turborepo convention. **Never abbreviate to `pkg/`** — collides with Go's standard `pkg/` library convention and confuses contributors in polyglot projects. |
| App-wide i18n | `i18n/<name>.<locale>.json` | Portable | `[stable]` |
| Custom scripts (CI, deploy, seed) | `scripts/` | Portable | `[stable]` |
| Generated Go | `dist/go/<f>/*.gen.go` (package `<f>gen`) | Disposable | `[partial]` — today package is `<f>` and shares files with user handlers; the pivot to a dedicated `<f>gen` package is in the framework roadmap. |
| Generated TS SDK per runtime target | `dist/ts-<target>/<f>/*.gen.ts` | Disposable | `[stable]` |
| IR cache + source map | `.lazuli/*` | Disposable | `[stable]` |

---

## Tier 1 — Lazuli territory (`app/features/`, `app/*.lzi`)

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
      handlers/hash_password.go # @fn.hash_password    [planned]
      handlers/before_create.go # @hook.before_create  [planned]

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

1. **One backend.** Unlike frontends (which can ship as multiple bundles
   when a project really needs separate deploys), the Go backend is
   singular by design. There's no second consumer fighting for handler
   ownership.
2. **The compiler cites them.** `@fn.X` in `.lzi` resolves to a function
   on disk. Doctor verifies the path. Codegen emits a typed contract
   the handler implements. That's the test for Lazuli territory.

An earlier framework iteration placed user handlers in `dist/go/<f>/`
alongside `*.gen.go` files. That layout broke the "`dist/` is
disposable" invariant (a `rm -rf dist/go && lazuli generate` would
lose user code) and is being reverted; the canonical location going
forward is `app/features/<f>/<name>.go` as described above.

### Why slot impls (`.tsx`) live next to `.lzx`

When a `.lzx` view declares `cells <field> @client.<slot>`, the slot is
a user-implemented React component the codegen wires up. The compiler
cites the slot by name; doctor verifies the implementation exists;
codegen emits a typed `<Slot>Props` interface in
`dist/ts-<target>/<f>/cells/<slot>.gen.ts` that the implementation
imports for its prop types.

The slot impl is Lazuli territory (compiler cites + doctor verifies)
even though it's React/TypeScript. It lives in
`app/features/<f>/<target>/cells/`, NOT in the frontend client's `src/`.
This is the one TSX exception in the rule "TS UI lives in the
client" — slot impls are the bridge between declarative-in-`.lzx` and
imperative-in-React, and they belong on the declarative side of the
bridge.

---

## Tier 2 — Client-specific (`app/web/` default, `app/clients/<name>/` opt-in)

A **client** is one self-contained TypeScript application targeting a
single runtime/bundle/deploy unit. Most projects have one — a web SPA.
Some need a second when a real architectural divergence appears (an
operator dashboard that must ship as a separate Electron app, a mobile
PWA with offline-first behavior unsuitable for the main bundle, a
browser extension, etc.). Audience differences alone (admin vs.
customer, operator vs. end-user) do **not** justify a second client —
those are solved by routes + authorization inside the same SPA.

### Default layout (one client)

```txt
app/
  web/                          # the default client — a web SPA
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
        operator/               # admin-side routes in the SAME client
          dashboard.tsx
      features/                 # suggested: mirror app/features/<f>/ names
        messaging/
          components/
            ChatExperience.tsx
          models/
      shared/                   # cross-feature primitives
        ui/
          Button.tsx
          Modal.tsx
        forms/
        theme/
        state/
    e2e/                        # Playwright
```

### Multi-client layout (opt-in)

When a project legitimately needs separate bundles — meeting two or
more of the runtime/data-surface/release-cadence/distribution
criteria — it migrates `app/web/` → `app/clients/<name>/` and adds
the second slice. Lazuli has no opinion on the names: pick what
matches your product.

```txt
app/
  clients/
    customer-app/               # was app/web/ — renamed, keep the
      package.json              #  name your product already uses
      src/...
    operator-tools/             # new second client, justified by:
      package.json              #  - different data surface (admin)
      src/...                   #  - different release cadence
                                #  - different distribution (intranet)
    mobile/                     # third client targeting Expo, when
      package.json              #  a native mobile target appears
      src/...                   #  (different runtime)

packages/                       # appears only when real cross-client
  shared/                       #  sharing is needed (design system,
    design-system/              #  common hooks, etc.)
    ui-primitives/
```

The migration is mechanical: `mv app/web app/clients/<your-chosen-name>`,
update `Lazurite.toml [frontends.*]` to point at the new path,
regenerate. Doctor flags the inconsistency if the manifest and folder
layout drift.

`app/web/` and `app/clients/` never coexist in the same project — when
`app/clients/` appears, `app/web/` is gone (renamed into it). This
matches Hanami's slices model (the singular `app/` doesn't coexist with
`slices/<name>/` as parallel siblings; the first slice migration moves
code, doesn't add a second tier).

**Worked example.** A project with a consumer marketplace and an
internal operator dashboard sharing one Lazuli backend justifies plural
because all four criteria diverge: marketplace ships as a PWA (and
later a native mobile target), operator dashboard is web-only and
intranet-distributed; marketplace exposes customer-facing data
(listings, bookings, reviews) while operator tooling exposes audit
tables and admin controls; marketplace ships on a versioned release
cadence with QA, operator tooling ships continuously without
ceremony; marketplace is publicly distributed (app store, public URL),
operator tooling is intranet-only. That product would adopt
`app/clients/marketplace/` + `app/clients/operator/` (or whatever
names the team naturally uses). If instead the operator screens were
just "admin pages" in the same web app under `/admin/*` routes with
authorization gating — same release cadence, same distribution, same
bundle — that would stay in `app/web/`.

### Why client code lives outside `app/features/`

- **Default is singular.** A typical Lazuli project has one web SPA. It
  lives at `app/web/`, alongside `app/features/` and other `app/*`
  entries, with no `apps/` plural required.
- **Multi is rare and explicit.** When a second client is truly needed,
  the project opts in via migration. Forcing `apps/<frontend>/` plural
  on every project (even single-client ones) was over-engineering — the
  cost of two Vite configs, two `package.json` files, two deploy units
  was being paid by projects that would never use the second slot.
- **App-level concerns aren't "about a feature".** Routes, layouts,
  providers, theming, navigation chrome — these are *about the client*,
  not about messaging or account specifically. They have no canonical
  home under `app/features/`.
- **Rails analogy.** Rails groups by responsibility (`app/controllers/`,
  `app/views/`, `app/models/`), not by feature. The convention "things
  with the same lifecycle live together" produces healthier projects
  than "everything related to X lives together". Same logic applies
  here: frontend code's lifecycle is the bundle/deploy unit; backend
  code's lifecycle is the feature contract.
- **Audience ≠ client.** Phoenix's `MyAppWeb` contains controllers for
  every audience, JSON APIs, and LiveView all in one namespace — never
  as separate Phoenix apps. Lazuli follows the same rule: an "admin
  panel" and a "customer marketplace" in the same browser bundle are
  routes + state inside `app/web/`, not two separate clients.

### Suggested convention: mirror feature names

When a client has feature-specific code (models, components,
providers), the suggested convention is `<client>/src/features/<f>/`
matching the name of `app/features/<f>/`. So:

- Default project: `app/web/src/features/messaging/components/...`
- Multi-client project: `app/clients/main/src/features/messaging/...`

The Lazurite scaffold will enforce this once the pivot lands; until
then it's a recommended convention.

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
    customer/                   # package `customergen`
      resource.gen.go           # structs + repository wire
      command.gen.go            # command literals with Effect, Policy, etc.
      query.gen.go              # list/lookup + cache
      api.gen.go                # route bindings (net/http stdlib ServeMux)
      register.gen.go           # lazuli.Register(&cmd, &query, ...) init
      auth.gen.go               # if feature declares auth
      job.gen.go                # River worker registration
      webhook.gen.go            # webhook receiver + HMAC
      types.gen.go              # input/output struct types  [planned]
    migrations/                 # generated SQL DDL per resource
      001_customer.sql
      001_customer.down.sql
  ts-web/                       # SDK targeting browser runtime
    customer/
      customer.gen.ts           # interfaces + defineCommand/defineQuery wrappers
      customer.zod.ts           # zod validation schemas
      cells/                    # [partial]
        tag_editor.gen.ts       # <SlotProps> interface for app/features/.../cells/
  ts-mobile/                    # SDK targeting Expo/RN runtime
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
- `dist/ts-<target>/` plural is correct: `ts-web` and `ts-mobile` are
  *different runtimes* with different contracts (browser fetch vs. Expo
  AsyncStorage, different Zod resolvers, different lifecycle hooks).
  This is the "plural only by runtime, not by UX" rule applied at the
  generation layer.

---

## Cross-tier consumption: the three import paths

The `.lzx` → `.tsx` chain is the most subtle relation in the layout
because it crosses tiers in both directions. The diagram below covers
every consumer relationship a client has with Lazuli-generated code.
The example uses the default single-client layout (`app/web/`); for
multi-client projects, replace `app/web/` with
`app/clients/<name>/` throughout.

```txt
app/features/customer/customer.web.lzx   (intent declaration, Tier 1 Portable)
  │
  ├──[1]─► codegen emits views in dist/ts-web/customer/views/         (Tier 3 Disposable)  [planned]
  │           │
  │           └──► client imports:
  │                import { CustomerList } from '@gen/customer/views/list'
  │                (used in app/web/src/routes/customers.tsx)
  │
  ├──[2]─► codegen emits slot interfaces in dist/ts-web/customer/cells/ (Tier 3 Disposable)  [partial]
  │           │
  │           └──► author implements:
  │                app/features/customer/web/cells/tag_editor.tsx     (Tier 1 Portable)
  │                imports the interface from dist for prop types
  │                │
  │                └──► client imports the .tsx directly:
  │                     import { TagEditor } from
  │                       '@features/customer/web/cells/tag_editor'
  │
  └──[3]─► codegen emits SDK in dist/ts-web/customer/customer.gen.ts  (Tier 3 Disposable)  [stable]
              │
              └──► client imports:
                   import { listCustomers } from '@myapp/sdk/customer/customer.gen'
                   (used freely throughout app/web/src/)
```

Three arrows, three import patterns, three responsibilities:

1. **Generated view → client** — the client imports a complete
   component the compiler synthesised from the `.lzx`. No customization
   point; if you need different UI, declare a different view or
   override at the slot level.
2. **Slot interface → user impl → client** — when a view needs custom
   UI, the `.lzx` declares a slot; codegen emits the prop-type
   contract; the author writes the `.tsx` in
   `app/features/<f>/<target>/cells/`; the client imports that `.tsx`
   directly. `dist` never sees the impl, only the contract.
3. **Generated SDK → client** — for everything outside view scope
   (manual command dispatch, custom query screens, programmatic flows),
   the client imports typed wrappers from the SDK. Most client-level
   routes today take this path.

---

## Lazurite manifest

`Lazurite.toml` at the project root holds environment glue the DSL
doesn't own — framework version pin, plugin module resolution, codegen
settings, frontend topology, migration runner policy, seed policy,
local-dev overrides. The manifest is owned by the **Lazurite distro**
(Lazuli's opinionated distribution); other future distros may ship
different defaults but the schema lives in Lazuli core
(`crates/lazuli_manifest/src/lazurite_manifest/mod.rs`).

Required sections: `[project]` (name + module + schema), `[lazuli]`
(runtime version pin). Optional: `[lazurite]`, `[plugins]`,
`[generate.go]`, `[frontends.*]`, `[migrations]`, `[seeds]`, `[dev]`.

**Boundary:** the manifest never duplicates declarations the DSL owns.
App environments, URLs, CORS, audiences, and deploy gates stay in
`app.lzi` (and `profiles.lzi` for overlays); audience scoping per
client stays in `.lzx`. The manifest is the *projection over the IR*,
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
| `@fn.<name>` (custom function) | `app/features/<f>/handlers/<name>.go` | `[planned]` — today: `dist/go/<f>/<name>.go` |
| `@validator.<name>` (custom validator) | `app/features/<f>/handlers/<name>.go` or `app/features/<f>/domain/validate_<resource>.go` | `[planned]` |
| `@hook.<name>` (workflow lifecycle hook) | `app/features/<f>/handlers/<name>.go` | `[planned]` |
| Domain function extension | `app/features/<f>/domain/<name>.go` | `[stable]` |
| Integration adapter extension | `app/features/<f>/integrations/<name>.go` | `[stable]` |
| Background job handler | `app/features/<f>/handlers/<name>.go` | `[planned]` |
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

### Default project (single client)

```txt
Lazurite.toml                    # workspace manifest (Lazurite distro)
README.md
.gitignore                       # ignores dist/ .lazuli/ secrets

app/                             # Tier 1 + Tier 2 live under app/
  app.lzi                        # ↓ Tier 1 (Lazuli territory)
  design.lzi
  registry.lzi
  profiles.lzi
  features/
    customer/
      customer.lzi
      customer.lzx
      customer.web.lzx
      customer.mobile.lzx
      handlers/<name>.go         # @fn / @hook / @validator extensions  [planned]
      domain/
      queries/
      integrations/
      templates/
      i18n/
      web/cells/                 # @client.<slot> impls
      mobile/cells/

  web/                           # ↓ Tier 2 (client-specific) — the default client
    package.json
    vite.config.ts
    src/...

workspace.lzi                    # optional: distributed-system root
contracts/                       # external service contracts
  acme.ai.v1.lzi
i18n/                            # app-wide translation catalogs
scripts/                         # custom scripts (CI, deploy, seed)

dist/                            # Tier 3 — Disposable
  go/                            # generated Go backend (package <f>gen)
  ts-web/                        # generated TS SDK for browser runtime
  ts-mobile/                     # generated TS SDK for Expo/RN runtime

go.mod                           # root Go module
go.work                          # workspace (root + dist/go)
go.sum
.lazuli/                         # IR cache + manifests (disposable)
```

### Multi-client project (opt-in)

```txt
app/
  app.lzi
  features/
    ...
  clients/                       # ← replaces `web/` when multi is needed
    <product-name>/              # was app/web/ — keep your product's
      package.json               #  natural name; framework is naming-
      src/...                    #  agnostic
    <other-client>/              # second client meeting ≥2 of the
      package.json               #  runtime/data/cadence/distribution
      src/...                    #  criteria

packages/                        # appears with real cross-client sharing
  shared/
    design-system/
```

Everything else (Tier 1, Tier 3, top-level config) is identical to the
default layout. Only the `app/web/` → `app/clients/<name>/` slice
changes.

---

## When in doubt: applying the rules

The framework boundary is enforced by two questions: **does the Lazuli
compiler touch this file?** and **does it require a separate runtime
bundle?**

- **Compiler touches it** (cited by `.lzi`/`.lzx`, validated by doctor,
  used by codegen) → Tier 1, lives in `app/features/<f>/` or another
  `app/*` location.
- **Specific to a client stack but not Lazuli-cited** (a React route, a
  Vite plugin, a Tailwind config) → Tier 2, lives in `app/web/`
  (default) or `app/clients/<name>/` (multi).
- **Generated by `lazuli generate`** → Tier 3, lives in `dist/` or
  `.lazuli/`, never edited by hand.

For the singular-vs-plural question (does this code go in `app/web/` or
do I need `app/clients/`?), apply the **two-or-more-criteria** test:

A second client is justified when two or more of these diverge:

1. **Runtime** — different platform (browser/Expo/Electron/extension).
2. **Data surface** — materially different queries/commands, not just
   role-gated variants of the same screens.
3. **Release cadence** — internal continuous vs. consumer versioned.
4. **Distribution** — app store / public URL / intranet-VPN /
   enterprise SSO.

One criterion alone is usually a routes + authorization problem inside
one client (`app/web/` is enough). Two or more start to justify the
cost of separate bundles. Naming follows the product: Lazuli requires
that each client lives at `app/clients/<name>/`, never that the name
itself match any framework convention.

When the answer is genuinely ambiguous (shared TypeScript code used by
multiple clients with no Lazuli dependency, for example),
`packages/shared/` is the reserved location outside the Lazuli
contract — explicitly *not* in `app/features/`, to preserve the
"Lazuli territory" invariant.

---

## See also

- [`docs/canonical-semantics.md`](canonical-semantics.md) — full normative spec.
- [`docs/invariants.md`](invariants.md) — closed grammar/IR constraints.
- [`docs/plugin-authoring.md`](plugin-authoring.md) — when adding a
  `@plugin/<name>` adapter.
- [Hanami 2.0 Slices guide](https://guides.hanamirb.org/v2.0/app/slices/)
  — prior art for the default-singular, opt-in-multi pattern.
- [Phoenix Directory Structure](https://hexdocs.pm/phoenix/directory_structure.html)
  — prior art for the responsibility-not-audience separation principle.
