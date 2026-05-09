# Lazuli Project Structure

Feature folders are the source of truth. Generated output is disposable.

```txt
features/
  customer/
    customer.lzi
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
- tiny application entrypoints that only wire runtime, routing, and feature registration.

Avoid both extremes: do not generate one giant `server.go`/`App.tsx` for the whole project, and do not create one file per individual command/query/view unless a target adapter has a concrete reason. The default is feature plus category.

`.lazuli/manifest.json` is derived from the capsule. It should include custom implementation files from both reusable `extensions` blocks and inline declarations such as job `handler`, webhook `verify`, resource `validate`/`validates`, and inline view `block ... at`.

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
