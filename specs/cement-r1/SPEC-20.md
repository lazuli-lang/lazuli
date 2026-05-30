# SPEC-20 — CLI surface hygiene: split framework-dev commands off the published `lazuli` binary

**Breaking:** false (additive split; published commands unchanged) | **Est. commits:** 3 | **Depends on:** none

## Problem
The published `lazuli` binary carries framework-*development* commands that mean
nothing to an app developer and pollute `lazuli --help`: `parse` (AST dump),
`spike-generate` (runtime codegen spike), `examples` (manages THIS repo's
fixtures), and the `doctor --self` flag (lints Lazuli's own Rust under
`INTERNAL-*`). App developers using Lazuli in their own projects should not see
these. (`lazuli upgrade` is NOT in this set — it migrates a *user's* project to
a new framework version and stays.)

## End state
Three binaries with clean audiences:
- **`lazuli`** (published) — app-dev only: `new, check, doctor <project>,
  generate, inspect, upgrade, test, fmt, dev, seed, migrate, translate, plan,
  lsp, mcp, init, changelog`.
- **`xtask`** (repo-only, pure projector — deps only `lazuli_keywords`) — the
  generators: `gen-tmlanguage`, `gen-keyword-reference`, `gen-catalog-reference`,
  `docs-staleness`. Unchanged.
- **`lazuli-dev`** (NEW, repo-only — may dep the heavy doctor/syntax crates) —
  `self-doctor` (today's `doctor --self`), `parse`, `debug`(?), `spike-generate`,
  `examples`. Contributors run `cargo run -p lazuli-dev -- <cmd>`.
- **`scripts/dev-check.sh`** — bash ORCHESTRATOR (not logic): sequences `cargo
  fmt --check` + `cargo clippy` + `xtask … --check` (tmlanguage/keyword/catalog
  freshness) + `cargo test … docs_hygiene` + `lazuli-dev self-doctor`.

Rationale for the mechanism: the self-investigation LOGIC is Rust (it IS the
doctor engine) — bash can only invoke it, not host it. So the logic moves to a
non-published Rust binary; bash is the convenience orchestrator. `xtask` stays
pure so its dependency boundary doesn't pull in the doctor crates.

## Done so far (commit 1)
The clearly-internal commands are `#[command(hide = true)]` so they vanish from
`lazuli --help` while still functioning for contributors (`parse`,
`spike-generate`, `examples`, and the `doctor --self` flag). This is the safe,
zero-risk first step. Verified: `lazuli --help` shows ~17 app-dev commands;
`lazuli parse --help` still works.

## Remaining
1. Create `tools/lazuli-dev` (or `crates/lazuli_devcli`) binary; move the
   `parse` / `spike-generate` / `examples` handlers + extract the `doctor --self`
   path into `lazuli-dev self-doctor`. Delete those variants from `lazuli_cli`.
2. `scripts/dev-check.sh` orchestrator + a contributor-tooling note in CLAUDE.md.
3. Acceptance: `lazuli --help` contains zero framework-dev commands; `cargo run
   -p lazuli-dev -- self-doctor` reproduces `doctor --self`; `dev-check.sh` green.

## Justification
A published CLI is a product surface. Every framework-dev command in it is
cognitive noise for every app developer (and a maintenance/compat obligation).
Splitting them out shrinks the surface an external user must understand and frees
the framework to change its internal tooling without CLI-compat concerns.
