# Lazuli Project Structure

Feature folders are the source of truth. Generated output is disposable.

```txt
app.lzi
registry.lzi
profiles.lzi

features/
  customer/
    customer.lzi
    customer.lzx
    customer.web.lzx
    customer.mobile.lzx
    customer.ctx.md
    ui/
      status_cell.tsx
      activity_timeline.tsx
    hooks/
      before_create.go
    domain/
      risk_score.go
    queries/
      lifetime_value.sql
    jobs/
      recompute_scores.go
    integrations/
      stripe.go
    pages/
      customer_imports.tsx

.lazuli/
  generated/
    go/
      cmd/
        lazuli/
          main.go
      internal/
        runtime/
        types/
      features/
        customer/
          types.go
          queries.go
          commands.go
          workflows.go
          events.go
          policies.go
          rules.go
          jobs.go
          webhooks.go
    react/
      src/
        App.tsx
        runtime/
        features/
          customer/
            api.ts
            types.ts
            List.tsx
            Detail.tsx
    types/
  graph.json
  source-map.json
  manifest.json
```

## Source

These are authored and committed:

- `features/**/<feature>.lzi`
- `app.lzi`
- `registry.lzi`
- `profiles.lzi` or `profiles/**/*.lzi`
- `features/**/<feature>.lzx`
- `features/**/<feature>.web.lzx`
- `features/**/<feature>.mobile.lzx`
- `features/**/<feature>.ctx.md`
- extension code under `ui/`, `hooks/`, `domain/`, `queries/`, `jobs/`, `integrations/`, `pages/`
- adapter configuration

## Generated

These are generated and should be treated as disposable:

- `.lazuli/generated/**`
- `.lazuli/graph.json`
- `.lazuli/source-map.json`
- `.lazuli/manifest.json`

Generated files may be committed or ignored depending on target adapter, but they are not source of truth.

Generated applications are not bundled into one source file. Lazuli emits small entrypoints plus feature-local files grouped by category. Go later links many `.go` files into one binary, and React/TypeScript later bundles many modules for the browser. Lazuli keeps the generated source split so stack traces, diffs, source maps, and future granular regeneration point back to the owning feature.

Canonical output granularity is:

- one generated package/folder per feature;
- one file per cohesive category inside that feature, such as `types.go`, `queries.go`, `commands.go`, `workflows.go`, `events.go`, `policies.go`, `rules.go`, `jobs.go`, and `webhooks.go`;
- one React/TypeScript feature folder with `types.ts`, `api.ts`, and view components such as `List.tsx`, `Detail.tsx`, or named form/panel components;
- tiny application entrypoints that only wire runtime, routing, and feature registration from `app.lzi`.

Avoid both extremes: do not generate one giant `server.go`/`App.tsx` for the whole project, and do not create one file per individual command/query/view unless a target adapter has a concrete reason. The default is feature plus category.

`.lazuli/manifest.json` is derived from the capsule. It should include custom implementation files from both reusable `extensions` blocks and inline declarations such as job `handler`, webhook `verify`, resource `validates resource`/`validates field`, and inline view `block ... at`.

## Conventions

Default extension paths:

- client UI: `features/<feature>/ui/<name>.tsx`
- hook/validator extensions: `features/<feature>/hooks/<name>.go`
- resource-local validator: `features/<feature>/domain/validate_<resource>.go`
- domain function extensions: `features/<feature>/domain/<name>.go`
- integration adapter extensions: `features/<feature>/integrations/<name>.go`
- query modifier extensions: `features/<feature>/queries/<name>.go`
- SQL query files: `features/<feature>/queries/<name>.sql`
- background job handler: `features/<feature>/jobs/<name>.go`
- integration/webhook verifier or handler: `features/<feature>/integrations/<name>.go`
- inline view block: `features/<feature>/ui/<name>.tsx`
- escape route/page: `features/<feature>/pages/<name>.tsx`

Use `at` in `.lzi` only when a file intentionally lives outside convention.

Feature-local files still belong to the feature that owns the capability, even when they extend another feature's UI. For example, `customer_tags` can extend `@anchor.customer_detail`, but its `tag_editor` implementation remains under `features/customer_tags/ui/tag_editor.tsx`.

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
