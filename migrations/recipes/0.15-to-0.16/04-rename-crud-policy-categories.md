---
name: rename-crud-policy-category-create-to-author
applies_to: .lzi
match: |
  ${indent:ws}@policy.create${rest:rest}
replace: |
  ${indent}@policy.author${rest}
description: SPEC-07 C — a `policies` category must not shadow a command effect verb. Renames `@policy.create` references to the semantic `@policy.author`. SCOPE-GUARDED — this recipe covers ONE of four CRUD→semantic mappings and the reference side only; see the table below and migrate category declarations + the other three verbs the same way. See docs/invariants.md §Policies and docs/design-decisions.md.
---

# rename-crud-policy-categories

SPEC-07 C kills the CRUD/effect collision at source: a `policies` block
category must not be named after a command effect verb. At a `policy
@policy.create` reference site the name reads as a write *effect*, not an
*authorization category* — the very near-collision the `@policy.` prefix used
to paper over. The fix is a semantic rename; `@policy.` then earns its keep
purely as the SPEC-04 named-reference marker.

Doctor rule `POLICY-CATEGORY-SHADOWS-EFFECT-001` flags the offending category
**declarations** (warning under `tdd-strict` during the migration window; error
under `tdd-iron-hand` + production).

## The four mappings

| forbidden (CRUD/effect) | canonical (authorization intent) |
|---|---|
| `create` / `creates` | `author` |
| `read` / `reads` | `view` |
| `update` / `updates` | `edit` |
| `delete` / `deletes` | `remove` |

Each mapping touches **two** positions:

1. **Category declaration** — the `<name>:` line at a `policies` block's
   direct-child indent: `create: @role.admin` → `author: @role.admin`.
   (The per-field `read:` / `write:` directions under a `fields <Resource>`
   sub-block are access directions, NOT categories — leave them.)
2. **Reference** — every `policy @policy.<crud>` / `@policy.<crud>` site
   across commands, queries, api, jobs, webhooks, escape routes, lifecycle
   transitions, view/route guards, and `policy_for` defaults.

This recipe file automates mapping #1's **reference** side for `create`→
`author` (the unambiguous `@policy.create` token). Apply the analogous
reference rename for `read`/`update`/`delete`, and rename the category
**declarations** with the scope-guarded workflow below.

## ⚠️ SCOPE-GUARD — declarations need block context

The v0.1 single-line match DSL cannot scope a bare `create:` line to a
`policies` block (a resource field or enum variant could share the spelling),
so the declaration rename is a manual-review step. Two safety nets keep a
migration from shipping half-done:

1. **Parse + dangling-ref check.** A renamed declaration with un-renamed
   references (or vice-versa) leaves `@policy.create` pointing at no category;
   `lazuli check` reports the unresolved reference, so a half-migrated file
   fails loudly rather than silently.
2. **Doctor backstop.** `POLICY-CATEGORY-SHADOWS-EFFECT-001` fires on any
   surviving CRUD-named declaration, so nothing ships with the collision.

### Recommended workflow

```sh
lazuli migrate dsl --from 0.15 --to 0.16 --dry-run --path features/<f>/<f>.lzi
lazuli doctor features/<f>            # confirm POLICY-CATEGORY-SHADOWS-EFFECT-001 is clear
```

Read the dry-run as a checklist: apply the `@policy.<crud>` reference hunks,
rename the matching category declarations in the `policies` block, and re-run
`lazuli doctor` until the rule is silent.
