---
id: 0027
title: Frontend scaffold fix — lazuli new produces a frontend that typechecks + builds + self-proves
type: prd
stage: standalone (framework defect)
status: ready
created: 2026-06-01
---

# PRD — Frontend scaffold fix

## Problem
`lazuli new --frontends web` generates a frontend that does NOT pass `tsc --noEmit`. Both pilots inherit it: hostpoint (the canonical pilot) shows 46 tsc errors, 30 of them INSIDE its vendored `@lazuli/runtime`; pauta shows ~31 of the same. Measured root cause (full file:line analysis in `/c/tmp/scaffold-rootcause.jsonl`):
1. The emitted `app/web/package.json` declares **phantom deps** — `@lazuli/runtime: "^0.1.0"` and `@lazuli/vite: "^0.1.0"` — versions that don't exist (the real package is `0.0.0`, private). `pnpm install` can't resolve them.
2. The emitted `app/web/tsconfig.json` has **NO `@lazuli/runtime` paths** — yet every generated SDK file (`dist/ts-web/*.gen.ts`) imports `@lazuli/runtime` / `@lazuli/runtime/react`. tsc sees the imports, can't resolve them.
3. The runtime is delivered (when delivered at all — pilots did it by hand) as **vendored `.ts` source**, not pre-built `.d.ts`. tsc pulls the runtime source into the consumer's program via the import graph and typechecks it under the WRONG resolution root (the vendor dir, where `react`/`@tanstack/react-query` aren't reachable) → the 30 `Cannot find module 'react'` errors.
4. The scaffold emits **zero frontend smoke tests** and its Rust scaffold tests only assert files EXIST, not that they COMPILE. That's why the defect shipped to both pilots undetected.

CRITICAL nuance: **`vite build` SUCCEEDS** (exit 0, full bundle) on the same tree — vite resolves react from the importer's node_modules + the `@lazuli/vite` alias. So the app RUNS; the broken thing is the `tsc` typecheck gate. The defect is wiring, not a non-functional app — but a framework whose `lazuli new` produces a red typecheck gate, and whose canonical pilot ships 46 tsc errors, is a framework whose quality gate lies.

## Why now (or why ever)
The maker's words: "se todo projeto tiver que fazer isso, é um framework lixo." This is not a one-pilot mistake — it's a scaffolder defect every new project inherits. Pauta and hostpoint both hand-vendored the runtime + added tsconfig paths to limp along; that hand-work is exactly what a scaffolder must do. Until fixed, every `lazuli new --frontends web` ships a frontend that fails `tsc` / `verify:all`, and no generated test catches it. Fixing the scaffold + emitting smoke tests makes the frontend correct-by-construction and self-proving — the same discipline the backend got from the 18-spec loop.

## Outcome — done means
1. `lazuli new demo --frontends web` + `pnpm install` produces a frontend where **`tsc --noEmit` AND `vite build` both exit 0**.
2. The TS runtime is delivered correctly: vendored as a workspace package consumed via `workspace:*` + **pre-built `dist/*.d.ts`** tsconfig paths, so the consumer NEVER typechecks runtime source (the `.d.ts` is covered by the already-present `skipLibCheck`).
3. The emitted `app/web/package.json` has real, resolvable deps (no phantom `^0.1.0`) + the runtime's required peer deps (`search-query-parser`, `@types/node`).
4. The scaffold emits **generated smoke tests**: a render smoke (`<App/>` mounts via testing-library, proving runtime imports resolve + compile) + a generated-SDK smoke (the `dist/ts-web` imports resolve under the client tsconfig). Plus a `verify:scaffold` script (`tsc --noEmit && vite build`).
5. A Rust integration test scaffolds a project, runs `pnpm install + tsc --noEmit + vite build`, and asserts exit 0 — the guardrail that would have caught this. (If running pnpm/tsc in the Rust test is infeasible in CI, assert the emitted tsconfig has the runtime paths + the package.json has no phantom deps + the smoke files exist — a structural proxy — and document the gap.)
6. Both pilots (hostpoint + pauta) re-wired to the corrected delivery → the 30 vendor-internal `Cannot find module 'react'` errors gone in both.

## Non-goals
- Building NEW frontend screens (that's a later task — this is purely making the scaffold + existing screens compile).
- Changing the codegen (`lazuli generate ts`) — it emits correct `@lazuli/runtime` imports; the fix is wiring the scaffold to resolve them, not changing what codegen emits.
- The mobile/native scaffold (`--frontends mobile`) — web first; mobile mirrors after.
- Publishing `@lazuli/runtime` to a real npm registry — vendoring (offline-capable) is the chosen delivery; publishing is a separate future decision.

## User stories
- As a dev running `lazuli new`, my frontend passes `tsc` + `vite build` on the first `pnpm install`, with no hand-vendoring.
- As a framework maintainer, the scaffold's own smoke tests + the Rust integration test fail loudly if the frontend wiring breaks — the defect can't ship again.
- As a pilot owner (hostpoint/pauta), my web client's `verify:all` is green instead of carrying 46 tsc errors.

## Constraints
- Vendoring must stay offline-capable (matches the existing `vendor/` convention + the `@lazuli/vite` alias design). No hard dependency on a network registry.
- `vite build` must STAY green (it works today) — the tsc fix must not break the working vite path.
- The runtime `dist` build is a one-time framework cost; the runtime currently has `build: tsc --noEmit` (emits nothing) — that becomes a real emit.

## Open questions
None. The delivery architecture (workspace-vendor + prebuilt `.d.ts`) is decided in the ADR.
