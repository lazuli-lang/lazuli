Implemented the RHF adapter path and coverage.

What changed:
- Added/verified `useLazuliFormRHF` at `runtime/ts/lazuli/src/react-rhf.ts`.
- Added `./react/rhf` export plus RHF/Zod peer/dev deps in `runtime/ts/lazuli/package.json`.
- Added `runtime/ts/lazuli/src/react-rhf.test.tsx` with 9 tests covering defaults, hydrate-once, submit mapping, success, server field errors, generic errors, reset, and type shape.
- Added `STATUS.md` with a `HostPersonal.tsx` migration sketch.
- Updated `pnpm-lock.yaml`.

Verification:
- `pnpm -C runtime/ts/lazuli test` passed: 87 tests.
- `pnpm -C runtime/ts/lazuli typecheck` passed.
- `pnpm typecheck` at repo root still fails in generated `dist/web/customer/src/react.ts:34` and `:41` because `UseLazuliQueryOptions` is used without its required generic type argument. I did not edit `dist/` per repo rules.