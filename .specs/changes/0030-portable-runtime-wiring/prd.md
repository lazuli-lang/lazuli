---
id: 0030
title: Portable runtime wiring — no absolute paths in generated go.mod/go.work
type: prd
stage: 3 of 3
status: ready
created: 2026-06-02
---

# PRD — Portable runtime wiring (no absolute disk paths)

## Problem
The generated Go module resolves the Lazuli runtime through a `replace lazuli.dev/runtime => <path>` that can be ABSOLUTE and machine-specific. Pauta's checked-in `go.mod:17` and `go.work:11` both read `replace lazuli.dev/runtime => C:/Users/lucas/lazuli/runtime/go` — a hand-added absolute path (pauta gap BT-01: codegen emitted NO replace, so the backend couldn't `go build` until the user pasted that absolute path in). Hostpoint's `go.work:21` is relative (`../lazuli/runtime/go`) but still assumes a sibling checkout and was also hand-wired. The codegen path that should produce a portable replace (`detect_runtime_dev_replace` in `crates/lazuli_cli/src/lazurite_codegen.rs:158`) only fires when the runtime sits in an ANCESTOR of the project — which is false for the canonical `dev/<pilot>` + `lazuli/` sibling layout, so it silently emits nothing and the developer falls back to pasting an absolute path. Nothing checks for or forbids absolute paths in the emitted artifacts.

## Why now (or why ever)
Every clone of pauta by a second developer (or CI) breaks on `go build`: `C:/Users/lucas/...` does not exist on their disk. The framework's flagship promise — `lazuli generate go .` then `go build` works — is false for any pilot that isn't on the original author's machine. If never fixed: pilots stay non-portable, every new contributor hand-edits go.mod/go.work, and the gap doc BT-01 stays open as a P1.

## Outcome — done means
- `lazuli generate go .` emits a `replace`/`go.work` that uses a PROJECT-ROOT-RELATIVE path to the runtime (the canonical sibling layout `../lazuli/runtime/go`), driven by an explicit `Lazurite.toml [lazuli] path` resolved relative to the project root.
- A fresh clone with the documented sibling layout builds with `go build ./...` and NO hand-editing — zero absolute paths, zero machine-specific assumptions in any generated artifact.
- `LAZULI_RUNTIME_PATH` env override remains the escape hatch for non-standard / CI layouts (it already exists in `runtime_wiring.rs:35`).
- A doctor rule + a codegen test assert the emitted `go.mod`/`go.work` contain NO absolute path.
- pauta + hostpoint are migrated: their go.mod/go.work no longer carry an absolute path; both `go build ./...` clean from a relative wiring.
- `cargo test --workspace` green; `docs/lazuli_way` (or the relevant runtime-wiring doc) teaches the sibling-layout + `[lazuli] path` contract.

## Non-goals
- Publishing/vendoring the runtime as a versioned proxy module — evaluated and rejected as the PRIMARY (see ADR); the runtime stays a local replace. (Vendoring may be a future spec; not here.)
- Supporting arbitrary non-sibling layouts WITHOUT an env override or explicit `[lazuli] path`. The portable default targets the documented sibling layout; everything else uses `LAZULI_RUNTIME_PATH` or an explicit relative `path`.
- Cross-drive (Windows different-drive) relativization — when no relative path exists, the env override is the answer; the rule still forbids baking an absolute path into the committed artifact.
- Changing the runtime module name (`lazuli.dev/runtime`) or the require-line mechanics.
- Touching the frontend/TS wiring.

## User stories
- As a second developer, I clone pauta + lazuli as siblings, run `go build ./...`, and it works — no path editing.
- As CI, I set `LAZULI_RUNTIME_PATH` and the build resolves the runtime without a committed absolute path.
- As the maker, the doctor refuses to let an absolute runtime path get committed into a generated artifact.

## Constraints
- The emitted `dist/go/go.mod` replace path is relative to `dist/go/` (two levels under the project root); the `go.work` path is relative to the project root. Both must be relative.
- Must run unattended; `cargo test --workspace` is the judge; the portability check is an automated test asserting no absolute path.
- Cannot break the existing workspace-mode require-line (`lazuli.dev/runtime v0.0.0`) mechanics.
- `lazuli_doctor` cannot depend on `lazuli_cli`.

## Open questions
None. Primary scheme decided (explicit relative `[lazuli] path`, project-root-relative emission; scaffold writes it). Fallback decided (`LAZULI_RUNTIME_PATH` env). Absolute-path detection decided (doctor rule + codegen test).
