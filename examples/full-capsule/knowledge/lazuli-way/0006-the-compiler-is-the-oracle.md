---
title:   "The compiler is the oracle — never guess syntax"
slug:    the-compiler-is-the-oracle
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, workflow, closed-forms]
read_when: "the parser rejects something and you are tempted to work around it"
---

# The compiler is the oracle — never guess syntax

Lazuli's surface is a **closed vocabulary**: finite keywords, each with finite
children and a closed value catalog. That closure is what lets an LLM author and
cold-read `.lzi` without docs — but only if you respect it instead of improvising
past it.

> **When the compiler rejects a form, the form is wrong — not the compiler.**
> Fix to the current grammar. Never wrap, work around, or invent a keyword to
> bridge a gap.

## The three sensors (in order of authority)

1. **`lazuli check <path>`** — parser + analyzer + invariants. Ground truth for
   "does this syntax exist". A rejected form is not in the language today, full
   stop.
2. **`lazuli inspect <feature> --expand=all`** — what the compiler actually
   *derived* (references, scopes, security envelope, event flow, context). Use
   when "I declared X but it doesn't behave like X".
3. **`lazuli doctor .`** — strict-profile audit + coverage. The lint, and the
   pass/fail sensor for the implement→validate loop.

`docs/keyword-reference.md` is generated from the keyword registry, so it cannot
drift from the parser. Prefer it over memory.

## A rejected form is a signal, not an obstacle

Exactly three honest responses — none is "force it through":

1. **Retired or mistyped spelling** → fix to the current form
   ([retired-forms](0004-retired-forms-and-replacements.md)).
2. **App-specific logic the grammar deliberately omits** → use one of
   [the five escape hatches](0002-five-escape-hatches.md), not a new keyword.
3. **The primitive genuinely doesn't exist and should** → that's a *gap*. Log it
   in `knowledge/gaps/` and (if you own the framework) file a proposal. Grammar
   grows only via proposal + "many faces" parity work, never by an app bending
   the language to fit.

Inventing a keyword, abusing a sibling construct because it parses, or encoding
real structure inside a string literal are the same mistake: they defeat the
closed-vocabulary property the language is built on.

Authoritative spec: `docs/design-principles.md` (Rule Zero — "Vocabulary Over
Mechanism"), `docs/scope-discipline.md`, the `lazuli-architect` skill.
