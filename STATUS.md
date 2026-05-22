# Wave A.6 Status

Implementation commit: `6df2fd13986217902e998b56c761adb7934b0289`

Pluralizer tests: 3 unit tests, 15 assertions.

Renamed exports observed in pilot regen: 26.

Deprecation aliases observed in pilot regen: 26.

Rust verification:
- `cargo test -p lazuli_codegen_ts`: pass.
- `cargo test -p lazuli_cli`: pass.
- `git diff --check`: pass.
- `cargo fmt --check --package lazuli_codegen_ts --package lazuli_cli`: blocked by pre-existing formatting drift in unrelated files.

Cross-pilot probe:
- Hostpoint: `lazuli generate ts .` wrote 28 files. Generated 25 deprecation aliases, including `listCustomServiceCategorys`, `listPropertys`, `listPendingBasicDetailsHostsHosts`, and `listMineTransactionsAsHostOperationss`. `pnpm app:typecheck` and `pnpm os:typecheck` still fail on existing `QuerySpec` passed to `useLazuliCommand` callsites; no missing-export errors observed.
- Pleiades: `lazuli generate ts .` wrote 23 files. Generated 0 deprecation aliases. `pnpm install --frozen-lockfile` is blocked by missing local dependency `C:\Users\lucas\lazuli\runtime\web\lazuli`; `pnpm exec tsc --noEmit` reports dependency/module-resolution failures, not renamed-export failures.
- Erudito: `lazuli generate ts .` from `apps/api` wrote 2 files and exited 0, with pre-existing parser skips for several `query.list` feature files. No TS consumer/tsconfig is present in the repo, so `tsc --noEmit` is not applicable.
- Atelier: `lazuli generate ts .` from `apps/api` wrote 16 files. Generated 1 deprecation alias. No TS consumer/tsconfig is present in the repo, so `tsc --noEmit` is not applicable.
