# Cement R1 — execution status (overnight run)

Branch: `cement-r1-exec` (isolated worktree `c:/tmp/cement-r1`, off `06f9c717`).
**47 commits, every one green; full workspace + all test binaries compile; all
campaign invariant gates pass.** The branch is preserved in git and ready to
merge when the main checkout's swarm is paused (merging into a swarm-active
branch would be invasive — left to the operator).

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

## Remaining — supervised / blocked tier

- **SPEC-07 (B/C)**: B = reclassify `@role/@scope/@actor` to a catalog-atom registry *kind* (low user-value, touches proven_complete/tmLanguage/hover). C = forbid CRUD-named policy categories — **101 sites across nearly every example**, and the rename to "semantic" names is NON-mechanical (per-category judgment). Opinionated boundary move; per `docs/scope-discipline.md` needs explicit appetite, not an autonomous sweep.
- **SPEC-19 registry.rs split**: BLOCKED — needs `constcat` (no built-in const-slice concat in stable Rust); `constcat` is not in the offline cargo cache. Other >500 files need full Rails production+test concern splits with the documented raw-string-fixture corruption risk.
- **SPEC-20 (2/n)**: the `parse` handler depends on the bin-private compiler entry `build_module_from_path`; a second binary requires hoisting the build pipeline into the lib — central-crate reorg with cascade risk. (1/n already hid the commands from `lazuli --help`.)

## Remaining — the invasive breaking cuts (specced, NOT auto-executed)

Deliberately left for supervised execution: each rewrites the canonical
fixture across parser + analyzer + codegen + snapshots, so it needs full
iterative test-fixing — unsafe to force autonomously under the "não invasiva"
directive (risk of leaving the tree red mid-iteration).

| Spec | Why it's invasive |
|---|---|
| **SPEC-02 (2/n)** | retire the `@command` sigil in lzx-surface action targets (the remaining §7a cut after braces) |
| **SPEC-05** == for equality | the predicate parser uses naive `split_once('=')` in many sites — `==` needs a shared operator tokenizer, touching rules/filters/tests |
| **SPEC-07** policy coherence | analyzer `rbac.rs` + the `@policy`/CRUD-collision + fixtures |
| **SPEC-19** LOC debt | `registry.rs` (now ~3.6k LOC) needs const-slice concat (`constcat` or section-`include!`); ~40 files >500 |
| **SPEC-20 (2/n)** | move `parse`/`spike-generate`/`examples`/`self-doctor` to a `lazuli-dev` binary + `scripts/dev-check.sh` |

Each ships its `lazuli upgrade` recipe (SPEC-00 makes this possible). Run them
one wave at a time, green per commit, on a paused-swarm checkout.

## Merge + cleanup
1. Review/merge `cement-r1-exec` (e.g. `--no-ff` into the integration line) when
   the swarm is paused.
2. `git worktree remove c:/tmp/cement-r1` frees the disk; the branch ref
   survives.
