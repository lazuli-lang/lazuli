---
id: 0019
title: Plugin resolution — single mandatory pipeline + loud failures
type: prd
stage: 1 of 5 (Plugin Platform)
status: ready
created: 2026-06-01
---

# PRD — Plugin resolution unify

## Problem
A correctly-declared, correctly-built plugin silently fails to wire, surfacing only as a confusing downstream codegen error. Concrete, reproducible: hostpoint declares `@lazuli/plugin-scalars-br` in `Lazurite.toml [plugins]`, the plugin's `manifest.toml` correctly declares `@semantic.BrazilianCPF/CNPJ/CEP/Phone`, and Go codegen fully handles `SemanticPluginType` — yet `lazuli generate go` fails with `CODEGEN-GO-SEMANTIC-004: @semantic.BrazilianPhone is outside the closed Go semantic table`. Root cause: the plugin-semantic resolver is called in `build_module_from_path` (`crates/lazuli_cli/src/module_loader/mod.rs:182-190`) but NOT in its near-duplicate twin `build_module_with_source_from_path` (mod.rs:195-329), which is the path `generate go` uses (`commands/generate/go.rs:92-97`, `with_source=true`). The `@semantic.Brazilian*` refs stay `TypeRef::UserDefined`, never reach the (working) Go emission arms, and trip the closed-catalog check. Compounding it, every resolution failure mode is SILENT: empty alias map, swallowed `load`/`build_alias_map` errors (`if let Ok`, mod.rs:184-187), project-root misdetection (running from `app/` where `Lazurite.toml` is one level up → `Ok(None)` → empty map), and plugins dropped on manifest parse error (`alias_map.rs:65-78` `continue`).

## Why now (or why ever)
hostpoint cannot `go build` today — it blocks the 0009 god-file split's build closure and any hostpoint deploy. Every PT-BR pilot (the canonical locale) hits this. And it is a CLASS of bug: it will recur for every plugin kind and every author, because resolution is copy-pasted into one of two load paths and failures are swallowed. Fixing the single path + making failures loud unblocks hostpoint immediately and kills the silent-failure class — the foundation the rest of the Plugin Platform (0020-0023) builds on.

## Outcome — done means
1. There is ONE resolution stage that EVERY module-load path funnels through. `build_module_from_path` and `build_module_with_source_from_path` both produce a module with plugin `@semantic.*` refs resolved to `SemanticPluginType` — proven by a test that runs both on the same input and asserts identical resolution.
2. `lazuli generate go` on hostpoint **succeeds** (no `CODEGEN-GO-SEMANTIC-004` for the `@semantic.Brazilian*` types) — the live oracle. The generated Go for `host` imports the scalars-br module and uses the carrier type.
3. Project-root detection finds `Lazurite.toml` by walking UP from the input dir, so `lazuli generate go app` (run with the features subdir) resolves plugins declared in the repo-root `Lazurite.toml`.
4. Resolution failures are LOUD and anchored: a plugin declared in `[plugins]` that fails to wire produces a clear error naming the plugin, the reason (manifest not found at `<path>` / parse error / unsupported carrier / namespace mismatch), and the fix — at the resolution boundary, not as a downstream codegen symptom. The single legitimate silent case (single-file `lazuli check` with no project root) stays silent by design.
5. `docs/plugin-authoring.md` documents the single-pipeline guarantee + the loud-failure contract.

## Non-goals
- The typed multi-kind manifest (adapter/capability schema) — that's 0021. This spec keeps the existing semantic-only `PluginManifest` shape; it only fixes WHERE/HOW resolution runs and how failures surface.
- Unifying doctor's separate alias map with the codegen resolver — that's 0020 (depends on this). Here, doctor is untouched; we fix the codegen/generate load paths.
- `lazuli plugin verify` / `lazuli plugin new` — 0022/0023.
- Adapter (mercadopago/smtp) wiring — those flow through a different path (`lazurite_codegen.rs` side-effect imports), not the semantic resolver; out of scope here.
- Widening the carrier catalog beyond `String`/`Text`.

## User stories
- As a hostpoint dev, `lazuli generate go` succeeds and the BR scalar fields compile, instead of failing with an opaque closed-table error.
- As a plugin author, when my plugin is declared but its manifest path is wrong, I get "plugin `@lazuli/plugin-X` declared in Lazurite.toml but manifest.toml not found at `<resolved path>`" — not a silent no-op that surfaces 200 lines later as a codegen error on a field I didn't write.
- As a framework maintainer, a regression test guarantees the two load paths can never again drift on plugin resolution.

## Constraints
- The single-file `lazuli check <one.lzi>` path (no project root) MUST keep working with unresolved `@semantic.*` (the doctor's `SEMANTIC-PLUGIN-001` anchors it) — loud failure applies only when a project root + `[plugins]` declaration exists and a declared plugin fails to wire.
- No behavior change for apps with zero plugins or all-resolving plugins (the common case): same output, no new errors.
- Reuse the existing `apply_plugin_semantic_resolution` + `build_alias_map`; do not rewrite the resolver, only its call-site topology + error surfacing.

## Open questions
None. The single-pipeline shape + loud-failure boundary are decided in the ADR.
