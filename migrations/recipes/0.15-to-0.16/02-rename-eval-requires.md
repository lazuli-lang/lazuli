---
name: rename-eval-requires-to-allows
applies_to: .lzi
match: |
  ${indent:ws}requires ${rest:rest}
replace: |
  ${indent}allows ${rest}
description: SPEC-08 — agent eval assertions adopt the authored allows/denies polarity (`requires <pred>` → `allows <pred>`). SCOPE-GUARDED — see the ⚠️ section; review the dry-run and reject non-eval hunks. See docs/design-decisions.md §11.
---

# rename-eval-requires-to-allows

SPEC-08 folded agent eval polarity into the authored `allows`/`denies`
dialect; the eval predicate subject names the dimension:

- legacy: `requires <predicate>`  (inside `agent … evals … case`)
- canonical: `allows <predicate>`

## ⚠️ SCOPE-GUARD — this recipe over-matches in the v0.1 DSL

The single-line match DSL (`migrations/recipes` v0.1) cannot scope a match
to the `evals` sub-block, and `requires` is overloaded across THREE
constructs:

| construct | example | migrate? |
|---|---|---|
| agent eval assertion | `requires output contains "active"` | **YES** → `allows …` |
| feature dependency | `requires integration crm: CRMProvider` | **NO** — leave it |
| command precondition | `requires @policy.delete` | **NO** — leave it |

This recipe matches **all three**. Two safety nets keep a blind apply from
corrupting a file:

1. **Parse-rollback.** After the rewrite, `apply` re-parses the file via
   `lazuli_syntax::parse_feature_skeletons`. `allows integration …` and
   `allows @policy.…` are not valid in their positions, so a file that
   mixes eval `requires` with header/precondition `requires` **rolls back
   whole** and reports the error — it will not silently corrupt.
2. **Parser hard-error.** Un-migrated eval `requires` hard-errors as
   `E-EVAL-REQUIRES-RETIRED`, so nothing ships half-folded.

### Recommended workflow (manual-review step)

```sh
lazuli migrate dsl --from 0.15 --to 0.16 --dry-run --path features/<f>/<f>.lzi
```

Read the dry-run diff as a checklist. Apply the eval-block hunks (and
ONLY those) — either by hand, or by running without `--dry-run` against a
file you have confirmed contains eval `requires` and NO
`requires integration` / `requires @policy` lines. Reject every
feature-header / precondition hunk.
