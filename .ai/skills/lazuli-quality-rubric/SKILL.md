---
name: Lazuli Quality Rubric
description: Scoring rubric for Lazuli as an AI-first DSL. Used by the quality gate to grade the language at a point in time and decide pass / pass-with-notes / block.
---

# Lazuli Quality Rubric

This is the rubric that turns the `lazuli-grade` pipeline from
hand-waving into a number. Use it when the pipeline asks you to grade
the language.

The rubric is biased: Lazuli's purpose is **AI-first authoring +
human cold-readability**. Criteria that don't serve those goals are
absent on purpose.

## How to grade

1. Cold-read the canonical fixture: `examples/full-capsule/`. Don't
   load `docs/canonical-semantics.md` first. The rubric measures
   whether the source explains itself.
2. For each criterion below, give a 1-10 score with a one-sentence
   justification anchored to a `path:line`.
3. Compute the weighted average. The weights bias toward AI-first.
4. Apply the gate rule (bottom of this doc).

## Criteria

| # | Criterion | Weight | What you're measuring |
|---|---|---|---|
| 1 | Legibility (cold human read) | 12% | Can a senior dev read 1000+ lines of fixture top-to-bottom without backtracking or doc-lookup? |
| 2 | Semantic density for LLM | 18% | Are `@policy`, `@cap`, `@semantic`, `@actor`, `@pii`, `@key`, `@llm`, `@tool` namespaces tight, closed, and unambiguous? |
| 3 | Token efficiency | 10% | Is there gordura recorrente? Count tokens of repeated boilerplate × number of repetitions. |
| 4 | Escape hatches | 8% | Can authors drop to `handler "./..."`, `validates resource "./..."`, custom Go without polluting source? Are the hatches minimal and visible? |
| 5 | Determinism (one way to say each thing) | 10% | If the same intent has two surface forms with no rule for choosing, that's a deduction. |
| 6 | Composability | 8% | Do `extends @anchor.*`, `extensible_by`, `packs`, `has_many`, `event_group` combine cleanly? |
| 7 | Multi-target fit (Go/React/Expo) | 8% | Are surface projections (`.web.lzx` / `.mobile.lzx`) clean? Does any contract leak transport mechanics? |
| 8 | Operational coverage | 6% | Do `runtime`, `deploy`, `profiles`, `services`, `architecture` cover real production needs without becoming Kubernetes config? |
| 9 | Declarative testability | 6% | Are `tests` blocks expressive enough for rules / transitions / anchors / commands without becoming a mock framework? |
| 10 | AI-first readiness | 14% | Does the language treat LLMs as first-class consumers (`agent`, namespaces, inspect contracts, doctor messages)? |

Sum of weights = 100%.

## Scoring scale

- **9.5–10** — exemplary. Better than current best-in-class.
- **8.5–9.4** — publishable. Real product can ship on this.
- **7.5–8.4** — usable but with clear friction. Not yet AI-first by
  Lazuli's own bar.
- **6.5–7.4** — needs structural work before adoption.
- **<6.5** — design problem, not polish.

## Anchoring discipline

Every score must include:

- One `path:line` reference for the strongest evidence.
- One `path:line` reference for the weakest spot in this dimension.

If you can't anchor, you can't grade. Re-read the fixture.

## Quality gate decision

Compute the weighted average → that's the **score**. Then apply:

- Score ≥ 8.5 **and** no criterion below 7 → **PASS** (ship as-is).
- Score ≥ 8.5 **but** at least one criterion below 7 →
  **PASS with notes** (ship, but log the weak criterion as a tracked
  cut in `docs/next-checklist.md`).
- Score < 8.5 **or** any criterion below 6 → **BLOCK** (do not
  publish; resolve the weak criterion first).

Boundary violations always block, regardless of score:

- Provider-specific names in core syntax (Stripe, AWS, MercadoPago,
  Kubernetes).
- `container.lzi` being introduced before registry pressure justifies it.
- `workspace.lzi` becoming mandatory.
- Magic discovery without inspect/doctor/LSP visibility.

## Output shape

When the rubric is done, emit:

```markdown
## Score: <weighted average>/10 — <PASS | PASS with notes | BLOCK>

| # | Criterion | Score | Best evidence | Weakest spot |
|---|---|---|---|---|
| 1 | Legibility | 9.0 | path:line | path:line |
| ... | ... | ... | ... | ... |

### Top atritos
- ... (cite path:line)

### Top faltas
- ... (cite path:line)

### Tracked cuts (if PASS with notes)
- ... (suggested addition to docs/next-checklist.md)
```

Don't editorialize. The rubric is the editorial.
