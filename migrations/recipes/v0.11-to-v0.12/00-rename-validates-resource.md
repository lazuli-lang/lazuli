---
name: rename-validates-resource-keyword
applies_to: .lzi
match: |
  ${indent:ws}validates resource @validator.${ref}
replace: |
  ${indent}validates @validator.${ref}
description: Tier-4 follow-up retired the `resource` axis from `validates`; the scope is now encoded in the validator's `Validator[<scope>]` type. See docs/design-decisions.md §10.
---

# rename-validates-resource-keyword

Tier-4 (cut 14) unified the two surface forms of `validates`:

- legacy: `validates resource @validator.<name>` and `validates field <name> @validator.<name>`
- canonical: `validates @validator.<name>`

The scope is now encoded in the validator's extension type
(`Validator[Resource]` for whole-resource, `Validator[Resource.field]`
for field-scoped). The keyword duplication never carried information
the type didn't already.

This recipe rewrites the `resource` keyword case mechanically. The
`field <name>` case is not handled here because it carries the field
name as a positional argument; a follow-up recipe (`01-...`) can
address it once we agree on whether the field reference needs to
survive on a `@validator.<name>` annotation or whether the validator
type already pins it.

## Verification

```sh
lazuli migrate dsl --from v0.11 --to v0.12 --dry-run --path .
```

The dry-run prints a diff per affected file. After review:

```sh
lazuli migrate dsl --from v0.11 --to v0.12 --path .
```

Each file is re-parsed via `lazuli_syntax::parse_feature_skeletons` after the
rewrite; a parse failure rolls the file back and surfaces the error.
