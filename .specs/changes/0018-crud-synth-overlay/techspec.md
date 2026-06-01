---
id: 0018
title: CRUD synth overlay — policy / validators / assignments / emits on conventions[crud]
type: techspec
track: evolve/ship
depends_on: [0001, 0002, 0004, 0015]
parallel_safe: false
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_syntax crud_overlay && cargo test -p lazuli_analyzer crud_overlay && cargo test -p lazuli_codegen_go crud_overlay && cargo test --workspace"
agent: unassigned
---

# TechSpec — CRUD synth overlay

## Approach
A `crud` child block on the resource carries per-effect overlays (`create`/`update`/`delete`), merged into the synthesized command during the conventions pass BEFORE lowering, so the analyzer emits the same `CommandEffect::{Creates,Updates,Deletes}` it produces for the equivalent hand-rolled command. No new IR command shape, no new emitter, no runtime change (RULE-VOCAB-03 preserved). Absent block = today's synth byte-identical. The acceptance oracle is `lazuli inspect` equivalence on Pauta's real `create_customer`/`update_customer`/`delete_customer` trio.

## Surface
**Create:**
- `crates/lazuli_syntax/src/parser/lzi/resource/crud_overlay.rs` — parse the `crud` block + `create`/`update`/`delete` sub-blocks + their clauses (`policy`, `validate`, `input excludes`, `assign`, `emits`).
- `crates/lazuli_syntax/tests/crud_overlay.rs` — parser tests.
- `crates/lazuli_analyzer/src/conventions/crud_overlay.rs` — merge an overlay into a synthesized command (the composition logic).
- `crates/lazuli_analyzer/src/conventions/crud_overlay_tests.rs` — merge + IR-equivalence unit tests (synth+overlay == hand-rolled IR for a fixture trio).
- `crates/lazuli_codegen_go/tests/crud_overlay.rs` — codegen golden: overlaid synth emits the same Go as the hand-rolled equivalent.

**Modify:**
- `crates/lazuli_keywords/src/registry/sections/*.rs` — register the new tokens (`crud` block header + `create`/`update`/`delete` sub-headers + `assign` + `excludes` if not already present). Reuse existing `policy`/`validate`/`emits`/`input`. Run `cargo run -p xtask -- gen-keyword-reference` after.
- `crates/lazuli_syntax/src/ast/resource_p1.rs` (or p2) — `ResourceDecl.crud_overlay: Option<CrudOverlayAst>`.
- `crates/lazuli_ir/src/nodes/resource/mod.rs` — IR carries the overlay (or it's consumed entirely in the analyzer pass and never reaches IR — preferred: overlay is analyzer-only, lowered away into the synthesized commands, so IR has NO new resource field. Decide in BUILD; analyzer-only is cleaner and avoids the ~110-literal ripple).
- `crates/lazuli_analyzer/src/conventions/crud.rs` — `build_create_command`/`build_update_command`/`build_delete_command` accept the merged overlay (policy override, validators, assignments, emits, input-exclude).
- `crates/lazuli_doctor/src/vocab/crud_synth_available.rs` — upgrade `VOCAB-CRUD-SYNTH-AVAILABLE-001` to match synth+overlay (fire when a hand-rolled set could adopt `[crud]`+overlay), per its existing detection mechanism.
- `docs/lazuli_way/crud-by-convention.md` — overlay is the production-CRUD idiom; bare `[crud]` is the trivial case. Cite the Pauta trio before/after.
- `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — one idiom bullet (byte-identical).
- `docs/keyword-reference.md` — regenerated via xtask (do not hand-edit).

## Contracts
**`crud` overlay block (resource-body child, after the `conventions [crud]` line):**
```
crud
  create
    policy @policy.<x>                  # overrides synth default policy
    validate @validator.<v>             # 0..n
    input excludes <field>, <field>     # drop system/derived fields from synth input
    assign <field> = <literal|input.x|ctx.x|enum-variant>   # 0..n, merged into the creates block
    emits <event>                       # 0..n
  update
    policy @policy.<x>
    validate @validator.<v>
    input excludes <field>, ...
    assign <field> = ...
    emits <event>
  delete
    policy @policy.<x>
    emits <event>
    # soft-delete-aware automatically when resource has `soft_delete` (spec 0015)
```
- Each sub-block is optional; each clause within is optional. Absent `crud` block = today's synth.
- `assign` values are the SAME RHS grammar the hand-rolled `creates`/`updates` assignment block already accepts (literal, `input.<f>`, `ctx.<f>`, enum variant) — reuse that parser.
- Merge semantics: overlay `policy` REPLACES synth default; `validate`/`emits`/`assign` ADD to the synth's generated effect; `input excludes` removes fields from the synth-generated input.

**IR-equivalence contract (the gate):** for the Pauta trio, `lazuli inspect <feature>.create_customer --expand=all` (and update/delete) of the synth+overlay output is byte-identical to the same projection of the hand-rolled command before migration. This is asserted both as a unit test (analyzer) and a codegen golden, and verified live on the pilot in step 9.

**RULE-VOCAB-03 invariant (must not break):** every overlaid command maps to exactly one existing `CommandEffect` shape; no new lowering, no new IR command node, no runtime change.

## Plan — for the executing agent
1. Read the existing synth: `crates/lazuli_analyzer/src/conventions/crud.rs` + `mod.rs` (how `build_*_command` constructs each `CommandEffect`), and the hand-rolled `creates` assignment-block parser in `lazuli_syntax` (to reuse the `assign` RHS grammar).
2. Register the new tokens in `lazuli_keywords` (parser↔registry parity: `cargo test -p lazuli_keywords` must pass). Regenerate `keyword-reference.md` via xtask. Register NO new diagnostic codes unless you add one (if you do, add to facets or the bridge test fails — spec 0017 lesson).
3. Parser + AST: parse the `crud` block into `CrudOverlayAst` on the resource. TDD: write `crates/lazuli_syntax/tests/crud_overlay.rs` first.
4. Analyzer merge: decide overlay is ANALYZER-ONLY (consumed in the conventions pass, never reaches IR as a resource field — avoids the struct-literal ripple). Implement `crud_overlay.rs` merging the overlay into `build_*_command`. TDD: `crud_overlay_tests.rs` asserts synth+overlay IR == hand-rolled IR for a `customer`-shaped fixture trio.
5. Codegen golden: `crates/lazuli_codegen_go/tests/crud_overlay.rs` — overlaid synth emits identical Go to the hand-rolled equivalent.
6. Upgrade `VOCAB-CRUD-SYNTH-AVAILABLE-001` to match synth+overlay; add a test that it fires on a `customer`-shaped hand-rolled-with-policy fixture (today it's silent).
7. TEACH: update `docs/lazuli_way/crud-by-convention.md` (overlay = production idiom) + scaffold bullet (byte-identical both tmpls).
8. Framework gate: the `test_gate` PLUS **`cargo test --workspace`** (full sweep — mandatory; a prior spec left a latent bridge/doc break only the full sweep caught) + `cargo build --workspace` + xtask freshness + `lazuli_diagnostics_registry` bridge.
9. LIVE PROOF (read-only, no pilot edit here — pilot migration is 0003): build the CLI, hand-author a `crud` overlay on Pauta's `Customer` resource in a SCRATCH COPY (or a throwaway branch you reset), regenerate, and diff the synth+overlay `create_customer`/`update_customer`/`delete_customer` IR against the committed hand-rolled versions — prove byte-identical. Report the diff (must be empty). Reset the scratch edit.
10. Commit framework on `loop-serial` (one clean commit). End message with the Co-Authored-By line.

## Tests first (TDD)
- [ ] `crud_overlay_parses` — the `crud` block + 3 sub-blocks + all clauses parse into `CrudOverlayAst`.
- [ ] `bare_crud_unchanged` — a resource with `conventions [crud]` and NO `crud` block synthesizes exactly today's IR (regression guard for Hostpoint adopters).
- [ ] `overlay_policy_replaces_default` — `crud create policy @policy.edit` overrides the synth default policy.
- [ ] `overlay_assign_merges_into_effect` — `assign situation = prospect` + `assign is_active = true` appear in the synthesized `creates` block.
- [ ] `overlay_emits_and_validate` — `emits customer_created` + `validate @validator.percentage` attach to the command.
- [ ] `overlay_input_excludes` — `input excludes situation, is_active` drops those from the synth input.
- [ ] `synth_overlay_ir_equals_handrolled` — the FULL customer trio: synth+overlay IR == hand-rolled IR (the acceptance oracle, unit level).
- [ ] `codegen_overlay_matches_handrolled` — emitted Go identical (golden).
- [ ] `crud_synth_available_fires_on_overlayable` — `VOCAB-CRUD-SYNTH-AVAILABLE-001` now fires on a hand-rolled-with-policy customer fixture.
- [ ] `delete_overlay_is_soft_delete_aware` — with `soft_delete`, the overlaid delete soft-deletes (composes with 0015).

## Gate

### Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test --workspace` green (full sweep, not per-crate).
2. MIGRATE: N/A for THIS spec — it ships the language + proves IR-equivalence on the Pauta `Customer` trio via a read-only scratch diff. The 84-command pilot migration is spec 0003 (this unblocks it).
3. TEACH: `docs/lazuli_way/crud-by-convention.md` updated (overlay = production idiom); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet (byte-identical).
4. ENFORCE: `VOCAB-CRUD-SYNTH-AVAILABLE-001` fires on an overlayable hand-rolled set (proven by `crud_synth_available_fires_on_overlayable`); the idiom doc names it.

**Four concrete gates:**
1. **BUILD** — `cargo test --workspace` 0 failures; `cargo build --workspace` clean; keywords parity + xtask freshness + diagnostics bridge green.
2. **PROVE** — the read-only Pauta `Customer` trio scratch diff is EMPTY (synth+overlay IR == hand-rolled), reported in the final message.
3. **TEACH** — idiom doc + scaffold bullet landed.
4. **ENFORCE** — the inverse linter fires on the overlayable fixture.

## Risks & rollback
- **Overlay merge changes synth behavior for bare adopters** → mitigation: `bare_crud_unchanged` regression test + a read-only Hostpoint `lazuli doctor` dry-run confirming catalog/host/traveler emit unchanged.
- **`assign` RHS grammar drift from the hand-rolled assignment block** → mitigation: REUSE the existing assignment-block parser, don't re-implement; a test that the same RHS parses identically in both positions.
- **IR-equivalence not exact** (synth orders fields differently, etc.) → mitigation: if the diff isn't empty, the overlay is incomplete — narrow scope to the clauses that DO reproduce exactly and report the residual as a follow-up; do NOT claim equivalence loosely.
- **New tokens collide with existing grammar** → if `crud`/`assign` as block headers conflict irreconcilably, STOP and report rather than forking.

**Rollback:** `git revert` — the overlay is additive (parser + analyzer pass + one doctor-rule upgrade + docs); absent the `crud` block nothing changes, so reverting is clean and no pilot depends on it until 0003 migrates.
