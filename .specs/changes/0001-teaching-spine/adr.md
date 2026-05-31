---
id: 0001
title: Teaching Spine — lazuli_way idioms canon + scaffold + DoD gate
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Teaching is a release gate, taught via one-file-per-idiom under docs/lazuli_way/

## Context
- A shipped feature with no teaching surface = an unused feature (Pauta: 0/84 CRUD adoption). The maker's explicit rule: "é esforço inútil colocar recursos legais na linguagem sendo que os agentes não estão sabendo como fazer."
- 16 downstream specs each must add one idiom's teaching. If they all edit a single `lazuli_way.md`, parallel worktrees collide on merge.
- The scaffold templates (`CLAUDE.md.tmpl`/`AGENTS.md.tmpl`) are the first thing an agent reads in a new project — the highest-leverage teaching slot. They currently omit idioms entirely and contain a `@fn` clause ("derived field computation, multi-step orchestration") broad enough to license the SQL-in-Go escape that the hostpoint audit flagged as an invisible escape hatch.

## Decision
- **Teaching is the 4th DoD gate.** A feature is not done until `docs/lazuli_way/<idiom>.md` + the scaffold bullet exist. Codified in `docs/lazuli_way/definition-of-done.md`, embedded verbatim in every feature spec's techspec Gate.
- **One file per idiom.** `docs/lazuli_way.md` is a thin index; each idiom lives in `docs/lazuli_way/<slug>.md`. Each downstream spec CREATES its own file → no shared-file collision. The index links all slugs up front with stubs.
- **Each idiom doc has a fixed shape:** *idiom → before/after (real pilot excerpt) → the doctor rule that enforces it*. This binds teaching to enforcement so docs can't drift from the linter.
- **Seed example over prose.** `lazuli new` ships a working feature using the idioms. Prevention (a copyable example) beats a rule the agent must remember.
- This spec writes only the two already-shippable idiom docs (CRUD-by-convention, escape-hatch decision tree) + all stubs; future specs fill their stub.

## Alternatives considered
- **One monolithic `lazuli_way.md` everyone edits** — rejected: guaranteed merge conflicts across 16 parallel worktrees; the file becomes a serialization bottleneck, killing the fan-out.
- **Teach in `docs/quickstart.md`** — rejected: quickstart is the CLI getting-started path; idioms are a separate, larger concern and would bloat it. quickstart gets one link, no more.
- **Teaching as a follow-up wave after all features land** — rejected: that is exactly how Pauta drifted. Teaching deferred is teaching never done. Make it a gate.
- **Generate lazuli_way from the keyword registry** — rejected (for now): the value is in before/after pilot examples and decision trees, which aren't derivable from the registry. Revisit if docs drift from facets.

## Consequences
**We accept:** a per-idiom file sprawl under `docs/lazuli_way/` (~10 small files), and the index must list a slug before its content exists (stub). Every feature spec carries doc + scaffold edits it cannot skip — slightly heavier specs, deliberately.
**We gain:** zero teaching-surface collisions across parallel specs; teaching can't be silently skipped (it's a gate); new apps start idiomatic by example; the @fn loophole that hid SQL-in-Go is closed at the source agents read first.
**We watch:** if idiom docs start contradicting the doctor rules they cite, switch to generating the enforcement half from `facets.rs`.
