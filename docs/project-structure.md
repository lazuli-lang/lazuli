# Lazuli Project Structure

Feature folders are the source of truth. Generated output is disposable.

A Lazurite-scaffolded app (the default produced by `lazuli new`) follows the
shape below. Bare-mode (`lazuli new --template=bare`) omits `Lazurite.toml`
and may keep sources at the root; the full Lazurite shape uses `app/` as the
product kernel and `frontends/<target>/` as delivery adapters.

```txt
Lazurite.toml                # workspace manifest (Lazurite distro)
app/
  app.lzi                    # app entrypoint: envs, urls, uses, deploy gates
  design.lzi                 # design tokens
  registry.lzi               # integrations, packs, capabilities, bindings
  profiles.lzi               # optional: env-specific overlays

  features/
    customer/
      customer.lzi           # DSL surface
      customer.lzx           # abstract experience
      customer.web.lzx       # web projection
      customer.mobile.lzx    # mobile projection
      customer.ctx.md        # LLM context pack
      handlers/              # @fn.*/@validator.*/@hook.* extension code
        hash_password.go
        before_create.go
      domain/                # domain functions, resource-local validators
        risk_score.go
        validate_customer.go
      queries/               # raw SQL files referenced via query.sql @file.<name>
        lifetime_value.sql
      jobs/                  # background job handler extensions
        recompute_scores.go
      integrations/          # webhook verifiers, adapter handlers
        stripe.go
      templates/             # email/notification templates per locale
        welcome.en-US.tmpl
        welcome.pt-BR.tmpl
      i18n/                  # feature-local catalogs
        customer.en-US.json
        customer.pt-BR.json
      web/                   # feature-owned React views/cells
        cells/
          status_cell.tsx
        views/admin/
          list.tsx
frontends/
  web/
    index.html
    main.tsx
    package.json
    vite.config.ts
    tsconfig.json
    tailwind.config.ts
    shell/
      root.tsx
      layout.tsx
    theme/
      globals.css
    ui/
      button.tsx
    hooks/
    lib/

workspace.lzi                # optional: distributed-system root

contracts/
  acme.ai.v1.lzi

i18n/                        # app-wide translation catalogs
  common.en-US.json
  common.pt-BR.json

scripts/                     # custom scripts (CI, deploy, seed, etc.)
  seed.sh

dist/                        # generated code (regen-only, gitignored by default)
  go/                        # `lazuli generate go --out dist/go`
    main.go                  # entrypoint (emit_main=true)
    go.mod                   # sub-module (submodule=true)
    customer/
      resource.gen.go        # structs + repository wire
      command.gen.go         # handler + middleware
      query.gen.go           # list/lookup + cache
      api.gen.go             # route binding (net/http stdlib ServeMux)
      auth.gen.go            # if feature declares auth
      job.gen.go             # River worker registration
      webhook.gen.go         # webhook receiver + HMAC
      notification.gen.go    # channel dispatcher
      storage.gen.go         # signed URL handler
      translation.gen.go     # i18n catalog loader
    migrations/              # generated SQL DDL per resource
      20260513_001_customer.up.sql
      20260513_001_customer.down.sql
  ts-web/                    # `[frontends.web]` audience-scoped SDK
    customer/
      api.ts
      types.ts
  ts-mobile/                 # `[frontends.mobile]` audience-scoped SDK
    customer/
      api.ts
      types.ts

go.mod                       # root module (`module <project>`)
go.work                      # workspace (root + dist/go)
go.sum

.lazuli/                     # internal cache + manifests (gitignored)
  graph.json                 # IR snapshot for incremental codegen
  source-map.json            # IR position ↔ generated line map
  manifest.json              # extension file registry

.gitignore                   # ignores dist/ .lazuli/ secrets
README.md
```

## Source

These are authored and committed:

- `Lazurite.toml` (Lazurite-scaffolded apps; see §"Lazurite manifest" below)
- `app/features/**/<feature>.lzi`
- `app/app.lzi`
- `app/design.lzi`
- `app/registry.lzi`
- `workspace.lzi` when the repo is a distributed-system root
- `contracts/**/*.lzi` for external service/schema contracts
- `app/profiles.lzi` or `profiles/**/*.lzi`
- `app/features/**/<feature>.lzx`
- `app/features/**/<feature>.web.lzx`
- `app/features/**/<feature>.mobile.lzx`
- `app/features/**/<feature>.ctx.md`
- extension code under `handlers/`, `domain/`, `queries/`, `jobs/`, `integrations/`, `templates/`, `i18n/`
- feature-owned web/mobile views and cells under `app/features/**/{web,mobile}/`
- frontend adapter code under `frontends/<target>/{shell,theme,ui,hooks,lib}/`
- adapter configuration
- top-level `i18n/`, `scripts/`

`registry.lzi` is a catalog, not an implementation folder. It may list
available packs, integrations, env schema, and capabilities. A pack entry such
as `customer_import from @runtime/customer-import` points to reusable source that
the Lazuli runtime can materialize; pack internals, provider payloads, handlers, and adapter
mechanics remain in the pack/adapters.

`workspace.lzi` is optional. Use it at a monorepo/polyrepo/system root when a
product has multiple apps, external services, shared event contracts, or
gateway edges. It points at app entrypoints and external contracts; repo URLs,
branches, local ports, broker providers, proxy implementations, and deploy
mechanics belong in `Lazurite.toml` or adapter config.

`contracts/**/*.lzi` contains external service contracts, not service
implementations. A contract can import OpenAPI, AsyncAPI, Proto, JSON Schema,
or Avro and can declare Lazuli-native records, operations, and events for
doctor/codegen. the Lazuli runtime consumes those contracts to wire Go transport bindings;
the external service may be implemented in Python, Java, Node, Rust, or any
other stack.

## Lazurite manifest

`Lazurite.toml` at the project root holds environment glue the DSL doesn't
own — framework version pin, plugin module resolution, codegen settings,
frontend topology, migration runner policy, seed policy, local-dev overrides.
The manifest is owned by the **Lazurite distro** (Lazuli's opinionated
distribution); other future distros may ship different defaults but the
schema lives in Lazuli core (`crates/lazuli_cli/src/lazurite_manifest.rs`).

Required sections: `[project]` (name + module + schema), `[lazuli]` (runtime
version pin). Optional: `[lazurite]`, `[plugins]`, `[generate.go]`,
`[frontends.*]`, `[migrations]`, `[seeds]`, `[dev]`.

**Boundary:** the manifest never duplicates declarations the DSL owns. App
environments, URLs, CORS, audiences, and deploy gates stay in `app.lzi` (and
`profiles.lzi` for overlays); audience scoping per frontend stays in `.lzx`.
The manifest is the *projection over the IR*, not a parallel source.

Doctor emits `MANIFEST-REQUIRED-001` when `.lzi` references `@plugin/*` but
the manifest is missing. Fixture suites used only for codegen testing
(no `@plugin/*` refs) may omit the manifest entirely.

See `docs/proposals/lazurite-scaffold.md` for the full schema and rationale.

## Generated

These are generated and should be treated as disposable:

- `dist/go/**` (`lazuli generate go --out dist/go`)
- `dist/ts-<frontend>/**` (per-frontend SDK from `[frontends.*]`)
- `.lazuli/graph.json`
- `.lazuli/source-map.json`
- `.lazuli/manifest.json`

**Convention:** `dist/` is the canonical output directory (web ecosystem
default — Vite, esbuild, tsc, etc.). `.lazuli/` is reserved for internal
**cache** and **manifests** (graph snapshots, source maps, extension file
registry) — never user-facing generated code.

Generated files in `dist/` are regen-only; do not hand-edit. They may be
committed (for vendored/deploy builds) or gitignored (default for `lazuli
new` scaffold); either way, they are not source of truth.

Generated applications are not bundled into one source file. Lazuli emits small entrypoints plus feature-local files grouped by category. Go later links many `.go` files into one binary, and React/TypeScript later bundles many modules for the browser. Lazuli keeps the generated source split so stack traces, diffs, source maps, and future granular regeneration point back to the owning feature.

Canonical output granularity is:

- one generated package/folder per feature;
- one file per cohesive category inside that feature, such as `types.go`, `queries.go`, `commands.go`, `workflows.go`, `events.go`, `policies.go`, `rules.go`, `jobs.go`, and `webhooks.go`;
- one React/TypeScript feature folder with `types.ts`, `api.ts`, and view components such as `List.tsx`, `Detail.tsx`, or named form/panel components;
- tiny application entrypoints that only wire runtime, routing, and feature registration from `app.lzi`.

Avoid both extremes: do not generate one giant `server.go`/`App.tsx` for the whole project, and do not create one file per individual command/query/view unless a target adapter has a concrete reason. The default is feature plus category.

`.lazuli/manifest.json` is derived from the capsule. It should include custom implementation files from both reusable `extensions` blocks and inline declarations such as job `handler`, webhook `verify`, resource `validates resource`/`validates field`, and inline view `block ... at`.

## Conventions

Default extension paths (Lazurite-canonical):

- `@fn.<name>` (custom function): `app/features/<feature>/handlers/<name>.go`
- `@validator.<name>` (custom validator): `app/features/<feature>/handlers/<name>.go` or `app/features/<feature>/domain/validate_<resource>.go` for resource-local validators
- `@hook.<name>` (workflow lifecycle hook): `app/features/<feature>/handlers/<name>.go`
- domain function extensions: `app/features/<feature>/domain/<name>.go`
- integration adapter extensions: `app/features/<feature>/integrations/<name>.go`
- query modifier extensions: `app/features/<feature>/queries/<name>.go`
- SQL query files (referenced via `query.sql @file.<name>`): `app/features/<feature>/queries/<name>.sql`
- background job handler: `app/features/<feature>/jobs/<name>.go`
- integration/webhook verifier or handler: `app/features/<feature>/integrations/<name>.go`
- email/notification template: `app/features/<feature>/templates/<name>.<locale>.tmpl`
- feature-local i18n catalog: `app/features/<feature>/i18n/<name>.<locale>.json`
- client cell implementation: `app/features/<feature>/<target>/cells/<name>.tsx`
- concrete view implementation: `app/features/<feature>/<target>/views/<audience>/<view>.tsx`

Filenames inside `handlers/`, `domain/`, etc. must match the DSL reference
name (`@fn.verify_password` → `handlers/verify_password.go` with
`func VerifyPassword(...)`). Doctor enforces this resolution rule.

Use `at` in `.lzi` only when a file intentionally lives outside convention.

Feature-local files still belong to the feature that owns the capability, even when they extend another feature's UI. For example, `customer_tags` can extend `@anchor.customer_detail`, but its `tag_editor` implementation remains under `app/features/customer_tags/web/cells/tag_editor.tsx`.

## Experience Sources

`.lzi` is the domain/capability contract. It owns resources, policies,
commands, workflows, jobs, webhooks, events, security contracts, and extension
contracts. It does not need a surface file to compile.

`.lzx` is the abstract experience/view model. It imports one or more `.lzi`
features and declares product-level views, actions, anchors, and exposure
intent without choosing a concrete platform widget.

`.web.lzx` and `.mobile.lzx` are protected compound suffixes for platform
projections. They use an
abstract experience and declare how each audience is rendered on that platform.
Product axes such as `audience admin` or `tenant acme` live in the file body.
Additional physical splits such as `customer.public.web.lzx` are organization
only; the protected platform segment remains immediately before `.lzx`, and the
header remains the semantic truth.

Dependency direction is fixed:

```txt
customer.lzi        # no UI dependency
customer.lzx        # imports customer
customer.web.lzx    # uses experience customer
customer.mobile.lzx # uses experience customer
```

Concrete surface variants use whole-view redeclarations. Do not use cascade
operators such as `columns += score`; redeclare the complete view for the
audience/tenant combination.
