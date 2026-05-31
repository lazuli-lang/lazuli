---
id: 0008
title: Feature cohesion rule — LZI-FEATURE-COHESION-002 + LZI-FILE-SIZE-001 demotion
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: true
track: prove
test_gate: "cargo test -p lazuli_doctor feature_cohesion_002 && lazuli check . && lazuli doctor ."
agent: unassigned
---

# TechSpec — Feature cohesion rule

## Approach
**Naming note (collision avoided):** `LZI-FEATURE-COHESION-001` / `feature_cohesion_001.rs` ALREADY EXISTS and solves a different problem — multiple `feature` blocks in one `.lzi` file lacking a shared name prefix. This spec ships its **sibling** `LZI-FEATURE-COHESION-002` / `feature_cohesion_002.rs` (intra-feature resource-graph disconnection). Do NOT edit or clobber the -001 file; the two are complementary (one is about file packing, one about resource cohesion). Share the `cohesion_graph.rs` helper only.

One new graph-connectivity rule + one re-key/demote of an existing rule, both in `crates/lazuli_doctor`. The rule reads the existing IR relation model (FK fields, `has_many`, `on_delete`); no grammar, no codegen, no runtime change. The intra-feature resource graph is built per `Feature`, connected components are counted via union-find, and ≥2 components fires. `LZI-FILE-SIZE-001` is demoted to a warning and its trigger swapped from LOC to a deterministic `(resource × effect)` surface-area count (LOC stays as body metadata). Two info-level cross-feature heuristics ride along. The idiom doc (stub from 0001) is filled and the scaffold templates get the one-line bullet.

## Surface
**Create:**
- `crates/lazuli_doctor/src/lzi_hygiene/feature_cohesion_002.rs` — the NEW sibling rule: graph builder + union-find components + finding/severity. (Do not edit the existing `feature_cohesion_001.rs`.)
- `crates/lazuli_doctor/src/lzi_hygiene/cohesion_graph.rs` — reusable intra-feature relation graph (nodes/edges from IR), so 0009 and future rules share one builder.
- `crates/lazuli_doctor/tests/feature_cohesion_002.rs` — the rule's test suite (fixtures below).
- `crates/lazuli_doctor/tests/fixtures/cohesion/disconnected.lzi` — 2-cluster fixture modeled on `platform.lzi` (LegalDoc | DataRequest+payload | PlatformConfig).
- `crates/lazuli_doctor/tests/fixtures/cohesion/connected.lzi` — 1-component fixture modeled on `account.lzi` shape (large but one graph).

**Modify:**
- `crates/lazuli_doctor/src/lzi_hygiene/file_size_001.rs` — re-key trigger from LOC to distinct `(resource × effect)` count; default severity → `Warn`; keep LOC in the diagnostic body as metadata.
- `crates/lazuli_doctor/src/lzi_hygiene/mod.rs` — register `feature_cohesion_002` + `cohesion_graph`.
- `crates/lazuli_doctor/src/lzi_hygiene/preset.rs` — wire `LZI-FEATURE-COHESION-002` severities per preset (default Warn / `tdd-strict` Warn / `tdd-iron-hand` Error); record `LZI-FILE-SIZE-001` demotion.
- `crates/lazuli_doctor/src/rule_category.rs` — add `LZI-FEATURE-COHESION-002` to the registry with category + summary.
- `crates/lazuli_diagnostics_registry/...` — register the rule code, title, and one-line help (mirror existing lzi_hygiene entries).
- `docs/lazuli_way/one-feature-one-capability.md` — fill the stub (fixed idiom shape).
- `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — add the idiom bullet under "Authoring idioms" (byte-identical edit in both).

## Contracts

**Graph model (`cohesion_graph.rs`):**
- Node = each `resource` declared in the `Feature`.
- Edge (undirected) between resources A and B of the *same feature* iff any of: A has an FK/relation field typed as B; A declares `has_many B`; an `on_delete` rule on A references B (or vice-versa). Relations to resources in *other* features (reached via `uses`) are **not** edges (intra-feature graph only).
- Components computed by union-find. A feature with 0 or 1 resources is trivially 1 component.

**`LZI-FEATURE-COHESION-002` (Warn; non-waivable-in-spirit):**
- Fires iff `components(feature) >= 2`.
- Diagnostic body lists each component as a named cluster ("Cluster 1: LegalDoc; Cluster 2: DataRequest; Cluster 3: PlatformConfig — no FK / has_many / on_delete edge connects them") and states verbatim: *"You can't honestly waive this: these resources have no relationship. The fix is to split the feature, not to suppress the finding (see `docs/lazuli_way/one-feature-one-capability.md`)."*
- `# doctor:allow LZI-FEATURE-COHESION-002` is honored mechanically (uniform allow-comment contract, spec 0007) — enforcement is by message + grader convention, not by un-silenceability.

**`LZI-FILE-SIZE-001` (demoted → Warn; re-keyed):**
- Trigger = count of distinct `(resource, effect-kind)` pairs in the feature, where effect-kind ∈ {command, query.list, query.lookup, query.sql, query.compose, job, webhook} that name/return/target the resource. Threshold tuned so legit-large-cohesive (`account` 591 LOC, `media_price_tables` 686 LOC) do **not** fire and the four hostpoint waivers become removable.
- LOC retained in the body as informative metadata only; never the trigger.
- Default severity `Warn` (was `Info`/preset-escalated); the point is it stops being the primary decomposition signal — `LZI-FEATURE-COHESION-002` is.

**Info-level companions (in `feature_cohesion_002.rs`):**
- `uses` fan-out ≥4 → info "grab-bag candidate: this feature depends on N other features."
- Cross-feature resource-name similarity ≥0.7 (normalized edit-distance over snake_case resource names across features) → info "duplicated-concern hint: `X.Config` ~ `Y.Config`."

**Idiom-doc shape (fixed; from 0001):**
```
# One feature, one capability
## Reach for this
<one sentence>
## Before (hand-rolled)  /  After (idiomatic)
hostpoint platform.lzi (LegalDoc + PlatformConfig + DataRequest, 0 edges) → legal / data_requests / feature_flags (spec 0009)
## Enforced by
LZI-FEATURE-COHESION-002 — fires when a feature's resource graph has ≥2 disconnected components
```

## Plan — for the executing agent
1. Build `cohesion_graph.rs`: walk a `Feature`, collect resources as nodes, derive intra-feature edges from FK fields / `has_many` / `on_delete`; expose `components(&Feature) -> Vec<Vec<ResourceName>>` via union-find.
2. Write `feature_cohesion_002.rs`: call the graph; if `components.len() >= 2`, emit the finding with one cluster line per component + the non-waivable-in-spirit body text. Add the two info companions (`uses` fan-out, name similarity).
3. Re-key `file_size_001.rs`: replace the LOC trigger with the `(resource × effect)` count; set default severity `Warn`; move LOC into body metadata. Update the module doc to reflect the new trigger.
4. Register both in `mod.rs` + `preset.rs` + `rule_category.rs` + the diagnostics registry (codes, titles, help one-liners).
5. Add fixtures `disconnected.lzi` (3 isolated resources, modeled on `platform.lzi`) and `connected.lzi` (one large connected graph, modeled on `account.lzi`).
6. Write `tests/feature_cohesion_002.rs` (TDD list below).
7. Fill `docs/lazuli_way/one-feature-one-capability.md` in the fixed shape (citing `platform.lzi` before / 0009 after / `LZI-FEATURE-COHESION-002` enforcer).
8. Add the idiom bullet to `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` (identical edit; verify both).
9. Run `lazuli doctor .` on hostpoint (`C:\Users\lucas\hostpoint\app`) and pauta-web to confirm: `platform.lzi` fires `LZI-FEATURE-COHESION-002`; `account`/`payments`/`operations`/`media_price_tables` do **not** fire the re-keyed `LZI-FILE-SIZE-001`.

## Tests first (TDD)
- [ ] `disconnected_feature_fires` — the `platform.lzi`-shaped fixture (3 isolated resources) yields ≥2 components → `LZI-FEATURE-COHESION-002` fires; body names 3 clusters.
- [ ] `connected_feature_silent` — the `account.lzi`-shaped fixture (one connected graph, large) yields 1 component → no `LZI-FEATURE-COHESION-002`.
- [ ] `single_resource_silent` — a 1-resource feature never fires (trivially 1 component).
- [ ] `cross_feature_fk_is_not_an_edge` — an FK to a `uses`d other-feature resource does NOT connect two intra-feature clusters.
- [ ] `file_size_rekeyed_off_loc` — a 700-LOC but low-(resource×effect) cohesive feature does NOT fire `LZI-FILE-SIZE-001`; a high-(resource×effect) feature does (LOC held constant proves trigger swapped).
- [ ] `file_size_is_warn` — `LZI-FILE-SIZE-001` default severity is `Warn`, not `Error`.
- [ ] `uses_fanout_info` — a feature with `uses` ≥4 emits the info-level grab-bag candidate.
- [ ] `name_collision_info` — two features each declaring a `Config`-like resource (≥0.7 similarity) emit the duplicated-concern hint.

## Gate

### Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_doctor feature_cohesion_002` green (all 8 TDD cases).
2. **MIGRATE** — `lazuli doctor .` on hostpoint reports `LZI-FEATURE-COHESION-002` on `platform.lzi` and reports it **not** firing on `account`/`payments`/`operations`; the re-keyed `LZI-FILE-SIZE-001` is silent on those four. (Splits + waiver removal are 0009; this gate just proves the signal is correct.)
3. **TEACH** — `docs/lazuli_way/one-feature-one-capability.md` filled in the fixed shape; `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` carry the idiom bullet.
4. **ENFORCE** — `LZI-FEATURE-COHESION-002` fires on the `disconnected.lzi` fixture and is named in the idiom doc's "Enforced by" line.

## Risks & rollback
- **IR under-models a relation** (e.g. a relation only present in a `query.sql` JOIN) → a truly cohesive feature could fire. Mitigation: the `connected.lzi` fixture + the hostpoint dry-run (gate 2) catch this; if it fires on a clean file, fix the graph builder, don't waive.
- **`(resource × effect)` re-key mis-tunes** and `account.lzi` still fires `LZI-FILE-SIZE-001` → mitigation: gate 2 explicitly checks the four hostpoint files stay silent; tune the threshold against that corpus before merge.
- **Name-similarity false hints** (info only, so low blast radius) → keep at info; never escalate without data.

**Rollback:** `git revert` — the rule and re-key are additive doctor code + one filled doc + two template bullets; no pilot `.lzi` is edited here (0009 owns that), so nothing downstream breaks at runtime.
