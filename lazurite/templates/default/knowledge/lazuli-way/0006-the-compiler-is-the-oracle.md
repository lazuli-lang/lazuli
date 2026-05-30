---
title:   "The compiler is the oracle — never guess syntax"
slug:    the-compiler-is-the-oracle
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, workflow, closed-forms]
---

# The compiler is the oracle — never guess syntax

Lazuli's surface is a **closed vocabulary**: a finite set of keywords, each with
a finite set of children and a closed catalog of values. This is the property
that lets an LLM author and cold-read `.lzi` without external docs — but only if
the LLM *respects* the closure instead of improvising past it.

The single rule that prevents almost every gaffe:

> **When the compiler rejects a form, the form is wrong — not the compiler.**
> Fix to the current grammar. Never wrap, never work around, never invent a
> keyword to bridge a gap.

## The three sensors (in order of authority)

1. **`lazuli check <path>`** — parser + analyzer + invariants. Ground truth for
   "does this syntax exist". If it rejects a form, that form is not in the
   language today, full stop.
2. **`lazuli inspect <feature> --expand=all`** — what the compiler actually
   *derived* (references, scopes, security envelope, event flow, context). Use
   when "I declared X but it doesn't behave like X".
3. **`lazuli doctor .`** — the strict-profile audit + coverage. The lint, and the
   pass/fail sensor for the implement→validate loop.

`docs/keyword-reference.md` is generated from the keyword registry, so it is the
one written authority that *cannot* drift from the parser. Prefer it over memory.

## A rejected form is a signal, not an obstacle

When `lazuli check` rejects something, there are exactly three honest responses —
none of which is "force it through":

1. **You used a retired or mistyped spelling.** → Fix to the current form
   ([retired-forms](0004-retired-forms-and-replacements.md)).
2. **You need app-specific logic the grammar deliberately doesn't model.** →
   Reach for one of [the five escape hatches](0002-five-escape-hatches.md), not a
   new keyword.
3. **The primitive genuinely doesn't exist and should.** → That is a *gap*. Log
   it in `knowledge/gaps/` and (if you own the framework) file a proposal. The
   grammar grows only through a proposal + the "many faces" parity work — never
   by an app bending the language to fit.

Inventing a keyword, abusing a sibling construct because it happens to parse, or
encoding real structure inside a string literal are all the same mistake: they
defeat the closed-vocabulary property the whole language is built on.

Authoritative spec: `docs/design-principles.md` (Rule Zero — "Vocabulary Over
Mechanism"), `docs/scope-discipline.md`, the `lazuli-architect` skill.
