---
id: 0029
title: Comment-discipline — canonical-channel policy + LZI-COMMENT-PROSE-001 + codemod
type: prd
stage: 2 of 3
status: ready
created: 2026-06-02
---

# PRD — Comment-discipline for `.lzi`/`.lzx`

## Problem
Agents (including this LLM) flood `.lzi`/`.lzx` with prose `#` comments — rationale, intent narration, section banners — even though Lazuli already has non-polluting channels for every one of those: `<feature>.ctx.md` context files for rationale, structured fields (`purpose "..."`, resource/field `doc`, `description`) for a construct's intent, and (after 0028) `@doctor.allow(...)` for waivers. The result is low-signal design surfaces that drift from the structured truth and that an LLM re-reads as noise. Existing `LZI-COMMENT-NOISE-001` (0007) catches only the extreme cases (decorative dividers + comment-dominant files); ordinary prose-comment-per-construct sails through. There is no written policy telling an agent WHERE each kind of text belongs, so the behavior never stops.

## Why now (or why ever)
This is the ROOT problem the user reported; 0028 only built the waiver home. If never built: the LLM keeps emitting prose into `.lzi`/`.lzx` forever (no rule fires, no doc says don't), the design surfaces keep rotting, and the structured channels stay underused. The cost is paid every time an agent reads a noisy feature file and every time a human reviews one.

## Outcome — done means
- A written CANONICAL-CHANNEL POLICY (decision table) in `docs/lazuli_way/comment-hygiene.md`: rationale → `.ctx.md`; construct intent → its `purpose`/`doc`/`description` field; waiver → `@doctor.allow` (0028); what (if anything) a bare `#` is still for.
- A new doctor rule `LZI-COMMENT-PROSE-001` (category LziHygiene) that flags PROSE `#` comments in `.lzi`/`.lzx` — WARNING by default, ERROR under iron-hand — with precise carve-outs decided below.
- A `lazuli fix --rule LZI-COMMENT-PROSE-001` codemod that, where mechanical, relocates a prose comment to the right channel (or removes a redundant one); where not mechanical, it leaves the comment and the rule reports it.
- The scaffold CLAUDE.md/AGENTS.md guidance updated so agents stop doing it.
- `cargo test --workspace` green; hostpoint + pauta-web `lazuli generate go .` gate-pass + `go build` (the rule is WARNING by default → does not block the gate; verify zero unexpected ERROR-under-iron-hand surprises).

## Non-goals
- Touching the waiver mechanism — owned by 0028; here it is just a carve-out.
- Flagging comments in Rust/Go/TS or any non-`.lzi`/`.lzx` file. Scope is the two design surfaces only.
- A natural-language "is this prose" classifier. The rule uses simple, explainable heuristics (length / word-count / sentence punctuation / not-an-allow / not-a-known-marker), not an LLM.
- Auto-rewriting a comment's MEANING into a `purpose`/`doc` field (semantic lift) — the codemod relocates/removes only the mechanical cases; semantic moves are reported, not performed.
- Removing `LZI-COMMENT-NOISE-001` (0007) — it stays; this rule is the per-comment complement to its file-level heuristics.
- Making the rule ERROR by default. Default is WARNING; ERROR only under the iron-hand profile.

## User stories
- As the LLM, when I emit a prose `#` line, `LZI-COMMENT-PROSE-001` tells me which channel to use instead — and the scaffold guidance told me before I started.
- As the maker, I run `lazuli fix` and the mechanical noise (redundant/empty/banner comments) is cleaned without hand-editing.
- As a reviewer, a `.lzi` diff carries structured intent and waivers, not prose.

## Constraints
- WARNING default → must NOT block `lazuli generate go` for either pilot (the gate blocks only concrete-bug categories; LziHygiene is non-blocking — verify it stays out of the blocking set).
- Honors `@doctor.allow(LZI-COMMENT-PROSE-001, ...)` (0028) AND the legacy comment form during the window.
- Must be clean-or-quiet on the pilots' CURRENT `.lzi` after 0028's `lazuli fix` ran (no false-positive storm); tune carve-outs against the real pilots before commit.
- Cannot depend on `lazuli_cli` from `lazuli_doctor`.

## Open questions
None. Carve-outs decided in ADR (allow-node lines; a single optional file-header line; short structured-marker labels; everything else that reads as prose fires). Severity decided (WARNING default / ERROR iron-hand).
