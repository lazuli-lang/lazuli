# .specs — Lazuli language→pilot→teaching loop

Goal: close the loop so every shipped language feature is **implemented, migrated into the pilots, taught in `lazuli_way`, and enforced by a doctor rule**. Teaching + enforcement are release gates, not follow-ups. DoD: `docs/lazuli_way/definition-of-done.md`.

Evidence base: pilot audit 2026-05-31 (hostpoint + pauta-web).

## Execution state (2026-06-01)
- Framework specs land on branch **`loop-serial`** (local `main` fast-forwarded to match). Pilot edits land on `spec/<id>-*` branches in each pilot repo. Nothing pushed to origin (multi-swarm shared remote — push is a user decision).
- Isolated worktree for serial execution: `c:\tmp\lazuli-loop`. The shared `c:\Users\lucas\lazuli` checkout is hostile (concurrent swarms flip HEAD + sweep untracked files).
- Mandatory gate per framework spec: **`cargo test --workspace`** (full sweep), not per-crate — a latent break (0017's unregistered diagnostic) slipped past per-crate testing.

## Definition of Done (every feature spec)
1. **Build** — implemented + `cargo test --workspace` green.
2. **Migrate** — pilots that needed it are on it.
3. **Teach** — `docs/lazuli_way/<slug>.md` filled + scaffold CLAUDE.md/AGENTS.md bullet.
4. **Enforce** — a doctor rule fires on the old hand-rolled shape, or the scaffold seed demonstrates it.

## Status

| id | spec | status | where |
|----|------|--------|-------|
| 0001 | Teaching Spine (keystone) | ✅ DONE | loop-serial |
| 0002 | crud inverse linter `VOCAB-CRUD-SYNTH-AVAILABLE-001` | ✅ DONE | loop-serial |
| 0003 | Pauta crud migration | ⛔ BLOCKED-BY-DESIGN → needs 0018 | — |
| 0004 | defaults hoist (`rate_limit`/`audit`) | ✅ DONE | loop-serial + both pilots |
| 0005 | `access:` field shorthand | ▫ ready | — |
| 0006 | `doctor:allow` highlighting | ✅ DONE | loop-serial |
| 0007 | comment/allow doctor rules | ✅ DONE | loop-serial |
| 0008 | `LZI-FEATURE-COHESION-002` + file-size re-key | ✅ DONE | loop-serial |
| 0009 | split hostpoint god-files | ▫ ready | — |
| 0010 | escape-hatch visibility rules (live: 31 hits) | ✅ DONE | loop-serial |
| 0011 | fix unread_count cross-tenant (HIGH) | ✅ DONE | pauta spec/0011 |
| 0012 | fix shared linker bugs | ✅ DONE | loop-serial + pilots |
| 0013 | actor-relative query.compose | ▫ ready | — |
| 0014 | referential guards (`restrict on_delete`) | ✅ DONE | loop-serial |
| 0015 | soft_delete → deleted_by | ✅ DONE | loop-serial + pauta |
| 0016 | first-class Money (`VOCAB-MONEY-SHAPE-001`) | ✅ DONE | loop-serial |
| 0017 | state{} enum + transition | ✅ DONE | loop-serial |
| 0018 | **crud synth overlay** (policy/validate/assign/emits on `[crud]`) | ▫ ready (NEW) | — |

**Done: 13/18.** Remaining: 0005, 0009, 0013 (independent), 0018 (unblocks 0003), 0003 (after 0018).

## The 0003 → 0018 reframe (the loop working as designed)
0003 (migrate Pauta's 84 hand-rolled CRUD commands onto `conventions [crud]`) is BLOCKED-BY-DESIGN, *proven* not assumed: `VOCAB-CRUD-SYNTH-AVAILABLE-001` fires **0×** on Pauta even after 0004+0015, because Pauta's commands carry per-resource `policy` + `validate @validator.*` + default-literal/rename `assign`s + `emits` + curated `input` that the bare synth can't reproduce. Forcing it would silently change Pauta's API contract. So the gap is a **language requirement, not pilot debt** → spec **0018** grows the synth with an opt-in `crud` overlay block; then 0003 migrates Pauta for real. User directive: "corrija a linguagem e em seguida corrija no pauta."

## Dependency-resolved order for what remains
```
0018 ── 0003          (grow synth → migrate Pauta CRUD)
0005                  (independent, pauta-only)
0009                  (independent, hostpoint god-file splits; 0008 also flagged payments.lzi)
0013                  (independent, hostpoint; migrates the 11 list_* handlers 0010 flags live)
```

## Deliberately cut / deferred
- PT-BR scalars → `@plugin/scalars-pt-BR`, not core.
- String→struct `rate_limit` → deferred; 0004 did the hoist axis only.
- `utf8_safe` field-default → observation only.
- Cascading soft-delete → upstream open question; 0015 shipped the column, not the cascade.

## Archived
_(none yet — branch not merged to origin)_
