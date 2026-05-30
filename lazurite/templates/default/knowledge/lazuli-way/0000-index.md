---
title:   "lazuli-way — index & fast path"
slug:    index
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, index]
read_when: "starting any lazuli-way task — read this first, then open the ONE doc you need"
---

# lazuli-way — index & fast path

Read this file first. It carries the highest-frequency decisions inline (the
**Fast path**) and routes you to the **one** doc to open for everything else — so
you load this index + 1 doc, not all 15.

## Fast path

The six gaffes that cost the most cold-read time. Most decisions need no further read:

- **Three operators, fixed by construct:** declare with `:`, mutate (command effect) with `=`, compare (filter / predicate) with `==`.
- **Policy categories are `author / view / edit / remove`** — never CRUD verbs (`create`/`read`/`update`/`delete`).
- **Security opt-outs carry a `reason "..."` child** (indented under the opt-out line, never inline): `rate_limit none` / `verify none` — then `reason "<why>"`.
- **Webhooks:** `verify hmac sha256` (+ `secret`/`header` children) or `verify none` + `reason` — never an unverified silent default, never the retired path form.
- **`workflow` is retired → `lifecycle`** (resource) + `triggers transition` (command). Never `workflow`.
- **When the parser rejects a form, the form is wrong** — fix the source, never work around it. The compiler is the oracle.

## Which doc

| doc | read when… |
|---|---|
| 0001-wire-not-reimplement.md | writing an authored Go handler/adapter — before adding any logic |
| 0002-five-escape-hatches.md | a typed surface cannot express something — before any workaround |
| 0003-the-three-operators.md | writing any assignment, declaration, or filter (`:` vs `=` vs `==`) |
| 0004-retired-forms-and-replacements.md | a form you remember gets rejected — is it retired? |
| 0005-justified-opt-outs.md | skipping a `rate_limit`, `verify`, or tenant scope |
| 0006-the-compiler-is-the-oracle.md | the parser rejects something and you are tempted to work around it |
| 0007-command-and-query-anatomy.md | writing a command or a query |
| 0008-lifecycle-not-workflow.md | modeling a state machine / status field (NEVER `workflow`) |
| 0009-project-wiring.md | writing app.lzi / registry.lzi / profiles / integrations / @runtime vs @plugin |
| 0010-surfaces-and-experiences.md | writing .lzx — UI, views, surfaces, web/mobile, anchors, extends |
| 0011-resources-and-fields.md | declaring a resource or field — types, @cap/@pii, tenancy, relations |
| 0012-events-and-event-groups.md | emitting or reacting to events |
| 0013-errors-and-i18n.md | errors block, error codes, translations / i18n |
| 0014-forcing-defaults-and-hygiene.md | finishing a feature — will it pass doctor? (policy names, audit, .ctx.md, non_goals, tests) |
| 0015-jobs-and-pollers.md | writing a job or a poller (async work) |

Ground every fact in `lazuli check`/`inspect`/`doctor` — the docs are the map, the compiler is the territory.
