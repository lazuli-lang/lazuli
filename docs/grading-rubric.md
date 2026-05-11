# Lazuli Grading Rubric

**Status**: Normative reference. Used by `/lazuli-grade` and
`/lazuli-improve` slash commands, by the
`lazuli-language-architect` subagent, and by human reviewers
auditing proposals.

The rubric is biased on purpose: Lazuli's mission is **AI-first
authoring + human cold-readability**. Criteria that don't serve
those goals are absent. This document is the single source of
truth for the rubric. The agent definition at
`.claude/agents/lazuli-language-architect.md` and the slash
command at `.claude/commands/lazuli-grade.md` reference this file.

## How to use

This rubric exists to turn vibes into a number. Three classes of
user:

1. **Proposal authors** — predict your proposal's score before
   asking for a grade. The eight criteria in §Self-Assessment lay
   out what to check.
2. **Reviewers** (human or agent) — score the language, a
   proposal, or a fixture against §Criteria. Always anchor with
   `path:line`. Apply the gate at the end.
3. **`/lazuli-grade` orchestration** — the rubric drives the
   pipeline DAG; outputs follow §Output shape.

## Scope

The rubric grades:

- **The language as a whole** — typically against the canonical
  fixture `examples/full-capsule/` plus the docs that ship with
  it.
- **A proposal** — the proposed shape against the same criteria,
  scored on what the proposal would land if implemented.
- **A patch / cut** — a specific PR's effect on the criteria.
  Often only 3–4 criteria move; the others stay the same.

It does not grade:

- Runtime correctness (Lazuli Go / generated code).
- Performance.
- Test coverage of the *implementation*. The implementation
  pipeline is its own concern.

## Criteria

Sum of weights = 100%.

| # | Criterion | Weight | What you're measuring |
|---|---|---|---|
| 1 | Legibility (cold human read) | 12% | Can a senior dev read 1000+ lines of fixture top-to-bottom without backtracking or doc-lookup? |
| 2 | Semantic density for LLM | 18% | Are `@policy.*`, `@cap.*`, `@semantic.*`, `@actor.*`, `@pii.*`, `@key.*`, `@llm.*`, `@tool.*` namespaces tight, closed, and unambiguous? |
| 3 | Token efficiency | 10% | Is there gordura recorrente? Count tokens of repeated boilerplate × number of repetitions. |
| 4 | Escape hatches | 8% | Can authors drop to `handler "./..."`, `validates resource "./..."`, custom Go without polluting source? Are the hatches minimal and visible? |
| 5 | Determinism (one way to say each thing) | 10% | If the same intent has two surface forms with no rule for choosing, that's a deduction. |
| 6 | Composability | 8% | Do `extends @anchor.*`, `extensible_by`, `packs`, `has_many`, `event_group` combine cleanly? |
| 7 | Multi-target fit (Go/React/Expo) | 8% | Are surface projections (`.web.lzx` / `.mobile.lzx`) clean? Does any contract leak transport mechanics? |
| 8 | Operational coverage | 6% | Do `runtime`, `deploy`, `profiles`, `services`, `architecture` cover real production needs without becoming Kubernetes config? |
| 9 | Declarative testability | 6% | Are `tests` blocks expressive enough for rules / transitions / anchors / commands without becoming a mock framework? |
| 10 | AI-first readiness | 14% | Does the language treat LLMs as first-class consumers (`agent`, namespaces, inspect contracts, doctor messages)? |

## Scoring scale

| Score | Meaning |
|---|---|
| 9.5–10 | Exemplary. Better than current best-in-class DSLs. |
| 8.5–9.4 | Publishable. A real product can ship on this. |
| 7.5–8.4 | Usable but with clear friction. Not yet AI-first by Lazuli's own bar. |
| 6.5–7.4 | Needs structural work before adoption. |
| < 6.5 | Design problem, not polish. |

## Anchoring discipline

Every score must include:

- One `path:line` reference for the **strongest evidence**.
- One `path:line` reference for the **weakest spot**.

If you can't anchor, you can't grade. Re-read the source. A score
without anchors is rejected and the reviewer is asked to re-grade.

This rule is what keeps grades from collapsing into vibes. It also
catches inflation: if every criterion's "weakest spot" is in the
same file, that file is the audit finding.

## Quality gate

Compute the weighted average → that's the **score**. Then apply:

| Condition | Verdict |
|---|---|
| Score ≥ 8.5 **and** no criterion below 7 | **PASS** (ship as-is) |
| Score ≥ 8.5 **but** at least one criterion below 7 | **PASS with notes** (ship; log the weak criterion as a tracked cut in `docs/next-checklist.md`) |
| Score < 8.5 **or** any criterion below 6 | **BLOCK** (do not publish; resolve the weak criterion first) |

**Boundary violations always block, regardless of score.** A
boundary violation is any of:

- Provider-specific names in core syntax (Stripe, AWS, MercadoPago,
  Kubernetes, OpenAI, Anthropic). Provider names live behind
  `@runtime/...`, `@plugin/...`, `@adapter.<local>`.
- `container.lzi` being introduced before registry pressure
  justifies it.
- `workspace.lzi` becoming mandatory for single-app projects.
- Magic discovery without `lazuli inspect` / `lazuli doctor` /
  LSP visibility.
- Lazuli runtime mechanics (DI, broker plumbing, transport details)
  pushed into the language layer.

A boundary violation is a *deletion*, not a *deferral*. Reject in
line; do not log as a tracked cut.

## Output shape (grade)

```markdown
## Score: <weighted average>/10 — <PASS | PASS with notes | BLOCK>

| # | Criterion | Score | Best evidence | Weakest spot |
|---|---|---|---|---|
| 1 | Legibility | 9.0 | path:line | path:line |
| 2 | Semantic density | 9.2 | path:line | path:line |
| ... |
| 10 | AI-first readiness | 8.7 | path:line | path:line |

### Top atritos
- path:line — 1-line description — affects criterion N.

### Top faltas
- path:line — 1-line description — what it would unlock.

### Tracked cuts (if PASS with notes)
- Suggested rows for `docs/next-checklist.md`.
```

Don't editorialize the output. The rubric is the editorial.

## Self-assessment for proposal authors

Before asking the architect to grade your proposal, walk these
eight checks. If you can't answer all of them in the proposal's
first 100 lines, the proposal is not ready.

1. **What's the boundary?** Language (contracts), runtime (the
   Lazuli Go and TS libraries), adapters (providers). Where does
   each piece of this proposal live? Cite
   `docs/capability-layering.md`.
2. **What does the closed-namespace catalog do here?** If the
   proposal introduces a name, is it under an existing
   `@<namespace>.*`, or is it inventing one? Inventing one is a
   structural change that needs its own proposal.
3. **What's the canonical form?** If the proposal allows two
   surface shapes for the same intent, what's the rule for
   choosing? If the rule is "author preference," cut one form.
4. **What does doctor enforce?** Each new construct needs a
   diagnostic. Anchor with the diagnostic ID and severity.
5. **What does inspect surface?** If the IR shape changes, the
   inspect projection must also change. Specify which `--expand=...`
   class gains the field.
6. **What's the IR delta?** Additive minor, or structural major?
   The rubric reads `LZIR_SCHEMA` and `LZI_LANG` bumps as
   evidence of discipline. Bigger isn't worse, but unannounced
   bigger is.
7. **What is the promotion gate?** Per
   `docs/capability-layering.md` lifecycle: custom → pack →
   pack+doctor → language-light → core. What evidence does the
   proposal have that justifies its placement? "It feels right" is
   not evidence.
8. **What does it remove?** A proposal that only adds is suspect.
   Lazuli stays small by deleting more than it adds. If the
   proposal removes nothing, name what it makes irrelevant in
   user code.

Self-graded score formula (rough): start at 8.5; subtract 0.5 for
each unanswered question. If you arrive below 7.5, the proposal
isn't ready for the architect.

## Examples (anchored)

These are real grades from past architect reviews, kept for
calibration.

### Example 1 — `docs/proposals/ai-primitives-v0.md` (first pass)

> **Verdict**: BLOCK as one cut. Weighted score 7.6.
>
> Six primitives in one cut. `knowledge` violated promotion
> lifecycle. `budget cost per tenant per month` was runtime
> metering. Q1 (registry effect) and Q3 (flow entry) deferred but
> load-bearing. Architect recommended split into Cut A (tools,
> discriminator, evals — language) and Cut B (flow, budget,
> knowledge — pack/deferred). The score failed the "no axis below
> 7" gate on three axes (static-analysis surface, open questions,
> bloat).

Lesson: a 7.6 weighted with three sub-7 axes blocks. The fix is
not raising the weighted; it's raising the floors.

### Example 2 — `docs/proposals/ai-primitives-v0.md` (second pass)

> **Verdict**: APPROVE.
>
> After split into Cut A + Cut B and resolution of B1–B4. Cut A
> graded 8.8 (AI-first coverage), 9.2 (layer placement), 8.5
> (coherence), 9.0 (static-analysis surface). Three non-blocking
> nits applied inline.

Lesson: the gate rewards discipline. A 0.4-point increase on the
worst axis turned a BLOCK into an APPROVE.

### Example 3 — Cut A.5 (single-pass)

> **Verdict**: APPROVE WITH NOTES.
>
> Architect already endorsed the shape twice in prior reviews;
> sanity-check pass found one IR-naming drift (`ValidatorExt`
> didn't exist; should extend the existing `Extension` wrapper),
> one promotion-gate softness ("multi-class PII fan-in" is
> trivially satisfiable), and a missing migration design-decision
> entry. All non-blocking; fixed in 4 small edits.

Lesson: a small focused proposal can pass first time if it
extends the IR shape that an already-approved proposal commits
to. The architect's bar lowers when the prerequisite is solid.

## How the rubric is enforced

Three enforcement points:

1. **`/lazuli-grade` slash command** — runs the multi-stage DAG
   in `.claude/commands/lazuli-grade.md`. The architect subagent
   walks the criteria here, grades, and emits the gate verdict.
2. **`lazuli-language-architect` subagent** — used directly via
   the `Task` tool when a single-shot grade is needed. The
   subagent's instructions reference this file as the rubric of
   record.
3. **Human reviewers** — read this document directly when
   proposing a cut, reviewing a PR, or auditing the language.
   Anchor scores with `path:line`. Same gate applies.

## Versioning

The rubric is part of the language contract. Changes to the
weights, criteria, or gate rule are themselves
`/lazuli-grade`-graded changes:

- A weight shift (e.g., Token efficiency 10% → 8%) is a minor
  rubric bump.
- A criterion replacement (e.g., merging Composability into
  Multi-target fit) is a major rubric bump.
- The boundary-violation list is append-only without a major
  bump. Removing a line is major.

History of changes lives in `git log -- docs/grading-rubric.md`.

## Open questions

- **Should there be a 11th criterion for "migration / backward
  compat"?** Today the rubric weighs migration through Criterion 5
  (determinism — by ensuring there's one canonical form,
  migrations stay simple). Real product pressure may justify a
  dedicated axis. Defer until ≥ 3 cuts produce migration debt.
- **Should the AI-first weight grow?** Currently 14% (Criterion
  10) plus 18% on semantic density (Criterion 2) = 32% weighted on
  AI-first concerns. The thesis of Lazuli is AI-first, but
  pushing past 35% would flatten the human-cold-read criterion.
  Defer; revisit if real LLM-author tests show systemic gaps the
  rubric misses.
- **Should the rubric be per-construct?** Today it grades the
  whole language. Per-construct grading would let proposals
  target specific axes. Plausible after Cut A's IR migration
  delivers per-construct typed shapes.

## Reserved

- A separate **runtime grading rubric** for the Go runtime and
  codegen lives outside this document. The language rubric does
  not measure runtime correctness; that is the runtime team's
  discipline.
- An **eval-success rate** metric ("X% of LLM-authored fixtures
  parse and pass doctor on first try") would be the most direct
  AI-first measure. Reserved until evals against the LSP
  diagnostics produce a reliable corpus.
