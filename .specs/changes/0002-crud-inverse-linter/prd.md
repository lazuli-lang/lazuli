---
id: 0002
title: crud-inverse-linter
kind: prd
track: prove
depends_on: [0001]
parallel_safe: true
status: ready
created: 2026-05-31
---

# PRD — CRUD inverse linter (`VOCAB-CRUD-SYNTH-AVAILABLE-001`)

## Problem

`conventions [crud]` is fully shipped: a resource opts in and the analyzer
synthesizes `create_X`/`update_X`/`delete_X` + `read_X`/`list_X`, with
hand-written members overriding by name (see `crates/lazuli_analyzer/src/conventions/mod.rs`
`ConventionOrigin::{Synthesized,AuthorOverride}`). But adoption is invisible.
Today's doctor only **validates existing opt-ins** (`crud_synth_*`/`me_synth_*`
in `crates/lazuli_doctor/src/vocab/conventions.rs`): nothing tells an author that
a resource they hand-rolled the full CRUD surface on *could* drop the boilerplate
and opt in. That silence is why Pauta-web drifted to 84 hand-rolled CRUD commands
across 13 features with **zero** convention adoption (audit 2026-05-31).

## Who

- Lazuli authors writing/maintaining `resource` blocks (human or agent).
- The agent consuming `lazuli doctor --json`, which today gets no adoption signal.

## What (user-visible outcome)

A new advisory doctor diagnostic that runs the existing crud synth **backwards**:
when a resource hand-rolls a set of commands whose names match what
`conventions [crud]` would synthesize, doctor emits
`VOCAB-CRUD-SYNTH-AVAILABLE-001` (Warning, never gates) suggesting the author add
`conventions [crud]` and delete the matched boilerplate.

## Why now

- The synth pass + override-by-name machinery already exists in
  `crates/lazuli_analyzer/src/conventions/`. This spec is the inverse consumer
  that closes the adoption gap ADR 0002 (`docs/architecture-decisions/0002-resource-conventions-crud-me.md`) flagged.
- 0003 (Pauta migration) needs this linter to *find* the migration candidates and
  to prove, post-migration, that no further nudge fires on create/update.

## Success criteria

- Doctor flags a fully-hand-rolled CRUD resource and stays silent once it adopts.
- The rule is **advisory**: never fails a build (its facet base severity is
  `warning` and it is excluded from gating — see techspec).
- The rule respects `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001`
  (via the existing `allow_comment` suppression path).
- Partial hand-rolls (e.g. `create_X` + `update_X` only) still get a suggestion
  scoped to which members match — adoption is incremental.
- A resource whose delete is `soft_delete` is suggested for create/update adoption
  only, not delete (the synth delete is hard — see ADR).

## Non-goals

- Auto-fix / auto-rewrite of source. Suggestion only (machine-readable payload).
- Gating. This never blocks `lazuli check` or `lazuli doctor` exit codes.
- Touching the synth logic, the `me` convention, or the soft-delete story (0015).
- Suggesting adoption for resources that genuinely deviate (no name match).
- Name-normalization heuristics beyond the synth's own naming (`create_<r>` etc.).

## Risks

- False positives on resources that *look* CRUD-shaped but deviate in behavior
  (e.g. a `create_X` that does more than create). Mitigation: match on synth
  names only, emit Warning never Error, always suppressible via `# doctor:allow`.
- Noise on resources mid-migration. Mitigation: per-member scoping in the message
  + the create+update core requirement (read/list-only resources don't fire).
