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

## Upgrade Recipes (`lazuli upgrade`)

Where the planner reasons about a *single project's* schema evolution, **upgrade
recipes** carry a *framework version bump* across every pilot — they migrate
authored `.lzi`/`.lzx` off a retired spelling when the language itself changes a
keyword, sigil, or catalog value.

Recipes live at `migrations/recipes/<from>-to-<to>/<slug>/recipe.toml`:

```toml
[recipe]
from_version = "0.14.0"
to_version   = "0.15.0"
kind         = "rewrite"        # additive | rename | rewrite
summary      = "@semantic.X / @cap.X -> bare PascalCase type"

# rename + rewrite recipes carry an ordered list of literal find->replace rules,
# applied to every .lzi/.lzx under the target (generated/internal trees skipped).
[[rule]]
find    = "@semantic."
replace = ""
[[rule]]
find    = "@cap."
replace = ""
```

The three kinds:

- **`additive`** — records intent; inserts new declarations. No in-place edit.
- **`rename`** — a keyword/sigil/catalog token swap (e.g. `public contract` ->
  `public_contract`).
- **`rewrite`** — a structural find->replace (e.g. dropping a type sigil).

`rename` and `rewrite` share one text-rule engine; they differ only in authoring
intent. Each is **meaning-preserving by construction**: a recipe may ship a
sibling `input.lzi` + `output.lzi` fixture, and the runner's IR-equality smoke
(`lazuli inspect --format=json` on both) **fails the recipe** if the two do not
lower to identical IR — so a recipe can never silently change behaviour. Run with
`--check` to report pending migrations without writing.
