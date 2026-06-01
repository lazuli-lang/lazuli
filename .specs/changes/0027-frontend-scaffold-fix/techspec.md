---
id: 0027
title: Frontend scaffold fix — runtime as workspace-vendored package with prebuilt .d.ts
type: techspec
track: ship (framework defect)
depends_on: []
parallel_safe: false
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_cli scaffold_web && cargo test --workspace"
agent: unassigned
---

# TechSpec — Frontend scaffold fix

## Approach
Three coupled framework edits + emitted smoke tests, all so `lazuli new --frontends web` produces a frontend where `tsc --noEmit` and `vite build` both pass: (1) the runtime emits prebuilt `dist/*.d.ts`; (2) the web scaffolder vendors the runtime as a workspace package; (3) the emitted tsconfig/package.json wire it correctly. Then the scaffold emits render + SDK smoke tests and a Rust integration test that proves the scaffold compiles. The oracle: a fresh scaffold + `pnpm install` + `tsc --noEmit` + `vite build` all exit 0.

## Surface
**Modify (framework):**
- `runtime/ts/lazuli/package.json` — `build` → real emit (`tsc -p tsconfig.build.json`); `main`→`./dist/index.js`, `types`→`./dist/index.d.ts`; exports-map `types` conditions → `./dist/*.d.ts`; add `"files": ["dist","src"]`.
- `runtime/ts/lazuli/tsconfig.build.json` — NEW: `extends ./tsconfig.json`, `noEmit:false`, `declaration:true`, `declarationMap:true`, `emitDeclarationOnly:false`, `outDir:"dist"`.
- `crates/lazuli_cli/src/templates/web/shell.rs`:
  - `FRONTEND_TSCONFIG_JSON` (~221-251) — add 5 `@lazuli/runtime*` paths → `../../../vendor/lazuli-runtime/dist/*.d.ts` (index, react.web, tanstack-adapter, react-rhf, formatters). Keep `@app/*`/`@generated/*`/`@web/*`.
  - `FRONTEND_PACKAGE_JSON` (~312-363) — `@lazuli/runtime`/`@lazuli/vite` `^0.1.0` → `workspace:*` (or `file:../../../vendor/lazuli-*`); add `"search-query-parser": "^1.6.0"` to deps + `"@types/node": "^22.0.0"` to devDeps; add `"verify:scaffold": "tsc --noEmit && vite build"` script.
  - NEW consts `FRONTEND_WEB_SMOKE_TEST_TSX` + `FRONTEND_WEB_SDK_SMOKE_TEST_TS` (the smoke tests).
  - `FRONTEND_WEB_ROOT_TSX` (~56) — `import.meta.env.VITE_API_URL` → `import.meta.env.PUBLIC_API_URL ?? "/api"` (env-prefix canon).
- `crates/lazuli_cli/src/cmd_new_frontends/web/mod.rs::scaffold_frontend_web` (~58-198) — NEW vendor step: copy `<lazuli-path>/runtime/ts/{lazuli,vite,playwright}` (dist+package.json) → project `vendor/`, idempotent (`write_if_absent`/copy-if-absent). Emit the two smoke test files (alongside ~195).
- `lazurite/templates/default/pnpm-workspace.yaml.tmpl` — add `- "vendor/*"` to `packages`.
- `lazurite/templates/default/package.json.tmpl` (~25) — chain `verify:scaffold` into `verify:all`.
- `lazurite/templates/default/knowledge/lazuli-way/` — NEW `0016-frontend-wiring.md` documenting the runtime-delivery contract.

**Create (tests):**
- `crates/lazuli_cli/src/cmd_new_frontends/web/mod.rs` `mod tests` (~after 332) — a scaffold-then-compile integration test (see Plan step 7 for the CI-feasibility fallback).

## Contracts
**Runtime delivery contract (frozen):** `@lazuli/runtime` is vendored at `<project>/vendor/lazuli-runtime/` as a workspace member (`pnpm-workspace.yaml` `vendor/*`), consumed by the client via `"@lazuli/runtime": "workspace:*"` + tsconfig `paths` pointing at `vendor/lazuli-runtime/dist/*.d.ts`. The consumer NEVER typechecks runtime `src` (`.d.ts` is `skipLibCheck`-skipped). `@lazuli/vite` handles the vite-side alias (unchanged).

**The 5 tsconfig path entries (exact):**
```
"@lazuli/runtime":                ["../../../vendor/lazuli-runtime/dist/index.d.ts"],
"@lazuli/runtime/react":          ["../../../vendor/lazuli-runtime/dist/react.web.d.ts"],
"@lazuli/runtime/react/tanstack": ["../../../vendor/lazuli-runtime/dist/tanstack-adapter.d.ts"],
"@lazuli/runtime/react/rhf":      ["../../../vendor/lazuli-runtime/dist/react-rhf.d.ts"],
"@lazuli/runtime/formatters":     ["../../../vendor/lazuli-runtime/dist/formatters.d.ts"]
```
(VERIFY the exact dist filenames the runtime build emits — `react.web` vs `react`; match what `runtime/ts/lazuli/src` actually exports. The path RHS must be a real emitted `.d.ts`.)

**Smoke test contract:** `app/web/__smoke__/scaffold.smoke.test.tsx` renders `<App/>` (from `@web/shell/root`) inside a `LazuliProvider` (from `@lazuli/runtime/react`) via `@testing-library/react`, asserts it mounts. `app/web/__smoke__/generated-sdk.smoke.test.ts` imports one symbol from a generated `@generated/<feature>/<feature>.react.gen` and asserts it's defined. Both are emitted by the scaffolder; both fail if the runtime/ SDK wiring breaks.

## Plan — for the executing agent
1. Read `/c/tmp/scaffold-rootcause.jsonl` (the full investigation — the 11-item repair checklist with file:line). Read `runtime/ts/lazuli/{package.json,tsconfig.json,src/}` (confirm the dist filenames), `crates/lazuli_cli/src/templates/web/shell.rs` (the emitted tsconfig + package.json consts), `cmd_new_frontends/web/mod.rs` (the scaffolder), hostpoint's `app/clients/hostpoint-app/tsconfig.json` (the hand-wired template that works for vite).
2. **Runtime dist build:** add `tsconfig.build.json`, fix `package.json` build+main+types+exports. Run `cd runtime/ts/lazuli && pnpm install && pnpm build` (or the workspace equivalent) — confirm `dist/*.d.ts` emit for index/react/tanstack/rhf/formatters. Confirm the runtime STILL typechecks standalone (`tsc --noEmit` in its own dir, with its devDeps installed).
3. **Scaffold tsconfig + package.json:** edit the shell.rs consts per Surface. Get the path filenames RIGHT (match the emitted dist).
4. **Vendor step:** add the copy-runtime-into-`vendor/` logic to `scaffold_frontend_web`, idempotent. Add `vendor/*` to the workspace template.
5. **Smoke tests:** add the two emitted test consts + wire their writes. Add `verify:scaffold` to the package.json + `verify:all` chain.
6. **Teach:** the `0016-frontend-wiring.md` knowledge doc.
7. **Rust guardrail:** add the integration test. FEASIBILITY: running `pnpm install + tsc + vite build` inside a `cargo test` may be too slow/networked for CI. DECISION: write it to (a) scaffold to a tempdir, (b) assert the emitted tsconfig contains the 5 runtime paths, the package.json has NO `^0.1.0` phantom deps + has search-query-parser, the smoke files exist, and `vendor/lazuli-runtime/dist` was copied — a STRUCTURAL proxy that's fast + deterministic. Gate the FULL pnpm+tsc+vite version behind an opt-in env (`LAZULI_E2E_SCAFFOLD=1`) so it can run in a heavy CI lane but not block the unit suite. Document this.
8. **LIVE PROOF:** `cargo run -p lazuli_cli -- new /tmp/scaffold-demo --frontends web`, then `cd /tmp/scaffold-demo && pnpm install && pnpm --filter ./app/web exec tsc --noEmit && pnpm --filter ./app/web exec vite build`. BOTH must exit 0. Capture the output. Clean up the demo.
9. **GATE:** `cargo test -p lazuli_cli scaffold_web` + **`cargo test --workspace`** (0 failures) + `cargo build --workspace` + the live proof (step 8).
10. Commit on `loop-serial`. (Pilot re-wiring is a SEPARATE step — this spec fixes the FRAMEWORK; the pilots get fixed after, reusing the corrected delivery.)

## Tests first (TDD)
- [ ] `runtime_emits_dts` — after the runtime build, `dist/index.d.ts` + the 4 sub-entry `.d.ts` exist (a build-output assertion).
- [ ] `scaffold_tsconfig_has_runtime_paths` — the emitted `app/web/tsconfig.json` contains all 5 `@lazuli/runtime*` path entries pointing at `dist/*.d.ts`.
- [ ] `scaffold_package_json_no_phantom_deps` — the emitted `app/web/package.json` has NO `"^0.1.0"` for `@lazuli/runtime`/`@lazuli/vite` (uses `workspace:*`/`file:`) AND includes `search-query-parser` + `@types/node`.
- [ ] `scaffold_emits_smoke_tests` — `app/web/__smoke__/scaffold.smoke.test.tsx` + `generated-sdk.smoke.test.ts` are written.
- [ ] `scaffold_vendors_runtime` — `vendor/lazuli-runtime/dist/index.d.ts` exists in the scaffolded project + `pnpm-workspace.yaml` lists `vendor/*`.
- [ ] `scaffold_compiles_e2e` (opt-in `LAZULI_E2E_SCAFFOLD=1`) — scaffold + pnpm install + tsc --noEmit + vite build all exit 0.

## Gate

### Definition of Done (framework-defect gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep)** + scaffold structural tests green.
2. PROVE: live `lazuli new --frontends web` + `pnpm install` → `tsc --noEmit` AND `vite build` both exit 0 (captured).
3. TEACH: `0016-frontend-wiring.md` documents the delivery contract; the scaffold emits smoke tests that teach-by-example.
4. ENFORCE: the structural scaffold tests + the opt-in e2e test + the emitted smoke tests prevent regression — the defect can't ship again.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_cli scaffold_web` (all structural TDD) + `cargo test --workspace` 0 failures.
2. **PROVE** — the live scaffold's tsc + vite build both exit 0 (report the actual output, before/after the fix).
3. **TEACH** — wiring doc + emitted smoke tests present.
4. **ENFORCE** — the structural tests assert paths/deps/smoke-files; the opt-in e2e proves full compile.

## Risks & rollback
- **The dist `.d.ts` filenames don't match the `paths`** → tsc still can't resolve → mitigation: step 2 confirms the emitted dist names; the `runtime_emits_dts` + `scaffold_compiles_e2e` tests catch a mismatch.
- **The runtime build breaks the runtime's own standalone typecheck** → mitigation: keep `tsconfig.json` (noEmit, for dev typecheck) separate from `tsconfig.build.json` (emit); run both in step 2.
- **The Rust e2e test is too slow/networked for CI** → mitigation: the structural proxy is the default gate; the full e2e is opt-in (`LAZULI_E2E_SCAFFOLD=1`), documented.
- **Vendored dist goes stale** → out of scope here (a `lazuli upgrade-runtime` follow-up); the scaffold copies fresh on `new`.

**Rollback:** `git revert` — the runtime build + scaffold-const edits + vendor step + emitted tests are additive; reverting restores today's (broken-but-vite-builds) scaffold. No pilot is touched by this spec.
