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

## Conventions

Default extension paths:

- client UI: `features/<feature>/ui/<name>.tsx`
- server hook/validator: `features/<feature>/hooks/<name>.go`
- server domain function: `features/<feature>/domain/<name>.go`
- raw SQL: `features/<feature>/queries/<name>.sql`
- background job: `features/<feature>/jobs/<name>.go`
- integration/webhook adapter: `features/<feature>/integrations/<name>.go`
- escape route/page: `features/<feature>/pages/<name>.tsx`

Use `at` in `.lzi` only when a file intentionally lives outside convention.

Feature-local files still belong to the feature that owns the capability, even when they extend another feature's UI. For example, `customer_tags` can extend `customer.surface.web.admin.view.detail`, but its `tag_editor` implementation remains under `features/customer_tags/ui/tag_editor.tsx`.
