---
id: 0030
title: Portable runtime wiring via explicit project-root-relative [lazuli] path, env fallback, and an absolute-path doctor guard
type: adr
status: accepted
created: 2026-06-02
supersedes: —
---

# ADR — The runtime is wired by an explicit relative `[lazuli] path` in `Lazurite.toml`, emitted project-root-relative into go.work/go.mod; `LAZULI_RUNTIME_PATH` is the fallback; a doctor rule + codegen test forbid absolute paths in generated artifacts.

## Context
- `emit_go_mod` emits `replace lazuli.dev/runtime => <dev_replace_runtime>` and `emit_go_work` emits a `use <dev_runtime_path>` (`crates/lazuli_codegen_go/src/emitter/module/go_mod.rs:144,182`). The path values come from `codegen_lazurite_manifest` (`crates/lazuli_cli/src/lazurite_codegen.rs:68-101`).
- That resolver prefers `Lazurite.toml [lazuli] path` (`manifest.lazuli.path`) and builds `../../<path>/runtime/go` (go.mod) + `<path>/runtime/go` (go.work) — these ARE relative when `path` is relative. GOOD. (lazurite_codegen.rs:76-87.)
- The fallback `detect_runtime_dev_replace` (lazurite_codegen.rs:158-179) walks ANCESTORS of `out_dir` for a `runtime/go`; for the canonical `dev/<pilot>` + `lazuli/` SIBLING layout there is no shared ancestor → returns `None` → no replace emitted → BT-01 (pauta couldn't build; user pasted an absolute path by hand).
- Neither pilot sets `[lazuli] path` (verified: pauta + hostpoint `[lazuli]` blocks carry only `runtime = "0.1.0"`). So both fell through to the broken ancestor heuristic and got hand-wired — pauta with an absolute path in BOTH go.mod and go.work, hostpoint with a relative go.work path.
- The scaffolder (`crates/lazuli_cli/src/commands/new/runtime_wiring.rs`) ALREADY does the right thing on `lazuli new`: it locates the runtime (`LAZULI_RUNTIME_PATH` → ancestor-of-binary), prefers a relative `go.work` entry, falls back to absolute only cross-drive, and writes `[lazuli] path` into `Lazurite.toml` (runtime_wiring.rs:84-150). But existing pilots were NOT scaffolded through this path / predate it, so they have no `[lazuli] path` and a broken/hand-edited wiring.
- `relative_path` (path_utils) already emits forward-slash relative output and is used by both the scaffolder and `detect_runtime_dev_replace`.

## Decision
1. **Primary: explicit relative `[lazuli] path` in `Lazurite.toml`, resolved project-root-relative.** The canonical wiring is `[lazuli] path = "../lazuli"` (sibling layout). `codegen_lazurite_manifest` already turns that into `../../../lazuli/runtime/go` for go.mod and `../lazuli/runtime/go` for go.work — both relative. We make this the DOCUMENTED, scaffolded default and STOP relying on the ancestor heuristic for the sibling case.
2. **Fix the fallback to never emit an absolute path.** When `[lazuli] path` is absent AND `detect_runtime_dev_replace` can only produce an absolute path (no relative path exists between project and runtime), emit NOTHING into the committed artifact and surface a loud diagnostic telling the author to set `[lazuli] path` or `LAZULI_RUNTIME_PATH`. An absolute path must never be baked into a generated, committed file.
3. **`LAZULI_RUNTIME_PATH` is the fallback** (already wired in scaffolding; extend so `lazuli generate go` also honors it when `[lazuli] path` is unset — it resolves the runtime for the build WITHOUT writing an absolute path into go.mod/go.work; the env is read at build time, not baked).
4. **Guardrail: `RUNTIME-WIRING-ABSOLUTE-PATH-001`** (doctor, Correctness category so it BLOCKS the generate gate — a committed absolute path genuinely breaks the build on any other machine). Fires when the project's committed `go.mod`/`go.work` carries a `replace lazuli.dev/runtime => <abs>` or `use <abs>` where `<abs>` is an absolute path (`C:\`/`C:/`/drive-letter/`/`-rooted/UNC). Plus a codegen unit test asserting the EMITTED go.mod/go.work strings contain no absolute path under the sibling layout.
5. **Scaffold writes the relative `[lazuli] path`** for new projects (already done by `runtime_wiring.rs`; verify + lock with a test). Existing pilots are migrated by setting `[lazuli] path = "../lazuli"` and regenerating.

## Alternatives considered
- **(c) Publish/vendor the runtime as a versioned proxy module** — rejected as PRIMARY: `lazuli.dev/runtime` is intentionally never published (runtime_wiring.rs:5 — developers edit it in-tree against the same checkout that built the CLI). A proxy tag would freeze the runtime and break the dogfooding loop. Vendoring (`go mod vendor`) bloats every pilot repo with a copy of the runtime and desyncs on every runtime edit. Local relative replace is the dumb, correct default for a co-developed framework.
- **(d) Env override as PRIMARY** — rejected as primary: an env var is invisible in the repo and easy to forget; a committed relative `[lazuli] path` is self-documenting and works on a fresh clone with the documented layout and no env setup. Env stays as the FALLBACK for non-standard/CI layouts.
- **(b) A fully generated `go.work` template with all relative paths** — partially adopted: `emit_go_work` ALREADY emits project-root-relative entries from the resolved path. The missing piece isn't a new template; it's making the path RESOLUTION reliably relative (decision 1+2). No need for a separate templating layer.
- **Keep the ancestor heuristic and just relativize harder** — rejected: the sibling layout has no shared ancestor, so the heuristic is structurally unable to find the runtime; relativizing an absolute path it found via some OTHER means is what produced the cross-drive footgun. Explicit `[lazuli] path` removes the guessing.
- **Make the absolute-path rule advisory** — rejected: a committed absolute path is a concrete build break on any other machine, not style. It belongs in the blocking Correctness set so the gate refuses to ship it.

## Consequences
**We accept:** pilots MUST declare `[lazuli] path` (or set `LAZULI_RUNTIME_PATH`) — a one-line `Lazurite.toml` addition. We accept the canonical layout is "lazuli as a sibling of the pilot" and document it as the contract. We accept that a truly cross-drive layout has no committed-relative answer and must use the env override (the rule forbids the absolute-path alternative).
**We gain:** a fresh clone of any pilot + a sibling lazuli checkout builds with no hand-editing; BT-01 closes; the framework's `generate → go build` promise holds across machines; the gate refuses to ship a non-portable artifact.
**We watch:** if a legitimate workflow genuinely needs an absolute path committed (none known), the rule's blocking severity reopens. If `[lazuli] path` resolution ever emits a non-relative value for a relative input, decision 2's "emit nothing + diagnose" must catch it (the codegen test pins this).
