---
description: Run the Lazuli quality gate — grade against the AI-first rubric and emit pass / pass-with-notes / block.
argument-hint: "[scope: fixture-only | full-repo | panel]"
allowed-tools: Task, Read, Grep, Glob
---

# /lazuli-grade — Lazuli Quality Gate

Mirror of `.orion/pipelines/lazuli-grade/pipeline.toml`. Orchestrates the
DAG locally by dispatching the `lazuli-language-architect` subagent per
stage.

## Resolve scope

`scope` controls how much material the audit reads:

- `fixture-only` (default) — cold-read `examples/full-capsule/` only.
- `full-repo` — also cross-check against `crates/`, `editors/`, every
  example.
- `panel` — second-opinion via `orion__consult_panel` (requires the
  orion MCP server connected to this session).

Resolve `$ARGUMENTS`: trim, lowercase, accept exactly one of the three.
If empty or unrecognized, use `fixture-only` and tell the user the
default was applied.

## DAG

Stages run in this dependency order. Independent stages with the same
`depends_on` may dispatch in parallel — issue their `Task` calls in a
single message.

Every dispatch uses `subagent_type: lazuli-language-architect`. Prefix
each stage prompt with one line: `Pipeline scope: <resolved>.`

After every stage returns, surface a one-line status update to the user
naming the stage and whether it produced output.

### Stage 1 — orient (no deps)

```
Cold-orient on the fixture as if you've never seen it.

1. List `examples/full-capsule/` files.
2. Read top-to-bottom, in order: `app.lzi`, `registry.lzi`,
   `workspace.lzi`, `profiles.lzi`, `full-capsule.lzi`,
   `full-capsule.lzx`, then the platform projections and the
   contract.
3. Don't load `docs/canonical-semantics.md` first. The rubric measures
   whether the source explains itself without doc-lookup.

Output: a single paragraph stating whether the cold-read was painful,
neutral, or pleasant — anchored to one specific moment that surprised
you (good or bad), with `path:line`.

This is the mental state the rubric will grade. Don't grade yet.
```

### Stage 2 — grade-criteria (depends: orient)

```
Grade the language against all 10 rubric criteria.

For each criterion (1 through 10 in the Lazuli Quality Rubric skill):
- Score 1-10 with a one-sentence justification.
- Cite ONE `path:line` for the strongest evidence.
- Cite ONE `path:line` for the weakest spot.

Walk them in order. Don't merge. Don't average yet.

If `scope == full-repo`, additionally cross-check each criterion against:
- `crates/lazuli_lsp/src/lib.rs` (diagnostics breadth and message quality)
- `crates/lazuli_cli/src/doctor.rs` (cross-checks)
- `editors/vscode/syntaxes/lazuli.tmLanguage.json` (highlighting coverage)

Output: the 10-row markdown table from the rubric skill's "Output shape
(grade)" section. No final score yet.
```

### Stage 3 — panel-consult (depends: grade-criteria; SKIP unless scope=panel)

If `scope != panel`, skip this stage entirely; do not dispatch.

```
Dispatch one `orion__consult_panel` call with two panelists. Both
receive the same source extract (a representative slice of the fixture,
~300 lines) and the rubric, and they each grade independently. Per
panelist:

{
  provider: "claude" | "codex",
  prompt: "Grade this DSL fixture against the attached rubric. Score
           each of the 10 criteria 1-10 with one-sentence justification
           anchored to a line number. Output the table only.",
  system_prompt: "You are a senior DSL/language architect. Be terse,
                  push back on weak premises."
}

Compare the two panelist tables to your own grade-criteria output.
Where panelists disagree with you by ≥2 points, re-read the cited
location and decide whether to revise.

Output: a 3-column table (Criterion | Your score | Panelist deltas)
plus 2-3 sentences on which (if any) of your scores moved.

If the orion MCP server is not available in the current session,
report that and stop — do not fabricate panelist output.
```

### Stage 4 — identify-friction (depends: grade-criteria)

May dispatch in parallel with stages 3 and 5.

```
List the top 3-5 concrete atritos (friction points) you found while
grading. Each one is a place a senior dev or LLM would stumble.

Format per finding:
- `path:line` anchor.
- 1-sentence description.
- Which rubric criterion it pulled down.
- Cost class: cosmetic | mechanical | structural.

These are the candidates for immediate cuts if the gate verdict is
"PASS with notes."
```

### Stage 5 — identify-missing (depends: grade-criteria)

May dispatch in parallel with stages 3 and 4.

```
List the top 3-5 missing constructs / primitives the fixture would
benefit from.

Cross-reference with `docs/next-checklist.md` — anything already
tracked or done is excluded.

Format per finding:
- Where in the fixture it would be used (`path:line`).
- The 1-line contract you'd give it.
- Whether it's Lazuli-shaped or actually Drusa/adapter (per the
  `lazuli-language-boundaries` rule).

If a candidate is Drusa/adapter-shaped, drop it from the list — it
doesn't belong in Lazuli core.
```

### Stage 6 — quality-gate-decision (depends: grade-criteria, identify-friction, identify-missing; also panel-consult if it ran)

```
Compute the weighted average from `grade-criteria` (use the weights in
the rubric skill — they sum to 100%).

Apply the gate rule from the rubric skill:
- Score ≥ 8.5 AND no criterion below 7 → PASS
- Score ≥ 8.5 AND at least one criterion below 7 → PASS with notes
- Score < 8.5 OR any criterion below 6 → BLOCK

Boundary violations always block, regardless of score.

Output the full report from the rubric skill's "Output shape (grade)"
section:

## Score: <weighted average>/10 — <PASS | PASS with notes | BLOCK>

| # | Criterion | Score | Best evidence | Weakest spot |
|---|---|---|---|---|
...

### Top atritos
- ... (from identify-friction)

### Top faltas
- ... (from identify-missing)

### Tracked cuts (if PASS with notes)
- ... (suggested rows for docs/next-checklist.md)

Stop. The user decides what to do with the verdict.

Do NOT edit `docs/next-checklist.md` yourself. Paste the suggested
rows in chat for human review.
```

## Final reply

Surface the stage-6 report as the slash command's output. Do not
editorialize, summarize, or compress it. The rubric is the editorial.
