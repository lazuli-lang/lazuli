# Cement R1 — execution status (overnight run)

Branch: `cement-r1-exec` (isolated worktree `c:/tmp/cement-r1`, off `06f9c717`).
**16 commits, every one green; full workspace + all test binaries compile; all
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
| **(bonus)** | Caught + fixed pre-existing `keyword-reference.md` drift — the freshness gate working on a real case |

## Remaining — the invasive breaking cuts (specced, NOT auto-executed)

Deliberately left for supervised execution: each rewrites the canonical
fixture across parser + analyzer + codegen + snapshots, so it needs full
iterative test-fixing — unsafe to force autonomously under the "não invasiva"
directive (risk of leaving the tree red mid-iteration).

| Spec | Why it's invasive |
|---|---|
| **SPEC-02** §7a retire | lzx-surface parser + analyzer `list_decls.rs` + codegen `lzx_ux.rs` + fixtures; the brace/`Int`/`@command` dialect must be rewritten to indentation |
| **SPEC-04** @ off types | every `@semantic.`/`@cap.` type site + codegen Go/TS + tmLanguage + every fixture; the biggest sweep |
| **SPEC-05** == for equality | the predicate parser uses naive `split_once('=')` in many sites — `==` needs a shared operator tokenizer, touching rules/filters/tests |
| **SPEC-07** policy coherence | analyzer `rbac.rs` + the `@policy`/CRUD-collision + fixtures |
| **SPEC-08** test dialects | lzx view-test parser + analyzer + `TEST-VIEW-*` doctor + `full-capsule.lzx` |
| **SPEC-19** LOC debt | `registry.rs` (3601 LOC) needs const-slice concat (`constcat` or section-`include!`); ~40 files >500 |
| **SPEC-20 (2/n)** | move `parse`/`spike-generate`/`examples`/`self-doctor` to a `lazuli-dev` binary + `scripts/dev-check.sh` |

Each ships its `lazuli upgrade` recipe (SPEC-00 makes this possible). Run them
one wave at a time, green per commit, on a paused-swarm checkout.

## Merge + cleanup
1. Review/merge `cement-r1-exec` (e.g. `--no-ff` into the integration line) when
   the swarm is paused.
2. `git worktree remove c:/tmp/cement-r1` frees the disk; the branch ref
   survives.
