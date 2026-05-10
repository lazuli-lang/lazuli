# Lazuli Migration Planning

Schema and semantic evolution should be planned, not guessed.

## Inputs

The planner compares:

- previous `.lzi` semantic IR
- current `.lzi` semantic IR
- `previously` identity hints
- generated manifest
- adapter capabilities

## Change Kinds

| Change | Default Risk |
|--------|--------------|
| Add optional field | low |
| Add required field without default | high |
| Add required field with default | medium |
| Remove field | high |
| Rename field with `previously` | medium |
| Rename field without `previously` | high |
| Add enum value | low |
| Remove enum value | high |
| Change tenancy | critical |
| Change soft delete | high |
| Change unique constraint | medium/high |
| Change SQL query return type | medium/high |

## Planner Output

`lazuli plan` should produce:

```txt
Feature: customer

Changes:
- Rename Customer.status -> Customer.lifecycle_status
- Add enum value CustomerStatus.paused

Risks:
- Field rename requires data migration
- Existing filters affected: customer.query.list.filters.status
- Existing views affected: customer.surface.web.admin.view.list

Required decisions:
- Confirm migration maps status -> lifecycle_status
```

## Source Of Truth

The planner must not infer renames from string similarity alone. Use `previously` when continuity matters:

```lazuli
lifecycle_status previously migrated status: CustomerStatus = lead
```

If no identity hint exists, planner should treat the change as remove + add.
