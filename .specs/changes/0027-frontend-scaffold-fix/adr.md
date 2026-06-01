---
id: 0027
title: Frontend scaffold fix — runtime as workspace-vendored package with prebuilt .d.ts
type: adr
status: accepted
created: 2026-06-01
supersedes: —
---

# ADR — Deliver @lazuli/runtime as a vendored workspace package with prebuilt .d.ts; scaffold emits smoke tests that prove the frontend compiles

## Context
- The runtime ships TS SOURCE (`main`/`types` → `./src/index.ts`), with react/@tanstack/zod/react-hook-form as optional peerDeps. When a consumer path-aliases `@lazuli/runtime` → vendored `src/*.ts`, tsc pulls those source files into the consumer's program (via the import graph, not `include`) and typechecks them under the vendor dir's resolution root, where the peerDeps aren't reachable → 30 `Cannot find module 'react'` errors. (Full trace: `/c/tmp/scaffold-rootcause.jsonl`.)
- `vite build` works because esbuild resolves react from the IMPORTER's node_modules + the `@lazuli/vite` `lazuliAliases()` (which reads `Lazurite.toml [lazuli] path` and aliases at vite-config load). There is NO tsc equivalent — the scaffold tsconfig has no `@lazuli/runtime` paths at all.
- The scaffold's `app/web/package.json` declares `@lazuli/runtime: "^0.1.0"` / `@lazuli/vite: "^0.1.0"` — versions that don't exist; `pnpm install` breaks. The pilots replaced these with hand-vendoring + `file:`/`workspace:` links + manual tsconfig paths.
- Nothing in the scaffold proves the frontend compiles: zero smoke tests, and the Rust scaffold tests assert files exist, not that they build. The defect was invisible until a human ran `tsc`.

## Decision
- **Deliver the runtime as a vendored WORKSPACE PACKAGE consumed via `workspace:*` + prebuilt `dist/*.d.ts` tsconfig paths.** Three coupled changes:
  1. **The runtime emits a real `dist`** (`runtime/ts/lazuli`): `build` becomes `tsc -p tsconfig.build.json` producing `dist/*.js` + `dist/*.d.ts`; `main`→`./dist/index.js`, `types`→`./dist/index.d.ts`, exports-map `types` conditions → `./dist/*.d.ts`. The consumer's tsconfig `paths` point at `dist/*.d.ts`, which `skipLibCheck:true` (already in the scaffold tsconfig) SKIPS — so runtime internals are never typechecked by the consumer. This is the load-bearing change.
  2. **The web scaffolder vendors the runtime** (`cmd_new_frontends/web/mod.rs`): copy `<lazuli path>/runtime/ts/{lazuli,vite,playwright}` (built `dist/` + `package.json`) into the project's `vendor/`, idempotently. Add `vendor/*` to the project's `pnpm-workspace.yaml`. This is the missing CLI step the pilots did by hand.
  3. **The emitted tsconfig + package.json are corrected** (`templates/web/shell.rs`): add the 5 `@lazuli/runtime*` paths (→ `dist/*.d.ts`); change `@lazuli/runtime`/`@lazuli/vite` from `^0.1.0` to `workspace:*`/`file:`; add `search-query-parser` + `@types/node`.
- **The scaffold emits smoke tests that make compilation a gate.** A render smoke (`<App/>` mounts → proves runtime imports resolve + compile) + a generated-SDK smoke (a `@generated/*.react.gen` import resolves) + a `verify:scaffold` script (`tsc --noEmit && vite build`). A Rust integration test scaffolds + installs + tsc + builds and asserts exit 0 (or a structural proxy if pnpm-in-CI is infeasible).
- **Vendoring (not npm-publish) is the delivery.** Offline-capable, matches the existing `vendor/` + `@lazuli/vite`-alias design. The runtime stays `private`.

## Alternatives considered
- **Keep vendoring source `.ts`, just add the runtime's deps to root node_modules** (so the vendor walk-up finds react) — rejected as the durable fix: it makes the consumer typecheck third-party runtime source (slow, brittle, couples consumer strictness to runtime code), and any runtime tsc error becomes the consumer's error. The cheap interim, noted, but `.d.ts` is correct.
- **Publish `@lazuli/runtime` to npm** — rejected for now: breaks offline scaffolding, adds a release pipeline, and the framework is pre-1.0. Revisit at public v1.
- **Exclude the vendor dir from the consumer tsconfig** (`exclude: ["vendor/**"]`) — rejected: `paths` + the import graph pull the source in regardless of `exclude`; only pointing at `.d.ts` (a library boundary) actually stops typechecking.
- **No smoke tests, just fix the wiring** — rejected: the absence of a compile-proving test is WHY this shipped to two pilots. The test is half the fix; without it the next scaffold change silently re-breaks.

## Consequences
**We accept:** a one-time runtime `dist` build step (the runtime now builds, not just `tsc --noEmit`); the scaffolder gains a vendor-copy step (more files written on `lazuli new`); the Rust integration test, if it runs pnpm/tsc/vite, is slower than the file-existence assertions (gate it or use the structural proxy). Vendored `dist` can go stale vs the framework runtime — mitigated by the vendor step copying on scaffold + a doc telling pilots to re-vendor on framework upgrade (or a `lazuli upgrade-runtime` follow-up).
**We gain:** `lazuli new --frontends web` is correct-by-construction (tsc + vite both green); the frontend self-proves via emitted smoke tests; the Rust guardrail catches regressions before they reach a pilot; both pilots go from 46/31 tsc errors to clean. The framework stops being "lixo" on the frontend axis — the scaffold delivers what it promises.
**We watch:** if vendored `dist` staleness bites (pilot runtime drifts from framework), promote a `lazuli upgrade-runtime` command or a CI freshness check. If the prebuilt `.d.ts` hides a real runtime type bug from consumers, the runtime's OWN `tsc` (in framework CI) is where that's caught — keep the runtime's standalone typecheck green in `cargo test`/CI.
