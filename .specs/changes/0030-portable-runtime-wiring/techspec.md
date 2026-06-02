---
id: 0030
title: Portable runtime wiring — no absolute paths in generated go.mod/go.work
type: techspec
status: ready
created: 2026-06-02
depends_on: []
parallel_safe: true
test_gate: "cargo test --workspace"
agent: unassigned
---

# TechSpec — Portable runtime wiring (no absolute disk paths)

## Approach
Make the runtime wiring portable in three moves, all on existing seams. (1) Adopt explicit relative `[lazuli] path = "../lazuli"` as the scaffolded + documented default; the resolver in `codegen_lazurite_manifest` already turns that into relative go.mod/go.work paths — no new templating. (2) Harden the fallback so it NEVER bakes an absolute path into a committed artifact: when only an absolute path is available, emit nothing + diagnose loudly, and honor `LAZULI_RUNTIME_PATH` at build time. (3) Guard with `RUNTIME-WIRING-ABSOLUTE-PATH-001` (doctor, Correctness → blocks the gate) + a codegen unit test asserting the emitted go.mod/go.work carry no absolute path. Migrate both pilots to the relative wiring. Dumb and functional: reuse `relative_path`, the existing `[lazuli] path` resolution, and the existing env override; add one detection predicate + one rule + one test.

## Surface
**Create:**
- `crates/lazuli_doctor/src/correctness/runtime_wiring_absolute_path_001.rs` — `RUNTIME-WIRING-ABSOLUTE-PATH-001`. Scans the project's committed `go.mod` + `go.work` for a `replace lazuli.dev/runtime => <p>` or a `use <p>` where `<p>` `is_absolute_path` (drive-letter `X:[\/]`, leading `/`, or UNC `\\`). Correctness category → blocks the generate gate. Message: set `[lazuli] path = "../lazuli"` (relative) or `LAZULI_RUNTIME_PATH`.
- `crates/lazuli_codegen_go/tests/emit_v1/tests_portable_wiring.rs` (or extend `tests_go_mod.rs`) — assert emitted go.mod/go.work under a relative `[lazuli] path` contain `../` and NO absolute path.

**Modify:**
- `crates/lazuli_cli/src/lazurite_codegen.rs` — in `codegen_lazurite_manifest` / `detect_runtime_dev_replace`: (a) when the resolved go.mod or go.work path would be ABSOLUTE (no relative path exists), return `None` for that side and emit a `eprintln!`/diagnostic instructing the author to set `[lazuli] path` or `LAZULI_RUNTIME_PATH` — do NOT pass an absolute string downstream; (b) when `[lazuli] path` is unset, consult `LAZULI_RUNTIME_PATH` (via the existing `locate_lazuli_runtime_dir` in `commands/new/runtime_wiring.rs`, lifted/shared) to RESOLVE the build without baking the path — i.e. relativize the env-resolved dir against project-root/out-dir and only emit if the result is relative, else emit nothing + diagnose. Add an `is_absolute_path`/`is_relative` guard helper (or reuse `Path::is_absolute` post-`relative_path`).
- `crates/lazuli_codegen_go/src/emitter/module/go_mod.rs` — defensive: when `dev_replace_runtime` / `dev_runtime_path` IS absolute (belt-and-suspenders), the emitter still emits it (it's a string) BUT the CLI layer must never hand it one; document the invariant in the module doc + rely on the doctor rule + test as the backstop. (No behavior change here unless the agent finds the emitter is the cleaner enforcement point — if so, skip the emit + log, and pin with the codegen test.)
- `crates/lazuli_doctor/src/lib.rs` + `correctness/mod.rs` — register `RUNTIME-WIRING-ABSOLUTE-PATH-001`; add the diagnostics-registry bridge entry (new doctor code); module_headers trigger-cue on the new rule module.
- `crates/lazuli_cli/src/commands/new/runtime_wiring.rs` — verify (and lock with a test) that the scaffold writes a RELATIVE `[lazuli] path` for the sibling layout; if it currently can write absolute (cross-drive branch at line 102-109), keep that branch ONLY for go.work-on-the-author-machine but ensure `[lazuli] path` written into Lazurite.toml is relative-or-omitted (never absolute committed).

**Teach:**
- `docs/lazuli_way/runtime-wiring.md` (new, or append to the closest existing runtime/deploy doc — grep `docs/lazuli_way` for runtime) — the portable contract: clone lazuli as a sibling of the pilot; set `[lazuli] path = "../lazuli"`; `lazuli generate go .` emits relative wiring; CI uses `LAZULI_RUNTIME_PATH`; absolute paths are forbidden (`RUNTIME-WIRING-ABSOLUTE-PATH-001`).
- scaffold `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — bullet (byte-identical region): "Runtime is wired via relative `[lazuli] path = \"../lazuli\"` (sibling layout) or `LAZULI_RUNTIME_PATH`. Never commit an absolute `replace lazuli.dev/runtime => C:/...` (`RUNTIME-WIRING-ABSOLUTE-PATH-001`)."

## Contracts
**`RUNTIME-WIRING-ABSOLUTE-PATH-001` (Correctness; BLOCKS the generate gate):**
- Fires once per absolute `lazuli.dev/runtime` wiring found in the project's `go.mod` or `go.work`.
- `is_absolute_path(p)`: matches `^[A-Za-z]:[\\/]` (Windows drive), `^/` (POSIX root), `^\\\\` (UNC).
- Suppressible via `@doctor.allow(RUNTIME-WIRING-ABSOLUTE-PATH-001, reason: "...")` (0028) — but blocking, so a reason is required (`DOCTOR-ALLOW-NO-REASON-001`).
- Message names the fix: relative `[lazuli] path` or `LAZULI_RUNTIME_PATH`.

**Emission invariant (codegen + CLI):** the strings emitted into `dist/go/go.mod` (`replace lazuli.dev/runtime => <p>`) and root `go.work` (`use <p>`) MUST be relative or absent. The CLI resolver (`codegen_lazurite_manifest`) is responsible for never producing an absolute `<p>`; the emitter trusts its input; the doctor rule + codegen test are the backstop.

**Resolution order (build-time runtime path, CLI):**
1. `Lazurite.toml [lazuli] path` (relative) → relative go.mod/go.work paths. (Primary.)
2. `LAZULI_RUNTIME_PATH` env → relativize against project root / out dir; emit only if relative, else emit nothing + diagnose. (Fallback.)
3. Ancestor heuristic (`detect_runtime_dev_replace`) → only when it yields a RELATIVE path; absolute result → emit nothing + diagnose.

## Plan — for the executing agent
1. Read `lazurite_codegen.rs` (`codegen_lazurite_manifest`, `detect_runtime_dev_replace`), `emitter/module/go_mod.rs` (`emit_go_mod`, `emit_go_work`), `commands/new/runtime_wiring.rs` (`locate_lazuli_runtime_dir`, `inject_runtime_into_go_work`), `path_utils` (`relative_path`, `absolutize_*`), `tests/emit_v1/tests_go_mod.rs`, `gate.rs` (Correctness blocks).
2. Add `is_absolute_path` helper (shared between CLI resolver guard + doctor rule, or duplicated tiny pure fn). Unit-test it (drive-letter, POSIX root, UNC, relative negatives).
3. Harden `codegen_lazurite_manifest`/`detect_runtime_dev_replace`: never return an absolute path; when only absolute is available, return `None` + `eprintln!` the fix hint. Add `LAZULI_RUNTIME_PATH` consult (relativized) for the `[lazuli] path`-unset case.
4. Add the codegen test: build a manifest with `[lazuli] path = "../lazuli"`-equivalent (`dev_replace`/`dev_work_replace` via the builder, mirroring `emit_go_mod_with_dev_replace_*`), assert emitted go.mod replace + go.work use are relative (`contains("../")`, `!is_absolute_path`).
5. Create `runtime_wiring_absolute_path_001.rs` (Correctness, blocks gate); register it; diagnostics-registry bridge entry; module_headers trigger-cue. Unit-test fires-on-absolute / silent-on-relative / silent-when-no-runtime-replace.
6. Verify/lock the scaffold writes a relative `[lazuli] path` (test against a temp sibling layout); ensure no absolute path is committed by `lazuli new`.
7. Run `cargo test --workspace` (FULL sweep).
8. MIGRATE pilots: set `[lazuli] path = "../lazuli"` in pauta-web + hostpoint `Lazurite.toml`; delete the hand-added absolute `replace`/`use` lines; run `lazuli generate go .` (emits relative wiring + gate passes — the absolute-path rule now clean); `go build ./...` clean in both. Confirm `RUNTIME-WIRING-ABSOLUTE-PATH-001` is GREEN on both (no absolute path remains).
9. TEACH: write `docs/lazuli_way/runtime-wiring.md` (sibling-layout + `[lazuli] path` + env fallback + the rule) + scaffold bullet in BOTH `.tmpl`s (byte-identical region).

## Tests first (TDD)
- [ ] `is_absolute_path::matches_windows_drive` — `C:/x`, `C:\x` → true.
- [ ] `is_absolute_path::matches_posix_and_unc` — `/x`, `\\\\host\\share` → true.
- [ ] `is_absolute_path::relative_is_false` — `../lazuli/runtime/go`, `./x`, `runtime/go` → false.
- [ ] `codegen::emit_go_mod_replace_is_relative_under_lazuli_path` — manifest with relative dev_replace → go.mod `replace lazuli.dev/runtime => ../../../lazuli/runtime/go` (or equiv), `!is_absolute_path`.
- [ ] `codegen::emit_go_work_use_is_relative` — go.work `use` entry is relative.
- [ ] `codegen::no_absolute_path_in_any_emitted_artifact` — sweep emitted go.mod + go.work strings, assert none match `is_absolute_path` for the runtime line.
- [ ] `resolver::absolute_only_emits_nothing` — when resolution can only produce an absolute path, `dev_replace`/`dev_work_replace` come back `None` (no absolute string downstream).
- [ ] `resolver::env_override_relativized` — `LAZULI_RUNTIME_PATH` set + `[lazuli] path` unset → relative emission when a relative path exists.
- [ ] `runtime_wiring_absolute_path_001::fires_on_absolute_replace` — `go.mod` with `replace lazuli.dev/runtime => C:/Users/.../runtime/go` → finding (blocking).
- [ ] `runtime_wiring_absolute_path_001::silent_on_relative` — `=> ../../../lazuli/runtime/go` → no finding.
- [ ] `runtime_wiring_absolute_path_001::silent_when_no_runtime_replace` — go.mod with no runtime replace → no finding.
- [ ] `scaffold::writes_relative_lazuli_path` — `lazuli new` into a temp sibling layout writes a relative `[lazuli] path`, not absolute.

## Gate
`cargo test --workspace` green **and** a fresh-clone simulation builds: with `[lazuli] path = "../lazuli"` set, `lazuli generate go .` emits relative wiring, passes the doctor gate (`RUNTIME-WIRING-ABSOLUTE-PATH-001` clean), and `go build ./...` succeeds in BOTH pauta-web and hostpoint **and** neither pilot's committed go.mod/go.work contains an absolute runtime path **and** `docs/lazuli_way/runtime-wiring.md` + both scaffold `.tmpl`s teach the contract.

### Definition of Done (the repo's governing rule — embedded)
1. BUILD — `cargo test --workspace` green (FULL sweep); the codegen no-absolute-path test green.
2. MIGRATE — pauta-web + hostpoint: `[lazuli] path = "../lazuli"` set, absolute lines removed, `lazuli generate go .` gate-passes, `go build ./...` clean.
3. TEACH — `docs/lazuli_way/runtime-wiring.md` teaches the sibling-layout + `[lazuli] path` + env fallback; scaffold CLAUDE.md/AGENTS.md bullet.
4. ENFORCE — `RUNTIME-WIRING-ABSOLUTE-PATH-001` fires on an absolute committed wiring + the codegen test asserts emitted artifacts are relative.
Plus: diagnostics-registry bridge for the new doctor code; module_headers trigger-cue on the new rule module. (No keyword change → no parser↔registry parity / xtask keyword-reference work.)

## Risks & rollback
- Some legitimate layout truly has no relative path (cross-drive) → mitigation: `LAZULI_RUNTIME_PATH` fallback resolves the build without committing a path; the rule only forbids the COMMITTED absolute, not the env-resolved build.
- Making the rule blocking (Correctness) surprises a pilot mid-flight → mitigation: pilots are migrated to relative wiring IN THIS SPEC before the rule lands green; `@doctor.allow` is the documented per-finding escape.
- Hardening the resolver to return `None` silently breaks a project that RELIED on the absolute fallback → mitigation: the `None` path emits a loud `eprintln!` fix hint; the codegen test pins relative-only output; pilots are re-verified `go build`.
- Pauta lives at `dev/pauta-web-monorepo` (not a direct sibling of `lazuli/`) → the relative path is `../../lazuli` not `../lazuli`; the resolver computes it via `relative_path` — verify the actual emitted value builds, don't hardcode `../lazuli` in the pilot's `[lazuli] path` (use the correct relative depth).

**Rollback:** `git revert` the framework commit (the rule + resolver guard are additive; reverting restores the prior emit-or-nothing behavior). Pilot `Lazurite.toml`/go.mod/go.work migrations revert separately; the absolute path they had before still "worked" on the author's machine.
