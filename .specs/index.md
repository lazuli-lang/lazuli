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

---

## Plugin Platform (specs 0019-0023) — added 2026-06-01 · ✅ ALL 5 DONE (origin/main a39c6622)

A second spec stack on the same `.specs/` system. Root cause: the PT-BR scalar failure (hostpoint can't `go build`: `@semantic.BrazilianPhone outside the closed Go semantic table`) is ONE symptom of 5 architectural plugin seams (survey in memory `project_plugin_platform_2026-06-01`). Goal: an elegant Plugin API+SDK that fixes the root for ALL plugin kinds. Same DoD gate (FULL `cargo test --workspace` + pilot build + teach + enforce).

**STATUS 2026-06-01: all 5 SHIPPED + pushed to origin/main.** 0019 (725ef845) unblocked hostpoint go-build (BR scalars 13→0). 0021 (51545422) typed manifest, 24/24 real manifests deserialize. 0020 (029c67ba) doctor↔generate agree (hostpoint BR scalars: doctor flagged 18→0). 0023 (60b4469d) `lazuli plugin new`, scaffolds pass `go test` green. 0022 (a39c6622) `lazuli plugin verify` (5-link chain) + PLUGIN-CONTRACT-001; live hostpoint = 6/8 PASS, 2 legit FAIL (smtp legacy interface + missing env, sms-twilio missing env). Full sweep 147 suites / 0 failures throughout.

| id | spec | seam | track | parallel | depends_on |
|----|------|------|-------|----------|------------|
| 0019 | plugin-resolution-unify (single resolver pipeline + loud failures + upward root) | 1+3 | ship | ✅ | — |  ✅DONE
| 0020 | plugin-authoritative-resolver (doctor shares codegen's resolver/root) | 4 | prove/ship | ❌ shares stage | 0019 |
| 0021 | plugin-typed-manifest (kind discriminant + adapter [env]/[binds]/implements schema) | 2 | evolve/ship | ✅ | 0019 |
| 0022 | plugin-verify-contract (`lazuli plugin verify` + `PLUGIN-CONTRACT-001` adapter check) | 4+2 | prove/ship | ❌ | 0020, 0021 |
| 0023 | plugin-scaffolder (`lazuli plugin new <name> --kind`) | 5 | ship/tell | ✅ | 0021 |

**0019 is the foundation** — smallest, highest-value, unblocks hostpoint `go build`, kills the silent-failure class. Ships first.

### Dependency graph
```
0019 ─┬─ 0020 ─┐
      ├─ 0021 ─┼─ 0022
      │        └─ 0023
```
**Dispatch order:** 0019 first (serial, it's the seam everyone shares). Then 0021 (parallel-safe, pure schema) alongside 0020 (shares the resolution stage with 0019 → serial after it). Then 0022 (needs both 0020+0021) and 0023 (needs 0021) — 0023 parallel-safe.

---

## DX / authoring-hygiene + portability (specs 0028-0030) — added 2026-06-02

Three connected DX problems: (A) doctor waivers are un-structured `#` comments; (B) agents flood `.lzi`/`.lzx` with prose comments though structured channels exist; (C) generated `go.mod`/`go.work` bakes absolute disk paths (pauta BT-01) → non-portable. Same DoD gate: FULL `cargo test --workspace` + pilot `lazuli generate go .` gate-pass + `go build` + teach (`docs/lazuli_way`) + enforce (doctor rule/test). Keyword changes also require parser↔registry parity (`cargo test -p lazuli_keywords`) + xtask keyword-reference freshness; new doctor codes require the diagnostics-registry bridge + module_headers trigger-cue.

| id | spec | problem | track | parallel | depends_on |
|----|------|---------|-------|----------|------------|
| 0028 | first-class `@doctor.allow` waiver node (FROZEN `Module.doctor_allows` seam) | A | ship/seam | ✅ | — |
| 0029 | comment-discipline: channel policy + `LZI-COMMENT-PROSE-001` + codemod | B | enforce/tell | ❌ builds on 0028's IR seam | 0028 |
| 0030 | portable runtime wiring (relative `[lazuli] path` + env fallback + `RUNTIME-WIRING-ABSOLUTE-PATH-001`) | C | ship/enforce | ✅ | — |

**Key citations grounding these specs:** waiver scan `crates/lazuli_doctor/src/allow_comment.rs` (~30 consumers); parser trivia `crates/lazuli_syntax/src/parser/common.rs:53`; existing hygiene rule `crates/lazuli_doctor/src/lzi_hygiene/lzi_comment_noise.rs`; gate categories `crates/lazuli_cli/src/doctor/gate.rs:110` (LziHygiene non-blocking, Correctness blocks); go.mod/go.work emit `crates/lazuli_codegen_go/src/emitter/module/go_mod.rs:144,182`; runtime-path resolver `crates/lazuli_cli/src/lazurite_codegen.rs:68-179`; env override `crates/lazuli_cli/src/commands/new/runtime_wiring.rs:35`; pilot evidence pauta `go.mod:17`+`go.work:11` (absolute `C:/Users/lucas/lazuli/runtime/go`), hostpoint `go.work:21` (relative).

### Dependency graph
```
0028 ── 0029        (waiver node seam → comment-discipline lint builds on it)
0030                (independent, parallel-safe — portability)
```
**Dispatch order:** 0028 and 0030 in parallel immediately (both `parallel_safe`, no shared surface — 0028 touches keywords/parser/IR/doctor-waivers, 0030 touches codegen-go/cli-resolver/doctor-correctness). 0029 waits on 0028's frozen `Module.doctor_allows` seam (`parallel_safe: false` — it consumes the IR contract + the same `lzi_hygiene`/`comment-hygiene.md` surfaces 0028 edits).

### Locked shared contract (0028, frozen for 0029)
`lazuli_ir::DoctorAllow { code, reason: Option<String>, scope: DoctorAllowScope (File | Construct{line}), legacy: bool, span }` on `Module.doctor_allows`. 0029's `LZI-COMMENT-PROSE-001` reads this slice to exempt waiver lines. Do not change the shape after 0028 lands.

### Cross-spec findings baked in (from the survey + spec authors)
- Doctor ALREADY calls the same `build_alias_map` as codegen — the real divergence is the project-ROOT (`doctor_project_root` doesn't walk up). So 0020 re-homes `find_project_root` into `lazuli_manifest` (doctor can't dep on `lazuli_cli`).
- All 24 `c:\Users\lucas\dev\lazuli-plugin-*` EXCEPT scalars-br are ADAPTERS (the big unmodeled category). 0021 models adapter fully, stubs capability/design (Pareto). `kind` is INFERRED for back-compat (no existing manifest declares it).
- Static verification limit (honest): Rust verifies the declared contract + wiring graph; the Go `var _ Interface = (*Adapter)(nil)` assertion under `go build` is the method-set complement.
- The CLI `Commands` enum is FLAT today; 0023 adds the first nested subcommand (`lazuli plugin new|verify`); 0022+0023 coordinate the `PluginCommand` enum (first-to-land defines it).
