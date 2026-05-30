---
name: fold-view-accepted-by-into-allows-extension
applies_to: .lzx
match: |
  ${indent:ws}accepted by ${feature}
replace: |
  ${indent}allows extension ${feature}
description: SPEC-08 — view extensibility tests fold into the authored allows/denies dialect; the typed `extension <feature>` subject carries the dimension. See docs/design-decisions.md §11.
---

# fold-view-accepted-by-into-allows-extension

SPEC-08 collapsed the `.lzx` view-extensibility test verb into the same
authored `allows`/`denies` vocabulary every other authored test uses. The
extensibility dimension is now carried by the typed `extension <feature>`
subject, not a bespoke verb:

- legacy: `accepted by <feature>`
- canonical: `allows extension <feature>`

This is the same "the keyword carried no information the subject didn't"
collapse as the reverted bare `query` (design-decisions §6) and the
unified `validates` (design-decisions §10).

This recipe is **safe to apply mechanically**: `accepted by` only ever
appears as a view-test assertion, so the full-line-anchored match cannot
collide with any other construct.

## Verification

```sh
lazuli migrate dsl --from 0.15 --to 0.16 --dry-run --path .
```

The dry-run prints a diff per affected file. After review:

```sh
lazuli migrate dsl --from 0.15 --to 0.16 --path .
```

Each file is re-parsed via `lazuli_syntax::parse_feature_skeletons` after
the rewrite; a parse failure rolls the file back and surfaces the error.
The retired spelling also hard-errors in the parser
(`E-TEST-ACCEPTED-BY-RETIRED`), so an un-migrated tree fails loudly.
