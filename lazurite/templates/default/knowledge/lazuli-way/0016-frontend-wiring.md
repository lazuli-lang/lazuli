---
title:   "Frontend wiring — how @lazuli/runtime reaches the web client"
slug:    frontend-wiring
sector:  lazuli-way
tier:    approved
created: 2026-06-01
updated: 2026-06-01
tags: [doctrine, frontend, web, runtime, vendor, tsconfig, vite, smoke-tests]
read_when: "the web client fails tsc --noEmit, a @lazuli/runtime import won't resolve, or you're upgrading the framework runtime"
---

# Frontend wiring

`lazuli new --frontends web` produces a web client that passes **both**
`tsc --noEmit` and `vite build` on the first `pnpm install` — no
hand-vendoring, no manual tsconfig paths. This doc is the contract that
makes that true, so you can keep it true (and fix it when it breaks).

The generated SDK (`dist/ts-web/<feature>/*.gen.ts`) imports
`@lazuli/runtime` and `@lazuli/runtime/react`. Two resolvers must agree
on where those live: **vite** (so the app runs) and **tsc** (so the
typecheck gate is honest). They resolve it differently, and getting only
one right is the defect spec 0027 fixed.

## The runtime is delivered as a vendored workspace package with prebuilt `.d.ts`

The TS runtime lives in the framework at `runtime/ts/lazuli`
(`@lazuli/runtime`), with sibling `@lazuli/vite` and `@lazuli/playwright`.
`lazuli new --frontends web` **copies their built output into the
project's `vendor/`**:

```
vendor/lazuli-runtime/   ← @lazuli/runtime  (dist/*.d.ts + dist/*.js + src)
vendor/lazuli-vite/      ← @lazuli/vite
vendor/lazuli-playwright/← @lazuli/playwright
```

Three things wire them in (all emitted by the scaffold — don't redo by hand):

1. **`pnpm-workspace.yaml`** lists `vendor/*`, so `@lazuli/runtime` is a
   real `workspace:*` member that `pnpm install` resolves with no
   network registry.
2. **`app/web/package.json`** depends on `"@lazuli/runtime": "workspace:*"`
   and `"@lazuli/vite": "workspace:*"` (NOT a published `^0.x` version —
   those are phantom and break `pnpm install`).
3. **`app/web/tsconfig.json`** `paths` point at the **prebuilt
   `dist/*.d.ts`**, never the `src/*.ts`:

   ```jsonc
   "@lazuli/runtime":                ["../../vendor/lazuli-runtime/dist/index.d.ts"],
   "@lazuli/runtime/react":          ["../../vendor/lazuli-runtime/dist/react.d.ts"],
   "@lazuli/runtime/react/tanstack": ["../../vendor/lazuli-runtime/dist/tanstack-adapter.d.ts"],
   "@lazuli/runtime/react/rhf":      ["../../vendor/lazuli-runtime/dist/react-rhf.d.ts"],
   "@lazuli/runtime/formatters":     ["../../vendor/lazuli-runtime/dist/formatters.d.ts"]
   ```

### Why `.d.ts`, not `src/*.ts` — the load-bearing reason

If the tsconfig `paths` point at the runtime **source** (`src/react.ts`,
…), tsc pulls that source into the consumer's program through the import
graph and typechecks it under the **vendor dir's** resolution root —
where `react` / `@tanstack/react-query` aren't reachable. Result: dozens
of `Cannot find module 'react'` errors that are NOT in your code. The
scaffold tsconfig already sets `skipLibCheck: true`, which **skips
`.d.ts` files** — so pointing `paths` at `dist/*.d.ts` means the consumer
never typechecks runtime internals at all. That is the whole fix.

`vite` resolves the other branch: `@lazuli/vite`'s `lazuliAliases()`
reads `Lazurite.toml [lazuli] path` at config-load time and aliases
`@lazuli/runtime` → the runtime source. So vite runs the app from source
while tsc checks against the prebuilt types. Both green, neither lying.

## The smoke tests are the gate

`lazuli new` emits two tests under `app/web/__smoke__/`:

- **`scaffold.smoke.test.tsx`** mounts `<App/>` through the live provider
  tree. If the `@lazuli/runtime*` import surface stopped resolving or
  compiling, this file would not typecheck.
- **`generated-sdk.smoke.test.ts`** imports the exact symbols every
  generated `*.react.gen.ts` pulls from the runtime. If the SDK's import
  surface broke, `tsc --noEmit` fails here.

Plus a `verify:scaffold` script (`tsc --noEmit && vite build`) chained
into the project's `verify:all`. Run it after any frontend-wiring change.

## When you upgrade the framework runtime

The vendored `dist` is a **snapshot** taken at `lazuli new` time. If you
edit `runtime/ts/lazuli` in the framework, re-build it
(`pnpm --filter @lazuli/runtime build`) and re-vendor into the pilot
(re-run the scaffold or copy `runtime/ts/{lazuli,vite,playwright}` →
`vendor/` — `lazuli new` only writes files that don't already exist, so
delete the stale `vendor/*/dist` first, or wait for `lazuli
upgrade-runtime`). A drifted vendor is the one staleness trap here.

## The dist filenames are real — verify, don't guess

The `paths` RHS must be a real emitted `.d.ts`. The runtime's `./react`
export declares its **`types` condition as `react.ts`** (the
platform-neutral type contract that both `react.web.ts` and
`react.native.ts` satisfy) — so the emitted type entry is **`react.d.ts`**,
NOT `react.web.d.ts`. The runtime builds via `tsc -p tsconfig.build.json`
(emit on); the standalone `tsconfig.json` (noEmit) stays the dev
typecheck. If a path points at a `.d.ts` the build doesn't emit, tsc
silently can't resolve the import — the `runtime_emits_dts` +
`scaffold_compiles_e2e` framework tests catch that.
