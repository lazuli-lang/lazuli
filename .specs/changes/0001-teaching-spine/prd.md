---
id: 0001
title: Teaching Spine — lazuli_way idioms canon + scaffold + DoD gate
type: prd
stage: 1 of 17
status: ready
created: 2026-05-31
---

# PRD — Teaching Spine

## Problem
Lazuli ships powerful authoring idioms that agents never use because nothing teaches them. Proof: `conventions [crud]/[me]` is fully shipped (AST + IR + `crud_synth_*` diagnostics + inspect + 3 proposals), yet Pauta-web uses it **0×** across 84 hand-rolled create/update/delete commands in 13 features. Hostpoint uses it; Pauta (generated separately) never learned it. There is no `lazuli_way` artifact, `docs/quickstart.md` teaches only CLI verbs, and the scaffold `CLAUDE.md.tmpl`/`AGENTS.md.tmpl` teach gates + the 5 escape hatches but **zero authoring idioms**. Every new app re-drifts.

## Why now (or why ever)
Every later spec in this set adds a language feature. If the teaching surface doesn't exist and isn't locked, each feature ships invisible — wasted effort, exactly the failure that produced Pauta's 84-command drift. This spec is the seam the other 16 write into; it must exist and be frozen before they run, or they collide on one file and teach nothing.

## Outcome — done means
1. `docs/lazuli_way.md` exists as an index linking one file per idiom under `docs/lazuli_way/`; each idiom file has a reserved stub (`<!-- filled by spec NNNN -->`) so parallel specs each own a distinct file, never the index.
2. `docs/lazuli_way/definition-of-done.md` defines the 4-gate DoD (build+test / pilot-migrate / teach / enforce) that every feature spec embeds.
3. Scaffold `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` gain an "Authoring idioms" section that links lazuli_way and lists each idiom with a one-line "reach for X, not the hand-rolled Y."
4. The `@fn` escape-hatch clause in the scaffold is rewritten to close the SQL-in-Go loophole (raw SQL belongs in `query.sql`/`query.compose`, declared and visible — never a multi-line SQL string buried in a `@fn` Go handler).
5. A seed example feature exists under `lazurite/templates/default/app/features/` using `conventions [crud]` + a `defaults` block, so `lazuli new` *demonstrates* the canon.
6. The stale language-backlog line ("`crud` … not canonical v0 sugar … remain explicit") is moved to "Closed v0 Decisions" with the resolution that `conventions [crud]/[me]` shipped.

## Non-goals
- Writing the idiom *content* for features not yet built (Waves 1–3 fill their own stub when they ship). This spec only creates stubs + cross-cutting content (the escape-hatch decision tree + CRUD-by-convention, both already shippable today).
- Touching `docs/quickstart.md` beyond adding a single link to lazuli_way (quickstart stays CLI-focused).
- Any language/grammar/codegen change. Docs + templates + one example feature only.
- Migrating pilots (that is spec 0003 and each feature spec's own migrate cell).

## User stories
- As an authoring agent on a fresh `lazuli new`, I read `CLAUDE.md` → "Authoring idioms" → `lazuli_way` and reach for `conventions [crud]` instead of hand-rolling CRUD.
- As a feature-spec executor (specs 0002–0017), I drop my idiom doc into a pre-reserved `docs/lazuli_way/<slug>.md` and tick the DoD gate without editing any file another spec also edits.

## Constraints
- Per-idiom files (not one monolith) so 16 parallel specs never git-conflict on the teaching surface.
- Seed feature must pass `lazuli check` + `lazuli doctor` clean (it is shipped in every new app).

## Open questions
None. All decisions made in the ADR.
