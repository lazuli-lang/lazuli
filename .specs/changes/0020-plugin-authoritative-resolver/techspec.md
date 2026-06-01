---
id: 0020
title: Plugin-authoritative resolver — doctor and codegen share ONE resolution
type: techspec
track: prove/ship (plugin platform)
depends_on: [0019]
parallel_safe: false
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_doctor plugin_semantic && cargo test --workspace"
agent: unassigned
---

# TechSpec — Plugin-authoritative resolver

> `parallel_safe: false` — this spec shares the resolution stage 0019 creates (`resolve_module_plugins` + `find_project_root`). It must land AFTER 0019 and cannot run concurrently with it (both touch project-root detection + the alias-map seam).

## Approach
Make doctor resolve through the SAME inputs as codegen: re-home 0019's upward `find_project_root` walk into `lazuli_manifest` (reachable from both `lazuli_cli` and `lazuli_doctor_run`), and route doctor's three plugin-semantic alias-map calls through ONE helper that loads the manifest + alias map from that shared root. The duplicate, root-divergent map disappears by construction — same function, same root, same manifest ⇒ same map. The residual condition 0019's loud scan uses ("`[plugins]` present + a `@semantic.<X>` with no resolving alias") is already what `SEMANTIC-PLUGIN-001` expresses; re-pointing the root makes the existing rule fire correctly on the build path, so it becomes the doctor-time mirror of 0019's generate-time bail. The oracle is hostpoint: `lazuli doctor app` and `lazuli generate go app` agree on `@semantic.Brazilian*`.

## Surface
**Create:**
- `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugin_resolution_view.rs` — the single "authoritative alias map for this package" helper: `authoritative_alias_map(package: &DoctorPackage) -> Result<BTreeMap<String, ResolvedPluginSemantic>, PluginManifestError>` + `authoritative_project_root(package: &DoctorPackage) -> PathBuf` (delegates to the shared `lazuli_manifest::lazurite_manifest::find_project_root`). `//!` header MUST carry a trigger-cue phrase (e.g. "fires when a `@semantic.<X>` reference would not resolve on the generate path") for the module_headers meta-lint.
- `crates/lazuli_doctor_run/src/doctor/tests/plugin_semantic_agreement.rs` (or a `#[cfg(test)] mod` named `plugin_semantic_agreement` co-located so `cargo test -p lazuli_doctor plugin_semantic` selects it) — the doctor↔generate agreement test (TDD below).
- `crates/lazuli_doctor_run/tests/fixtures/plugin_agree/` — a fixture app: repo-root `Lazurite.toml [plugins]` → a tiny local plugin whose `manifest.toml` declares one `@semantic.Foo` (carrier String), and a feature under `app/` using `foo: @semantic.Foo`. (Reuse / mirror 0019's `tests/plugin_resolution.rs` fixture shape so the SAME fixture proves both halves.)

**Modify:**
- `crates/lazuli_manifest/src/lazurite_manifest/mod_p1.rs` — add `pub fn find_project_root(input: &Path) -> Option<PathBuf>` (the upward `Lazurite.toml` walk; the canonical home, since both `lazuli_cli` and `lazuli_doctor_run` depend on `lazuli_manifest`). Bounded walk to filesystem root.
- `crates/lazuli_cli/src/module_loader/plugin_resolution.rs` — 0019's `find_project_root` becomes a re-export of / thin delegate to `lazuli_manifest::lazurite_manifest::find_project_root` (codegen behavior byte-identical; 0019's tests stay green).
- `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugins_semantic.rs` — route `check_semantic_plugin_unresolved` (`:107-110`), `check_semantic_plugin_no_validator` (`:42-48`), and `check_plugin_unused` (`:242-243`) through `plugin_resolution_view::authoritative_alias_map(package)` instead of `build_alias_map(Some(manifest), &package.project_root)`. The `SEMANTIC-PLUGIN-001` inline-literal check now evaluates `@semantic.<X>` against the authoritative (upward-walked) root → it fires exactly when generate would fail to resolve, and stays silent when generate resolves.
- `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/mod.rs` — declare `mod plugin_resolution_view;`. (No new `check_*` fan-out entry needed — the existing `check_semantic_plugin_unresolved` IS the residual finding once it reads the authoritative root; if a separate explicit residual walk is added, register it here in the dispatcher.)
- `docs/plugin-authoring.md` — add a "Doctor and codegen agree" section.

**Bridge / registry note:** no NEW diagnostic code is introduced (the residual surfaces as the existing `SEMANTIC-PLUGIN-001`, an inline literal in `lazuli_doctor_run`, which the `lazuli_diagnostics_registry` bridge does NOT scan). The bridge stays untouched. IF the build determines a distinct code is unavoidable, it MUST be a `pub const …CODE…: &'static str` in `crates/lazuli_doctor/src/**` WITH a matching `GLOBAL_DIAGNOSTICS` entry in `crates/lazuli_keywords/src/lib_p1.rs` (category `correctness`, derived via `RuleCategory::from_code_prefix`) — confirm with `cargo test -p lazuli_diagnostics_registry`.

## Contracts

**`find_project_root(input: &Path) -> Option<PathBuf>`** (re-homed to `lazuli_manifest::lazurite_manifest`; the SINGLE upward walk):
- Start at `input` (or `input.parent()` if it is a file); ascend while `!dir.join("Lazurite.toml").is_file()`; stop at filesystem root; return the first dir containing `Lazurite.toml`, else `None`. Bounded (no infinite loop at root). Byte-identical to 0019's semantics — 0019's `find_project_root` now delegates here.

**`authoritative_alias_map(package) -> Result<BTreeMap<String, ResolvedPluginSemantic>, PluginManifestError>`** (`plugin_resolution_view.rs`; the ONE entry the three doctor checks share):
1. `let root = find_project_root(&package.project_root).unwrap_or_else(|| package.project_root.clone())` — walk UP from the doctor's input to the manifest root, exactly as codegen does. (Single-file / no-manifest case falls back to the input root → empty map, preserving the silent single-file behavior.)
2. `let manifest = lazurite_manifest::load(&root)?` (or reuse `package.lazurite_manifest` only when it was loaded from the SAME `root`; prefer loading from `root` so doctor and codegen read the identical manifest).
3. `build_alias_map(manifest.as_ref(), &root)` — the SAME function, SAME root, SAME manifest codegen feeds. Errors propagate (the three callers already convert `Err` into `SEMANTIC-PLUGIN-001` at the project anchor — keep that mapping).

**`SEMANTIC-PLUGIN-001` (extended, NOT re-keyed)** — the residual/doctor-time mirror of 0019's generate-time bail:
- Same code, same message, same field anchor. The ONLY change: the alias map it checks against is built from the AUTHORITATIVE upward-walked root (via `authoritative_alias_map`), not the divergent `package.project_root`. After this, `@semantic.<X>` fires `SEMANTIC-PLUGIN-001` **iff** the same `@semantic.<X>` would leave a residual `UserDefined("@semantic.*")` on 0019's generate path (`[plugins]` present + no resolving alias) — i.e. doctor and generate agree on the unresolved set.
- Built-in catalog short-circuit (`BUILT_IN_SEMANTIC`) and the `@semantic.Money(...)` head-token lift are unchanged.

**Agreement invariant (drift guard):** a test asserts, on the SAME plugin-using fixture dir run from the SAME features subdir: doctor's plugin-semantic resolution set == the generate path's residual-`@semantic.*` set. Both resolve (zero `SEMANTIC-PLUGIN-001` ∧ zero generate residual), OR both flag the identical alias (the `SEMANTIC-PLUGIN-001` alias == the generate-path unresolved alias). They can never disagree.

## Plan — for the executing agent
1. Read 0019's landed `crates/lazuli_cli/src/module_loader/plugin_resolution.rs` (the `find_project_root` + residual-scan it shipped — this spec depends on it existing), `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugins_semantic.rs` (the three `build_alias_map` call sites), `helpers.rs:226-235` (`doctor_project_root`), and `crates/lazuli_manifest/src/plugin_manifest/alias_map.rs` (`build_alias_map` signature + error variants).
2. Re-home `find_project_root` into `lazuli_manifest::lazurite_manifest::mod_p1` (`pub fn`); make 0019's `lazuli_cli` copy delegate/re-export. Run `cargo test -p lazuli_cli plugin_resolution` to confirm 0019's tests stay green (byte-identical behavior).
3. Create `plugin_resolution_view.rs`: `authoritative_alias_map` + `authoritative_project_root`, with the trigger-cue `//!` header. Wire `mod plugin_resolution_view;` in `aggregators/lazurite_manifest/mod.rs`.
4. Route the three `plugins_semantic.rs` checks through `authoritative_alias_map(package)`. Confirm the `lazurite_manifest_diagnostics` short-circuit (`mod.rs:40-42`) still guards the genuinely-no-manifest case, but that a features-subdir input now resolves via the upward walk (the manifest IS found one level up).
5. Add fixture `tests/fixtures/plugin_agree/` (mirror 0019's `plugin_resolution.rs` fixture so ONE fixture proves both halves) + write the agreement test `plugin_semantic_agreement` (TDD list below).
6. Run the gate (below). The CRITICAL acceptance is step-7 live hostpoint.
7. LIVE PROOF (read-only on hostpoint): `cargo build -p lazuli_cli`; from `C:\Users\lucas\hostpoint`: run `target\debug\lazuli.exe doctor app` AND `target\debug\lazuli.exe generate go app --out app\dist\go`. Confirm they AGREE on `@semantic.Brazilian*`: doctor emits ZERO false `SEMANTIC-PLUGIN-001` for the BR scalars AND generate emits ZERO `CODEGEN-GO-SEMANTIC-004` for them (both resolve). Capture before/after doctor finding counts for the BR aliases. DO NOT commit hostpoint changes (dist/ is regen).
8. TEACH: `docs/plugin-authoring.md` "Doctor and codegen agree" section.
9. Commit framework on `loop-serial`.

## Tests first (TDD)
- [ ] `plugin_semantic_doctor_and_generate_agree` — on the `plugin_agree` fixture run from the `app/` subdir, doctor's `SEMANTIC-PLUGIN-001` set for `@semantic.Foo` is EMPTY ∧ the generate path's residual `UserDefined("@semantic.Foo")` set is EMPTY (both resolve via the upward walk). The agreement drift-guard.
- [ ] `doctor_resolves_plugin_from_features_subdir` — `lazuli doctor` with input = `<root>/app` resolves plugins declared in `<root>/Lazurite.toml` (NO false `SEMANTIC-PLUGIN-001`); proves the shared upward walk replaced `doctor_project_root`'s pass-through.
- [ ] `doctor_flags_unresolved_alias_generate_also_fails` — a fixture with `[plugins]` present but a `@semantic.Bar` no declared plugin provides: doctor fires `SEMANTIC-PLUGIN-001` on `@semantic.Bar` ∧ the generate path leaves it a residual (both flag the SAME alias).
- [ ] `single_file_check_stays_silent_no_hard_error` — a single `.lzi` (no project root) yields the existing field-anchored `SEMANTIC-PLUGIN-001` (unresolved) and NO hard error / NO panic — mirrors 0019's silent single-file case.
- [ ] `no_plugins_app_unchanged` — an app with no `[plugins]` block produces zero new doctor findings (common-case regression).
- [ ] `find_project_root_shared_with_codegen` — `lazuli_manifest::lazurite_manifest::find_project_root` returns the SAME root for the SAME input that 0019's codegen resolver uses (assert via the re-export identity, or that running from a subdir finds the repo-root `Lazurite.toml`).

## Gate

### Definition of Done (Lazuli Plugin Platform gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep, not per-crate)**; `cargo test -p lazuli_doctor plugin_semantic` green.
2. PILOT: hostpoint `lazuli doctor app` and `lazuli generate go app` AGREE on `@semantic.Brazilian*` — doctor emits no false `SEMANTIC-PLUGIN-001` for the BR scalars and generate emits no `CODEGEN-GO-SEMANTIC-004` for them (live proof, captured before/after).
3. TEACH: `docs/plugin-authoring.md` documents the "doctor and codegen agree" guarantee.
4. ENFORCE: the `plugin_semantic_doctor_and_generate_agree` drift-guard test (+ the subdir + unresolved-alias tests) prevent the two surfaces from re-forking.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_doctor plugin_semantic` (all 6 TDD) + `cargo test --workspace` 0 failures + `cargo build --workspace` clean. No new diagnostic code (residual surfaces as existing `SEMANTIC-PLUGIN-001`) — run `cargo test -p lazuli_diagnostics_registry` + `cargo test -p lazuli_keywords` to confirm the bridge + proven-complete gates stay green. IF a new code WAS added, it is claimed in `GLOBAL_DIAGNOSTICS` and these two gates prove it.
2. **PILOT** — hostpoint `doctor app` BR-scalar false `SEMANTIC-PLUGIN-001`: before = N, after = 0; `generate go app` BR-scalar `CODEGEN-GO-SEMANTIC-004`: 0 (report actual counts; both agree on resolved).
3. **TEACH** — `docs/plugin-authoring.md` "Doctor and codegen agree" section landed.
4. **ENFORCE** — the agreement drift-guard + the subdir-resolution + unresolved-alias-agreement tests green; the module_headers meta-lint passes on `plugin_resolution_view.rs` (trigger-cue header present).

## Risks & rollback
- **Re-homing `find_project_root` breaks 0019's call site / tests** → mitigation: make 0019's `lazuli_cli` `find_project_root` a re-export (`pub use lazuli_manifest::lazurite_manifest::find_project_root;`) or a one-line delegate; run `cargo test -p lazuli_cli plugin_resolution` (step 2) before touching doctor. Byte-identical semantics required.
- **Doctor now FIRES on a project that was previously false-clean** (silent because the root was misdetected) → that is the bug being fixed; verify against the hostpoint corpus (gate 2) that every NEW `SEMANTIC-PLUGIN-001` corresponds to a real generate-path residual, not a false positive. If a legit project trips it, the alias genuinely isn't provided — the finding is correct.
- **Loading the manifest twice (doctor already has `package.lazurite_manifest`)** → mitigation: prefer loading from the authoritative `root` inside `authoritative_alias_map` for byte-fidelity with codegen; if `package.lazurite_manifest` was loaded from a DIFFERENT root, using it would re-introduce the divergence — so the helper owns the root + load, not the package field.
- **0019 has NOT landed when this runs** (`parallel_safe: false`) → mitigation: the executing agent confirms `crates/lazuli_cli/src/module_loader/plugin_resolution.rs` + `find_project_root` exist (step 1) before starting; if absent, this spec blocks on 0019.

**Rollback:** `git revert` — the change is a function relocation (with re-export), one new doctor helper module + its `mod` line, three call-site root swaps, one fixture, the agreement tests, and a doc section. Absent it, doctor reverts to its divergent root (the bug). No pilot file is committed.
