---
title:   "Retired forms and their replacements"
slug:    retired-forms-and-replacements
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, syntax, gaffe, retired]
---

# Retired forms and their replacements

The language evolves by **retiring** old spellings, not aliasing them. A retired
form is hard-rejected by the parser — there is no compatibility fallback. Agents
trained on older corpora reach for these constantly; this is the canonical
"don't write that, write this" table. When the parser rejects something, assume
it is retired (or never existed) and reach for the current form — never invent a
workaround.

| ❌ Retired / never-existed | ✅ Current form |
|---|---|
| `workflow <name> on R.field { ... }` (state machine) | a resource `lifecycle <field>` block + command `triggers transition <name>` — see [lifecycle-not-workflow](0008-lifecycle-not-workflow.md) (hard-errors `E-WORKFLOW-RETIRED`) |
| `creates X` with `field: expr` (colon) | `field = expr` (assignment) — see [the-three-operators](0003-the-three-operators.md) |
| `verify "./integrations/x.go"` (path verifier) | `verify hmac sha256` + `secret`/`header` children, **or** `verify none` + `reason` |
| `rate_limit none` *without* a `reason` child, or an unquoted spec | `rate_limit "60 per minute per user"`, **or** `rate_limit none` + `reason "..."` |
| `query.lookup` with a `key <expr> = params.x` child | a `filters` block: `customer.id == params.customer_id` |
| `defaults` with arbitrary children | `defaults` children are only `tenancy`, `timestamps`, `policy_for` |
| `non_goals` with *keyed* `name: "..."` entries at top level | either a flat list of bare quoted strings (`non_goals` / `  "Real-time chat"`), or partition them under a `delegated_to` / `out_of_scope` block |
| `attach_ctx "<path>"` directive | co-located `<feature>.ctx.md` (the parser hard-errors `E-ATTACH-CTX-RETIRED`) |
| feature-level `context "@..."` block | co-located `<feature>.ctx.md` (`E-CONTEXT-RETIRED`) |
| `accepted by` / `rejected by` (old test dialect) | `allows` / `denies extension` |

## Why retire instead of alias

An alias would let two spellings of the same idea coexist, and a cold-reading
agent could never be sure which is canonical. The framework's "many faces" rule
(`CLAUDE.md`) says a keyword is only shipped when the parser, IR, LSP, syntax
highlighting, docs, scaffold, and `examples/` **all** agree on exactly one
spelling. Retiring keeps that set of one. The cost — a hard parser error on the
old form — is the point: it surfaces the drift immediately instead of letting
stale syntax rot silently in a fixture.

## How to stay current

Do **not** trust memory for these. The freshest authority is, in order:

1. `lazuli check <file>` — the parser is the ground truth; if it rejects a form,
   that form is not in the language today.
2. `docs/keyword-reference.md` — generated from the keyword registry, so it can
   never drift from the parser.
3. A passing feature already in `app/features/` — copy its shape.

Authoritative spec: `docs/keyword-reference.md`, `docs/grammar.lzi.md`,
`CLAUDE.md` §"Language-surface parity".
