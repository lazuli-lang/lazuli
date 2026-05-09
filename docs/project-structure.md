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
    react/
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
