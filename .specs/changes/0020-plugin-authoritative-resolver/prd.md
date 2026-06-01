---
id: 0020
title: Plugin-authoritative resolver — doctor and codegen share ONE resolution
type: prd
stage: 2 of 5 (Plugin Platform)
status: ready
created: 2026-06-01
---

# PRD — Plugin-authoritative resolver

## Problem
`lazuli doctor` and `lazuli generate go` disagree about whether a plugin resolves, so **doctor-green ≠ codegen-green**: a plugin can pass `SEMANTIC-PLUGIN-001` and still fail `generate go` (or vice-versa). The doctor builds its OWN plugin alias map from `.lzi` text, on a DIFFERENT project-root than codegen uses:
- Doctor's plugin-semantic checks call `build_alias_map(Some(manifest), &package.project_root)` (`crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugins_semantic.rs:42-48` in `check_semantic_plugin_no_validator`, `:107-110` in `check_semantic_plugin_unresolved`, `:242-243` in `check_plugin_unused`). `package.project_root` comes from `doctor_project_root` (`crates/lazuli_doctor_run/src/doctor/helpers.rs:226-235`), which **passes a directory input through unchanged** — it does NOT walk up to find `Lazurite.toml`.
- Codegen (post-0019) resolves through `resolve_module_plugins` → `find_project_root` (`crates/lazuli_cli/src/module_loader/plugin_resolution.rs`), which **walks UP** to the nearest `Lazurite.toml`, then runs the SAME `build_alias_map` over that root.
- Result: `lazuli doctor app` (run with the features subdir, `Lazurite.toml` one level up) builds an EMPTY alias map → every `@semantic.BrazilianCEP` either false-flags `SEMANTIC-PLUGIN-001` or, with the manifest absent at that root, the whole manifest aggregator short-circuits (`lazurite_manifest_diagnostics` returns `Vec::new()` when `project_has_lazurite_manifest` is false, `aggregators/lazurite_manifest/mod.rs:40-42`) — so doctor goes SILENT on a plugin that `generate go app` resolves perfectly. The two surfaces use the same `build_alias_map` function but feed it DIFFERENT roots, so they can never be trusted to agree.

This is **Seam 4** from the resolution survey: doctor builds a resolution view independent of the one codegen trusts. 0019 unified the two CODEGEN load paths into one stage; it explicitly left doctor out of scope. This spec folds doctor into that same authoritative resolution.

## Why now (or why ever)
0019 makes `generate go` trustworthy but leaves a worse trap: `lazuli doctor` is the gate every agent and CI runs BEFORE codegen, and it now reports a DIFFERENT resolution than the build will perform. An author who fixes every doctor finding can still hit `CODEGEN-GO-SEMANTIC-004` at `generate` time — the exact opaque failure 0019 set out to kill, re-entering through the front door. And it is a CLASS of bug: any future plugin kind (adapter, capability — specs 0021+) will be checked by doctor and consumed by codegen, and the two will drift again unless they resolve through one entry. Closing this seam makes `lazuli doctor` a true precondition for `generate`: doctor-green ⇒ the generate path resolves identically.

## Outcome — done means
1. Doctor's plugin-semantic check consumes the SAME authoritative resolution as codegen: the SAME `build_alias_map` over the SAME project-root detection (the upward walk 0019 centralized). The duplicate, root-divergent text map is eliminated — there is ONE project-root rule and ONE alias map shared by doctor + check + every generate path.
2. A doctor finding fires exactly when a `@semantic.<X>` reference would NOT resolve on the real generate path — the SAME condition 0019's loud residual-scan uses, surfaced at `lazuli doctor` time (anchored at the field) instead of only at `generate` time. The author sees the resolution failure at the cheapest gate.
3. `lazuli doctor` on hostpoint run with the features subdir (`app/`) resolves the repo-root `Lazurite.toml [plugins]` exactly as `generate go app` does — no false `SEMANTIC-PLUGIN-001`, no false-clean silence.
4. A test proves doctor + the generate path AGREE on a plugin-using fixture: both resolve, or both flag the same unresolved alias. The agreement is mechanical, not coincidental.
5. `docs/plugin-authoring.md` documents the "doctor and codegen agree" guarantee: passing `lazuli doctor` means the plugin resolves on `generate`.

## Non-goals
- Re-architecting the resolver internals or `build_alias_map` — unchanged; this spec only unifies the project-root detection doctor feeds it and adds the residual finding. (Resolver internals are 0019's frozen surface.)
- The typed multi-kind manifest (adapter/capability schema) — 0021.
- `lazuli plugin verify` / `lazuli plugin new` — 0022/0023.
- Adapter (mercadopago/smtp) wiring — flows through `lazurite_codegen.rs` side-effect imports, not the semantic resolver; out of scope (same boundary as 0019).
- Changing the SEMANTIC-PLUGIN-001/002 message text or the closed built-in catalog (`BUILT_IN_SEMANTIC`).

## User stories
- As a hostpoint dev, if `lazuli doctor` is green on plugin types, `lazuli generate go` resolves those same types — I never get a `generate`-time `CODEGEN-GO-SEMANTIC-004` for a `@semantic.*` that doctor said was fine.
- As a plugin author, when I declare a plugin but a `@semantic.<X>` it should provide won't resolve on the build path, `lazuli doctor` tells me at the field site — not 200 lines into a codegen dump.
- As a framework maintainer, a regression test guarantees doctor and the generate path can never again resolve the same project differently.

## Constraints
- Single-file `lazuli check <one.lzi>` (no project root) keeps its current behavior: empty alias map, `@semantic.*` unresolved, anchored `SEMANTIC-PLUGIN-001` — NOT a hard error (mirrors 0019's silent single-file case; the loud residual is scoped to "project root + `[plugins]` present").
- No new false positives on apps with zero plugins or all-resolving plugins. The hostpoint corpus is the regression oracle.
- Reuse 0019's centralized project-root walk; do NOT add a third root-detection function. If 0019's `find_project_root` is private to `lazuli_cli`, this spec re-homes it (or its twin) to `lazuli_manifest` so `lazuli_doctor_run` — which depends on `lazuli_manifest` but NOT `lazuli_cli` — can call the identical walk.
- Any NEW diagnostic code must register in `lazuli_keywords` (GLOBAL_DIAGNOSTICS or a capability `produces`) or it fails the `lazuli_diagnostics_registry` bridge; any new doctor module needs a `//!` header with a trigger-cue phrase for the module_headers meta-lint.

## Open questions
None. Whether to extend `SEMANTIC-PLUGIN-001` vs. add a new code, and where the shared root-walk lives, are decided in the ADR.
