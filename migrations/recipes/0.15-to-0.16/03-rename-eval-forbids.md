---
name: rename-eval-forbids-to-denies
applies_to: .lzi
match: |
  ${indent:ws}forbids ${rest:rest}
replace: |
  ${indent}denies ${rest}
description: SPEC-08 — agent eval `forbids <pred>` → `denies <pred>`, resolving the eval/matrix `forbids` collision. SCOPE-GUARDED — see the ⚠️ section; reject generated actor-matrix `forbids @role` hunks. See docs/design-decisions.md §11.
---

# rename-eval-forbids-to-denies

SPEC-08 folded eval `forbids` into authored `denies`, which also resolves
the cross-semantic `forbids` collision: after this fold `forbids` means
ONLY the generated command actor-matrix row, never an eval verb.

- legacy: `forbids <predicate>`  (inside `agent … evals … case`)
- canonical: `denies <predicate>`

## ⚠️ SCOPE-GUARD — this recipe over-matches in the v0.1 DSL

`forbids` is used by TWO constructs and the single-line DSL cannot tell
them apart:

| construct | example | migrate? |
|---|---|---|
| agent eval assertion | `forbids output contains @semantic.Email` | **YES** → `denies …` |
| generated actor-matrix row | `forbids @role.viewer` | **NO** — leave it (it is machine-derived from `policy @policy.*` and is hand-written only as a smell) |

This recipe matches **both**. The generated matrix row is the one
distinction SPEC-08 keeps (`permits`/`forbids` = "this row is generated,
do not hand-edit") — rewriting it to `denies` would erase that signal.

Safety nets: the post-rewrite re-parse rolls a file back if the result is
invalid, and un-migrated eval `forbids` hard-errors as
`E-EVAL-FORBIDS-RETIRED`.

### Recommended workflow (manual-review step)

```sh
lazuli migrate dsl --from 0.15 --to 0.16 --dry-run --path features/<f>/<f>.lzi
```

Apply only the eval-block hunks. Reject every `forbids @role.…`
actor-matrix hunk (RHS begins with `@`). When in doubt, hand-edit the
eval `case` lines and leave the matrix rows untouched.
