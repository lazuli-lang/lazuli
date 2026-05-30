---
name: fold-view-rejected-by-into-denies-extension
applies_to: .lzx
match: |
  ${indent:ws}rejected by ${feature}
replace: |
  ${indent}denies extension ${feature}
description: SPEC-08 — symmetric fold of `rejected by`; `denies extension <feature>` preserves the existence-tolerant semantics the old verb had. See docs/design-decisions.md §11.
---

# fold-view-rejected-by-into-denies-extension

Symmetric counterpart of `00-fold-view-accepted-by`:

- legacy: `rejected by <feature>`
- canonical: `denies extension <feature>`

`denies extension` preserves the existence-tolerant semantics that
`rejected by` carried (a denied feature need not resolve to an existing
experience — it is a pre-commitment that the anchor must never accept it).

**Safe to apply mechanically**: `rejected by` only appears as a view-test
assertion, so the full-line-anchored match cannot collide with any other
construct.

## Verification

```sh
lazuli migrate dsl --from 0.15 --to 0.16 --dry-run --path .
```

After review, drop `--dry-run`. Each file is re-parsed after the rewrite;
a parse failure rolls the file back. The retired spelling hard-errors as
`E-TEST-REJECTED-BY-RETIRED`.
