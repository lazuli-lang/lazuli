---
id: 0019
title: Plugin resolution — single mandatory pipeline + loud failures
type: adr
status: accepted
created: 2026-06-01
supersedes: —
---

# ADR — Resolution is one extracted stage every loader calls; declared-but-unwired plugins fail loud at the boundary

## Context
- Two near-duplicate loaders exist (`build_module_from_path` mod.rs:52-193, `build_module_with_source_from_path` mod.rs:195-329). The plugin-resolver block was added to the first (lines 182-190) and never copied to the second. `generate go` uses the second (`with_source=true`) → the PT-BR failure. This is structural drift, not a deep limitation: the resolver, alias builder, and Go emission arms all already work; they're just not reached on one path.
- `project_root_for_input` (mod.rs:331-341) returns the input dir itself when it's a dir. hostpoint runs `lazuli generate go app` (features under `app/`, `Lazurite.toml` at repo root) — so the project root is misdetected as `app/`, `lazurite_manifest::load(app/)` returns `Ok(None)`, the alias map is empty, and resolution silently no-ops even on the path that DOES call the resolver.
- Every failure is swallowed: `if let Ok(manifest) = ... if let Ok(alias_map) = ...` (mod.rs:184-187) drops both error arms; `build_alias_map` `continue`s past unreadable manifests with only an `eprintln!` (alias_map.rs:65-78). A plugin can be fully declared and produce zero wiring with zero diagnostics.
- One silent case is legitimate: `lazuli check <single.lzi>` with no project root — there's no `Lazurite.toml`, so `@semantic.*` can't resolve and the doctor anchors `SEMANTIC-PLUGIN-001` at the field. That must stay silent.

## Decision
- **Extract resolution into ONE function** — `resolve_module_plugins(module: &mut Module, input: &Path) -> Result<()>` — and call it from BOTH loaders at the end (just before each returns its module). The two loaders keep their distinct return shapes (one with source map), but neither contains the resolution logic inline; both delegate to the single stage. This makes drift structurally impossible: there is one place resolution lives.
- **Project-root detection walks UP.** Add `find_project_root(input)` that ascends from `input` looking for `Lazurite.toml` (bounded walk to the filesystem root). `lazuli generate go app` finds the repo-root manifest. `project_root_for_input` stays for the no-manifest case; the resolution stage uses the upward search.
- **Loud failure at the boundary, scoped by a precondition.** When a project root WITH a `Lazurite.toml [plugins]` block exists: a declared plugin that fails to resolve (manifest missing, parse error, unsupported carrier, namespace mismatch) is a hard, anchored error — `lazuli: plugin '<ns>' declared in Lazurite.toml [plugins] but failed to wire: <reason> (at <path>)`. `build_alias_map`'s current `eprintln!`+`continue` for a manifest that declares NO semantic types stays silent (that plugin legitimately contributes nothing to the semantic map — it's an adapter). The loud rule fires only when a referenced `@semantic.<X>` in the module has NO resolving plugin AND a `[plugins]` declaration that should have provided it — i.e. the unresolved-alias-with-plugins case. Single-file / no-`[plugins]` stays silent.
- **Reuse, don't rewrite.** `apply_plugin_semantic_resolution` + `build_alias_map` are unchanged internally; the loud-failure surfacing is a new wrapper that inspects the resolved module for residual `UserDefined("@semantic.*")` refs after resolution and, when `[plugins]` is present, converts them to anchored errors.

## Alternatives considered
- **Just copy the resolver block into the second loader** — rejected: fixes PT-BR but leaves two copies that will drift again. The whole point is one stage.
- **Make the doctor the single resolver and have codegen consume its output** — rejected here (it's exactly 0020's job, and it depends on this spec landing the single stage first). Sequencing: unify the codegen/generate paths now; fold doctor in next.
- **Fail loud on EVERY unresolved `@semantic.*` always** — rejected: breaks single-file `lazuli check` and fixtures that legitimately reference yet-unregistered semantics. The precondition (`[plugins]` present + project root found) scopes it correctly.
- **Auto-detect plugins without `[plugins]` declaration** (scan sibling dirs) — rejected: magic; the `[plugins]` block is the authoritative declaration and must stay the single source of truth.

## Consequences
**We accept:** a small refactor of two hot loaders to delegate to one stage (risk: the source-map path has extra steps — the extracted stage must run AFTER feature lowering on both, so it's called at the same logical point). The loud-failure precondition adds a residual-scan over the module post-resolution (cheap; only when `[plugins]` present).
**We gain:** hostpoint builds; the silent-failure class dies; the two loaders can never drift on resolution again; every later Plugin Platform spec (0020-0023) builds on a single, trustworthy resolution stage. The error a confused author sees names the plugin and the fix, at the right layer.
**We watch:** if the loud precondition ever fires on a legitimate single-file or no-plugins flow, the scoping is wrong — tighten the precondition, never broaden the silence. And if a third loader appears, it MUST call `resolve_module_plugins` (enforce via a test that greps for the call, or by making the loaders private and the stage the only public entry).
