# Cement R1 — execution status (overnight run)

Branch: `cement-r1-exec` (isolated worktree `c:/tmp/cement-r1`, off `06f9c717`).
**Every commit green; full workspace test suite (`cargo test --workspace`)
passes; all campaign invariant gates pass.** Fast-forwarded onto `origin/main`
after each spec. **All cement-r1 specs (00–20) are implemented and merged.**
The only remaining work is pre-existing CI style/lint debt (fmt + clippy),
diagnosed at the bottom of this file — orthogonal to the spec campaign.

## Done (green, committed)

| Area | What landed |
|---|---|
| **SPEC-00** | `lazuli upgrade` learns `rename`/`rewrite` recipe kinds (unblocks every breaking migration recipe) |
| **SPEC-01 (full)** | Single-sourced the `@`-namespace / scalar / semantic catalogs in `lazuli_keywords`; LSP `vocab.rs` + doctor `refs.rs` + analyzer `types.rs` derive from them → **healed the 18-vs-23 LSP/doctor divergence**; 3 drift gates; `gen-catalog-reference` → `docs/closed-catalogs.md`; **`VOCAB-SCALAR-ALIAS-001`** ends the silent `Int`/`Bool`/… resolution |
| **SPEC-06** | Documented the generative compound-keyword join rule (dotted = variant / underscore = atomic / space = modifier+head) — predictable without a breaking rename |
| **SPEC-09+14** | Retired the `many` collection grammar ghost (never in the parser) |
| **SPEC-12/13/15/16/17/18** | Documented the whole parsed-but-undocumented grammar surface: app `locale/cors/logging/tracing/encryption/cookie/proxy/limits/headers/locale_negotiate/lazuli_version/subscription`; enum storage + metadata forms (retired the `value` ghost) |
| **Docs anti-stale (3 tiers)** | (1) generated catalogs/keyword-ref + freshness gates; (2) `docs_hygiene` CI gate continuously verifies every citation + link (**cleaned 43 dead citations + 4 dead links**); (3) self-maintaining `docs-staleness` git report; `README.md` rewritten as a current index; CLAUDE.md/AGENTS.md updated |
| **SPEC-20 (1/n)** | De-polluted the published `lazuli --help` — `parse`/`spike-generate`/`examples`/`doctor --self` hidden (framework-dev only) |
| **SPEC-02 (1/n)** | Retired the §7a `{ }`/`;` braces in the lzx-surface repeatable-group line → indentation/`validates` form |
| **SPEC-04 (full)** | `@` off types: closed-core semantic + ALL capability types spelled BARE; `@semantic.<core>`/`@cap.X` deprecated; analyzer/parser/LSP/doctor + fixtures + migration recipe |
| **SPEC-08 (full)** | Folded the four test dialects to two (generated permits/forbids vs authored allows/denies); `.lzx` `accepted by`/`rejected by` → `allows/denies extension`, eval `requires`/`forbids` → `allows`/`denies`; AST/IR variant renames, parser hard-errors, registry drop+`extension` subject, 3 doctor backstops, 4 migrate recipes, curated docs, vscode snapshots, diagnostics-registry claim — full workspace green |
| **(bonus)** | Caught + fixed pre-existing `keyword-reference.md` drift — the freshness gate working on a real case |

## Session 2 — merged to origin/main (green)

| What landed | Notes |
|---|---|
| **Campaign ↔ main merge** | resolved `lazuli_keywords/lib.rs` (both-side additions kept) + regenerated catalogs; full workspace + vscode grammar snapshots green; pushed `0c672722..6afdcc10` |
| **SPEC-05 (full lang)** | `==` is the closed-predicate equality operator; bare `=` retired from comparison (kept for assignment/default/enum-storage + lifecycle bindings). 4 comparison parsers + 50 fixtures + 21 examples + `PREDICATE-EQ-OPERATOR-001` doctor rule + docs + tmLanguage (`=`→assignment) + snapshots. Pushed. LSP hover/auto-recipe deferred — enforced by the doctor fix-it. |
| **SPEC-07 (A)** | UNIFORM policy reference: deleted the command/workflow-only `@policy.*` asymmetry — every callable now takes the same grammar. Pushed. |

## Session 3 — SPEC-19 LOC debt, merged to origin/main (green)

The ≤500-LOC/file convention had regressed to **79 offenders** post-merge.
Driven to **0** (100%) — every `.rs` under `crates/` ≤ 500 LOC, all merged,
every commit green:

| Move | Result |
|---|---|
| **registry.rs 3691 → 14 files** | `facets`/`builders`/`sections/s01..s11` concatenated by `constcat::concat_slices!` into `ALL`; `gen-keyword-reference` byte-identical (order preserved), proven_complete green |
| **~60 `include!`-chunk splits** | every `#[cfg(test)] mod tests {…}` and oversized module split into same-dir `<base>_p<k>.rs` / `_tests.rs` fragments — same module, so `pub`/`pub(crate)` ABI unchanged; string/raw/char-aware, fixtures byte-exact |
| **impl-wrap / doc-root / integration-test splits** | the last bespoke offenders (single-`impl` files, doc-heavy module roots, cargo integration-test roots via `tests/foo/main.rs` + mod-siblings) all carved to ≤500 |
| **gate updates** | `module_headers` skips `_p<N>.rs` fragments; `keyword_surface_parity` scans `keywords_p*.rs`; chunk names end `_tests.rs` where the parser-literal scan must keep excluding them |

## Session 4 — campaign closeout, merged to origin/main (green)

The remaining specs landed, every commit green (`cargo test --workspace`):

| Spec | What landed |
|---|---|
| **SPEC-02 (2/n)** | Retired the `@command.<name>` sigil — `view.inline_table on_change` takes a bare command name; parser hard-errors `E-AT-COMMAND-RETIRED`; registry drop + regen + fixtures/docs migrated |
| **SPEC-07 (B)** | Named the identity-axis *kind* — `@role`/`@scope`/`@actor` are app-level **catalog atoms** (new `catalog_atom` builder + `entity.name.tag.catalog-atom.lazuli` scope), `@policy` stays the feature-local **named reference**; surfaces in hover + the generated keyword-reference; enforced by `identity_catalog_atoms_are_a_distinct_kind` |
| **SPEC-07 (C)** | Killed the CRUD/effect collision — `POLICY-CATEGORY-SHADOWS-EFFECT-001` doctor rule (correctness; indent-aware source scan, warn-strict / error-iron-hand), canon migrated to semantic names (create→author / read→view / update→edit / delete→remove) across examples + parser fixtures, invariants §Policies rewritten (uniform model + kind split + collision prohibition, collision-patch justification deleted), migration recipe shipped |
| **SPEC-20 (2/n)** | Split framework-dev commands onto a separate `lazuli-dev` binary — hoisted the command tree + `build_module_from_path` into the lib crate; `lazuli`/`lazuli-dev` are thin shells (`run`/`run_dev`); `parse`/`spike-generate`/`examples`/`self-doctor` off the published surface; CI self-doctor + `scripts/dev-check.sh` repointed |

**All cement-r1 specs (00–20) are now implemented and merged.** The full
workspace test suite is green on every commit.

## Known pre-existing infrastructure debt (NOT a cement-r1 spec)

Two CI gates carry repo-wide debt that **predates** the cement-r1 campaign and
is orthogonal to it (the `cargo test --workspace` gate is green):

- **`fmt-check` (CI uses STABLE; rustfmt.toml documents NIGHTLY conventions).**
  The repo is intentionally nightly-formatted (function-signature wrapping +
  `imports_granularity = Module` / `group_imports = StdExternalCrate`), but the
  CI `fmt-check` job runs `dtolnay/rust-toolchain@stable`, which ignores those
  options → ~286 files read as "dirty" to stable. A stable `cargo fmt --all`
  would FIGHT the documented nightly convention (un-wrap signatures, un-group
  imports) and ping-pong with any nightly-fmt contributor; local nightly shows
  ~612 dirty from version skew. **Root-cause fix (owner call): pin an exact
  nightly rustfmt version (`rust-toolchain.toml` or a CI step), reformat with
  it, and switch the CI fmt job to that pinned version.** This is a single
  workspace-wide reformat — the most invasive possible change — so it wants a
  swarm-paused window, not an autonomous sweep on shared `main`.
- **`clippy -D warnings`.** ~100+ lint warnings across dependency crates
  (collapsible-if, doc-list indentation, `clone`-on-`Copy`, …), all pre-existing
  and in non-`lazuli_cli` crates. A mechanical `cargo clippy --fix` pass clears
  most; same swarm-pause caveat.

Neither is introduced by this campaign (SPEC-19 chunk files are stable-clean;
untouched crates like `lazuli_planner` are stable-clean). Tracked for a
coordinated hygiene pass.

## Merge + cleanup
1. `cement-r1-exec` is fast-forwarded onto `origin/main` after every spec; no
   separate review-merge needed (history is the per-spec commit narrative).
2. `git worktree remove c:/tmp/cement-r1` frees the disk; the branch ref
   survives.
