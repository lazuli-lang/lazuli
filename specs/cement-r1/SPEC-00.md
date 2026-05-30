# SPEC-00 — Migration engine: `lazuli upgrade` learns `rename` + `rewrite` recipe kinds

**Breaking:** false (engine-only, additive) | **Est. commits:** 3 | **Depends on:** none | **Wave:** 0 (hard prerequisite for every breaking spec)

## Problem
Every breaking spec in this campaign (SPEC-04 `@`-off-types, SPEC-05 `==`, SPEC-06 compound-join, SPEC-07 policy, SPEC-08 test dialects, SPEC-09 collections) promises a `lazuli upgrade` migration recipe so existing pilots auto-migrate off the retired spelling. But `apply_recipe` in `crates/lazuli_cli/src/upgrade.rs:137-141` only implements `kind = "additive"`; every other kind returns `Err(... not yet implemented)`. So no breaking spec can actually ship its recipe. This is a **hard prerequisite** the spec-drafting agents each independently flagged as "extend apply_recipe minimally."

## Evidence
- `apply_recipe` only handles `additive`; `other => Err("not yet implemented")` — `crates/lazuli_cli/src/upgrade.rs:137-141`
- SPEC-06/04/09 each hedge "extend apply_recipe to handle this line-rewrite" — completeness-critic gaps
- The IR-equality smoke harness already exists and can be reused as the `rewrite` verifier — `crates/lazuli_cli/src/upgrade.rs:143-191`

## End state
`apply_recipe` supports three kinds, each with the existing dry-run/`--check`/IR-equality smoke wired:
1. `additive` (exists) — inserts new declarations.
2. `rename` — token-level rename of a keyword/sigil/catalog literal across `.lzi`/`.lzx`, scoped by surface + construct context (not blind text replace), verified IR-equal modulo the rename map.
3. `rewrite` — structured line/region rewrite (e.g. `@semantic.X` → `X`, predicate `=` → `==`), each rule a (matcher, replacement) pair driven from a `recipe.toml`, verified IR-equal against the pre-rewrite IR (the rewrite must be **meaning-preserving** by construction — the smoke harness fails the recipe otherwise).

Recipes live at `migrations/recipes/<slug>/recipe.toml`; the engine is generic so each later spec only ships data, not code.

## Parity surfaces
- `crates/lazuli_cli/src/upgrade.rs` (engine) + a new `upgrade/recipe_kinds/` module dir (≤500 LOC each)
- `migrations/recipes/` schema docs
- CLI exit-codes doc (`docs/cli-exit-codes.md`) if a new failure code is added

## Doctor rules
- _none_ (engine feature, not a lint). The recipes themselves are validated by the IR-equality smoke.

## Tests required
- `rename` recipe round-trips a fixture and is IR-equal modulo the map
- `rewrite` recipe rewrites a fixture and is IR-equal; a deliberately meaning-changing rewrite is **rejected** by the smoke
- `--check` reports staleness without writing; non-zero exit on pending migration

## Docs required
- `docs/migrations.md` documents the three recipe kinds + the `recipe.toml` schema

## LOC plan (≤500/file)
`upgrade.rs` is the orchestrator; extract `recipe_kinds/{additive,rename,rewrite}.rs` siblings + `recipe_kinds/smoke.rs` (the IR-equality verifier). `mod.rs` re-exports. Each sibling ≤500.

## Acceptance criteria
- `cargo test -p lazuli_cli` green
- A throwaway `rename` recipe and a throwaway `rewrite` recipe both pass the IR-equality smoke
- A meaning-changing rewrite fails the smoke (negative test)

## Justification (token / entropy)
Not a surface change — it is the safety rail that lets every later surface change ship reversibly. Without it the breaking cuts either skip migration (stranding any pilot) or get deferred (the dead-proposal failure mode the owner forbade).
