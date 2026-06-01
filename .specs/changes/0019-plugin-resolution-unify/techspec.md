---
id: 0019
title: Plugin resolution — single mandatory pipeline + loud failures
type: techspec
track: ship (plugin platform)
depends_on: []
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_cli plugin_resolution && cargo test --workspace"
agent: unassigned
---

# TechSpec — Plugin resolution unify

## Approach
Extract the inline resolver block from `build_module_from_path` into one function `resolve_module_plugins(&mut Module, input)` and call it from BOTH loaders. Add `find_project_root(input)` that walks up to the nearest `Lazurite.toml`. Add a post-resolution residual scan that, when a `[plugins]` block exists, converts any leftover `UserDefined("@semantic.*")` ref into an anchored hard error. No resolver-internals rewrite; pure call-site topology + error surfacing. The oracle is hostpoint `lazuli generate go` succeeding.

## Surface
**Create:**
- `crates/lazuli_cli/src/module_loader/plugin_resolution.rs` — `resolve_module_plugins(module: &mut Module, input: &Path) -> anyhow::Result<()>` (the single stage) + `find_project_root(input: &Path) -> Option<PathBuf>` (upward walk) + the residual-scan loud-failure logic.
- `crates/lazuli_cli/tests/plugin_resolution.rs` — the regression + parity tests (TDD below).

**Modify:**
- `crates/lazuli_cli/src/module_loader/mod.rs` — `build_module_from_path` (replace the inline block at 182-190 with `resolve_module_plugins(&mut module, input)?`); `build_module_with_source_from_path` (add `resolve_module_plugins(&mut module, input)?` just before the `Ok((module, source_map, feature_file_ids))` return at line 328); declare `mod plugin_resolution;`.
- `docs/plugin-authoring.md` — add a "Resolution guarantee" section: one pipeline, every load path resolves, declared-but-unwired = loud anchored error.

## Contracts
**`resolve_module_plugins(module: &mut Module, input: &Path) -> Result<()>`** (the frozen single entry point; 0020 will make doctor call this too):
1. `let Some(root) = find_project_root(input) else { return Ok(()) }` — no project root (single-file check) → silent no-op, unresolved `@semantic.*` left for doctor.
2. `let manifest = lazurite_manifest::load(&root)` — on `Err`, return a loud error (`failed to read Lazurite.toml at <root>: <e>`); on `Ok(None)`, return `Ok(())` (no manifest = nothing to resolve).
3. `let alias_map = build_alias_map(manifest.as_ref(), &root)?` — propagate `build_alias_map` errors LOUDLY (today they're swallowed); these are the namespace-mismatch / unsupported-carrier cases.
4. `apply_plugin_semantic_resolution(module, &alias_map)` — unchanged.
5. **Residual scan (loud failure):** if `manifest` has a non-empty `[plugins]` block, walk the resolved module for any field/input/event `TypeRef::UserDefined(q)` where `q.name` starts with `@semantic.` → collect them; if any remain, `bail!` with one anchored message per distinct alias: `plugin semantic '<alias>' is referenced but no declared plugin provides it — declare the contributing plugin in Lazurite.toml [plugins], or check its manifest.toml [[semantic_types]] (declared plugins: <list>)`. (Reuse the walk shape from `apply_plugin_semantic_resolution` — same TypeRef sites.)

**`find_project_root(input) -> Option<PathBuf>`:** start at `input` (or `input.parent()` if it's a file); ascend while `!dir.join("Lazurite.toml").is_file()`; stop at filesystem root; return the first dir containing `Lazurite.toml`, else `None`. Bounded (no infinite loop on root).

**Invariant test contract (drift guard):** a test asserts BOTH loaders, given the same plugin-using input dir, yield a module whose `@semantic.<plugin>` refs are ALL `SemanticPluginType` (none left `UserDefined`).

## Plan — for the executing agent
1. Read `crates/lazuli_cli/src/module_loader/mod.rs` fully (both loaders + `project_root_for_input`), `crates/lazuli_cli/src/plugin_semantic_resolver.rs` (the walk shape to reuse for the residual scan), `crates/lazuli_manifest/src/plugin_manifest/alias_map.rs` (the error variants to surface loudly).
2. Create `plugin_resolution.rs`: move the resolution logic into `resolve_module_plugins`; add `find_project_root`; add the residual scan. Use `lazurite_manifest::load` + `plugin_manifest::build_alias_map` + `plugin_semantic_resolver::apply_plugin_semantic_resolution` (same calls, now in one place + loud).
3. Edit both loaders to call `resolve_module_plugins(&mut module, input)?` at their end. Remove the inline block from `build_module_from_path`.
4. Write `tests/plugin_resolution.rs` (TDD — write FIRST):
   - a fixture app dir under the test (or reuse `crates/lazuli_cli/tests/fixtures/`) with a `Lazurite.toml [plugins]` pointing at a tiny local plugin whose `manifest.toml` declares one `@semantic.Foo` (carrier String), and a feature using `foo: @semantic.Foo`.
   - assert both loaders resolve it; assert running from a SUBDIR resolves via upward search; assert a declared-but-missing-manifest plugin yields the loud error; assert single-file check (no root) stays silent.
5. Run the gate (below). The CRITICAL acceptance is step-6 live hostpoint.
6. LIVE PROOF (read-only on hostpoint): `cargo build -p lazuli_cli`, then from `C:\Users\lucas\hostpoint`: `target\debug\lazuli.exe generate go app --out app\dist\go`. Confirm ZERO `CODEGEN-GO-SEMANTIC-004` for `@semantic.Brazilian*` (there may be OTHER pre-existing errors — report them, but the BR-scalar closed-table errors MUST be gone). Capture the before/after error count. DO NOT commit hostpoint changes (dist/ is regen; this is a build-success proof).
7. TEACH: `docs/plugin-authoring.md` "Resolution guarantee" section.
8. Commit framework on `loop-serial`.

## Tests first (TDD)
- [ ] `both_loaders_resolve_plugin_semantics` — `build_module_from_path` and `build_module_with_source_from_path` on the same plugin-using dir both yield zero residual `UserDefined("@semantic.*")` (the drift guard).
- [ ] `resolves_from_subdir_via_upward_search` — running the resolver with `input = <root>/app` resolves plugins declared in `<root>/Lazurite.toml`.
- [ ] `declared_plugin_missing_manifest_fails_loud` — a `[plugins]` entry whose path has no `manifest.toml` → anchored error naming the plugin + path (not a silent no-op).
- [ ] `unsupported_carrier_fails_loud` — a manifest with `carrier_type = "Integer"` → loud `build_alias_map` error propagated (today swallowed).
- [ ] `single_file_check_stays_silent` — `resolve_module_plugins` with a single `.lzi` file input (no project root) returns `Ok(())`, leaves `@semantic.*` unresolved, no error.
- [ ] `no_plugins_app_unchanged` — an app with no `[plugins]` block resolves to `Ok(())` with no new errors (common-case regression).

## Gate

### Definition of Done (Lazuli Plugin Platform gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep, not per-crate)**.
2. PILOT: hostpoint `lazuli generate go app` no longer emits `CODEGEN-GO-SEMANTIC-004` for `@semantic.Brazilian*` (live proof, captured before/after).
3. TEACH: `docs/plugin-authoring.md` documents the single-pipeline + loud-failure guarantee.
4. ENFORCE: the `both_loaders_resolve_plugin_semantics` drift-guard test + the loud-failure tests prevent regression.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_cli plugin_resolution` (all 6 TDD) + `cargo test --workspace` 0 failures + `cargo build --workspace` clean. parser↔registry parity untouched (no keyword change) but run `cargo test -p lazuli_keywords` to confirm. No new diagnostic code added (loud failures are anyhow errors, not doctor codes) — if you DO add a code, register it in facets + bridge.
2. **PILOT** — hostpoint `generate go` BR-scalar errors: before = 13, after = 0 (report actual counts).
3. **TEACH** — plugin-authoring.md section landed.
4. **ENFORCE** — drift-guard + loud-failure tests green.

## Risks & rollback
- **The extracted stage runs at the wrong point on the source-map path** (must be after feature lowering + invalidates resolution, before return) → mitigation: the `both_loaders_resolve` test catches any ordering bug; place the call at line 328 (after `attach_lzx_surfaces`, before the `Ok` return).
- **Loud precondition fires on a legitimate flow** (fixture referencing an unregistered `@semantic.*` with a `[plugins]` block present) → mitigation: the precondition requires `[plugins]` non-empty AND a residual `@semantic.*`; scope to exactly the "declared plugins but this alias unprovided" case; the `single_file` + `no_plugins` tests guard the silent paths. If a real fixture trips it, the alias genuinely isn't provided — that's the correct error.
- **hostpoint has OTHER generate errors** beyond BR scalars → mitigation: the pilot gate is specifically "no BR-scalar CODEGEN-GO-SEMANTIC-004", not "generate fully green"; report other errors as out-of-scope (they belong to other features/specs).

**Rollback:** `git revert` — the change is a call-site refactor + one new module + doc; absent it, behavior is exactly today's (the bug). No pilot file is committed.
