---
id: 0017
title: state{} enum bound to transition — lifecycle status as a typed lattice
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
track: evolve/ship
test_gate: "cargo test -p lazuli_syntax state_enum && cargo test -p lazuli_codegen_go state_transition && lazuli check + doctor + go build clean in BOTH pilots"
agent: unassigned
---

# TechSpec — closed state{} enum bound to transition

## Approach
Elevate the existing inline `state X initial`/`transition` vocabulary into a first-class, named, closed `state { ... }` declaration that `transition` blocks bind to. Reuse the existing `state`/`transition` keywords (`s10.rs:89-90`) and the existing lifecycle lowering — do NOT rebuild runtime codegen. Add closed-type membership checking (generalizing the existing `LIFECYCLE-TRANSITION-*-UNDECLARED` rules) plus a new rule for the comment-only "enum-by-command" shape. Migrate the 7 lifecycle features. Critically: must not regress the synthesized-enum false-positives the traveler waivers document (respect `synth_origins`).

## Surface
**Modify (language):**
- `crates/lazuli_keywords/src/registry/sections/s10.rs:89-90` — extend `state`/`transition` so `state` heads a closed `state { ... }` block (named, referenceable) and `transition` binds to it; completion/hover copy.
- `crates/lazuli_keywords/src/registry/sections/s06.rs:74` — `lifecycle` block can host/reference a declared `state {}`.
- `crates/lazuli_syntax/src/parser/lzi/...` (lifecycle/resource body) — parse the closed `state { member initial / member / member terminal }` declaration + `transition`'s reference to it.
- `crates/lazuli_syntax/src/ast/` — AST: named closed state set (members + initial/terminal flags) + transition→state-set binding.
- `crates/lazuli_ir/src/nodes/` — IR: closed state lattice as a typed node; transitions carry resolved member refs.
- `crates/lazuli_codegen_go/src/emitter/` (lifecycle path) + `crates/lazuli_analyzer/src/lifecycle/` — bind the declared `state {}` into the existing lifecycle lowering WITHOUT double-emitting the discriminator enum (consult `synth_origins`, per the traveler-waiver root cause).

**Create / Modify (enforcement):**
- `crates/lazuli_doctor/src/lifecycle/state_set_undeclared_001.rs` — `LIFECYCLE-STATE-SET-UNDECLARED-001`: fires on a `transition`/lifecycle machine with NO declared closed `state {}` (the comment-only "enum-by-command" shape).
- Generalize the existing `transition_to_undeclared.rs` (`LIFECYCLE-TRANSITION-TO-UNDECLARED`) + `transition_from_undeclared.rs` to resolve `from`/`to` against the declared closed `state {}` membership (a transition referencing a non-member fires).

**Modify (teaching):**
- `docs/lazuli_way/state-machines.md` — fill the stub (0001 idiom shape).
- `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — state-machine idiom bullet ("Reach for a closed `state {}` bound to `transition`, not enum-by-command comments").
- `docs/keyword-reference.md` / `docs/grammar.lzi.md` / `docs/lifecycle-transitions.md` — document `state {}`.

**Migrate (BOTH pilots — 7 lifecycle features):**
- pauta `job_steps_activities.lzi:60-82` — lift the inline `state pending/in_progress/completed` list into a closed `state {}`; bind the 7 transitions; drop the "no separate enum JobStepStatus" comment (`:60-65`).
- pauta `hoxo_financial_integration.lzi:28-64` — lift the prose lattice (`:31-35`) + inline states (`:62-64`) into a closed `state {}`; bind the 5 transitions; drop the "no separate enum" comment (`:29`).
- pauta `attachments.lzi:46` — declare a closed `state {}` for the upload-status discriminator; bind transitions/commands; drop "enum-by-command" comment.
- hostpoint `traveler.lzi` — bind the 6 transitions to a declared `state {}`; ensure the synth-origin waivers (`:2-3`) are no longer needed (or remain correctly suppressed).
- hostpoint `host.lzi` — bind the 4 transitions to a declared `state {}`.

## Contracts
- **Declaration**: `state <Name> { <member> initial | <member> | <member> terminal ... }` — a named closed set; exactly one `initial`; zero+ `terminal`. (Final surface fixed in BUILD; the contract is: named, closed, initial/terminal markers, referenceable.)
- **Binding**: a `transition`'s `from`/`to` MUST be members of the bound state set; non-members are a doctor finding via the generalized `LIFECYCLE-TRANSITION-{FROM,TO}-UNDECLARED`.
- **Comment-only shape**: a lifecycle/transition machine with no declared `state {}` → `LIFECYCLE-STATE-SET-UNDECLARED-001`.
- **No double-emit**: declared `state {}` lowers through the existing lifecycle synth; the `enum_duplicate.rs`/`field_double_declared.rs` detectors must skip synth-origin enums (the traveler-waiver root cause) — do not regress.
- **DoD block (embedded verbatim — see Gate).**
- **Idiom-doc shape (from 0001):** `# State machines` / `## Reach for this` / `## Before (hand-rolled) / After (idiomatic)` (cite `job_steps_activities.lzi:60-71` + `attachments.lzi:46` "enum-by-command" before; closed `state {}` after) / `## Enforced by LIFECYCLE-STATE-SET-UNDECLARED-001 + LIFECYCLE-TRANSITION-*-UNDECLARED`.

## Plan — for the executing agent
1. **BUILD-lang**: extend `state`/`transition`/`lifecycle` keywords + parser + AST for the closed `state {}` declaration + transition binding. Test `state_enum` in `lazuli_syntax`.
2. **BUILD-ir/codegen**: IR closed-lattice node; bind into existing lifecycle lowering without double-emitting (respect `synth_origins`). Test `state_transition` in `lazuli_codegen_go`.
3. **ENFORCE**: write `LIFECYCLE-STATE-SET-UNDECLARED-001` (comment-only shape) + tests; generalize `transition_to/from_undeclared` to closed-set membership; assert no regression of `LIFECYCLE-ENUM-DUPLICATE`/`-FIELD-DOUBLE-DECLARED` false-positive suppression.
4. **MIGRATE-pauta**: `job_steps_activities`, `hoxo_financial_integration`, `attachments` → declared `state {}`; drop the 3 "enum-by-command" comments; clean.
5. **MIGRATE-hostpoint**: `traveler`, `host` → declared `state {}`; verify the traveler synth-origin waivers are unneeded or correctly suppressed; clean.
6. **TEACH**: fill `docs/lazuli_way/state-machines.md`; add CLAUDE.md.tmpl + AGENTS.md.tmpl bullet; update keyword-reference / grammar / lifecycle-transitions docs.

## Tests first (TDD)
- [ ] `state_enum` (lazuli_syntax) — closed `state {}` parses; exactly-one-`initial` enforced; transition referencing a non-member fails resolution.
- [ ] `state_transition` (lazuli_codegen_go) — declared `state {}` lowers through lifecycle codegen; discriminator enum emitted ONCE (no double-emit).
- [ ] `lifecycle_state_set_undeclared_001` — fires on a transition machine with no declared `state {}` (enum-by-command); silent when `state {}` declared.
- [ ] `transition_membership` — generalized `LIFECYCLE-TRANSITION-{FROM,TO}-UNDECLARED` fires on a `from`/`to` that is not a member of the bound set.
- [ ] `synth_origin_no_regression` — `LIFECYCLE-ENUM-DUPLICATE`/`-FIELD-DOUBLE-DECLARED` stay silent for lifecycle-synth-origin enums/fields (traveler shape).

## Gate

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**4 concrete gates:**
1. `cargo test -p lazuli_syntax state_enum && cargo test -p lazuli_codegen_go state_transition` green.
2. `lazuli check && lazuli doctor && go build ./...` clean in BOTH hostpoint and pauta-web; the 7 lifecycle features bind transitions to a declared `state {}`; no enum double-emit.
3. `docs/lazuli_way/state-machines.md` filled per the 0001 shape; CLAUDE.md.tmpl + AGENTS.md.tmpl bullet present.
4. `LIFECYCLE-STATE-SET-UNDECLARED-001` fires on the pre-migration "enum-by-command" shape and is silent after; its code (+ the generalized `LIFECYCLE-TRANSITION-*-UNDECLARED`) named in the idiom doc.

## Risks & rollback
- **Double-emit / waiver regression** — binding `state {}` into the existing lifecycle synth re-trips `LIFECYCLE-ENUM-DUPLICATE`/`-FIELD-DOUBLE-DECLARED` (the traveler waivers) → mitigation: consult `synth_origins`; `synth_origin_no_regression` test is mandatory.
- **"enum-by-command" features have NO state list to lift** (`attachments`) → must author the closed set from the prose/command set → mitigation: derive members from the existing transitions + status discriminator; verify against handler behavior.
- **Runtime behavior change** — out of scope; lowering stays. If `state {}` changes the emitted discriminator, gate 2 `go build` + existing lifecycle tests catch it.
- **Rollback**: `git revert`. Language change is additive; pilot migration revert restores inline state lists + "enum-by-command" comments.
