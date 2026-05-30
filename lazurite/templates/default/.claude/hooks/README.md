# Validation harness — Claude Code hooks

This directory wires a **two-level validation harness** into every AI coding
session on this project. It is the Lazuli equivalent of the well-known
"post-edit `tsc --noEmit` + a `test`/coverage acceptance gate" pattern for
TypeScript agents.

> **Philosophy.** The quality of what an agent delivers is proportional to the
> quality of the validation environment you put around it. An agent left to
> self-report "done" will ship broken `.lzi`, missing handlers, and uncovered
> behavior. An agent that physically *cannot* take the next step while a file
> is broken — and *cannot stop* while the acceptance gate is red — converges on
> correct work. These hooks are that environment, checked into the repo so it
> travels with the project instead of living in one engineer's head.

The harness has two independent levels:

| Level | Hook event | Script | What it runs | How it blocks |
|---|---|---|---|---|
| 1. Immediate post-edit check | `PostToolUse` (Write/Edit/MultiEdit) | `lazuli-check.sh` | `lazuli check <edited file>` (fast, single-file) | **exit 2 + stderr** → the agentic loop halts before the next step until the parse/analyzer error is fixed |
| 2. Acceptance gate on stop | `Stop` | `lazuli-gate.sh` | `lazuli doctor` (strict + coverage, test-discipline hard-block) **+** `lazuli test --layer handler` coverage gate once handlers exist | **exit 0 + `{"decision":"block","reason":…}`** → Claude refuses to stop and keeps fixing |

Both are configured in [`../settings.json`](../settings.json).

---

## Level 1 — immediate post-edit check (`lazuli-check.sh`)

Fires after **every** `Write` / `Edit` / `MultiEdit`. It reads the tool
payload on stdin, pulls out the edited file path, and — **only** for
`.lzi` / `.lzx` sources — runs:

```bash
lazuli check <edited-file> --security-profile prototype --allow-version-mismatch
```

Why these exact flags:

- **Single file, not `.`** — `lazuli check <file>` is ~30 ms and needs no
  project context, so it's cheap enough to run on every edit. (The project's
  `pnpm lazuli:check` script targets the whole tree; the per-edit loop wants
  just the file that changed.)
- **`--security-profile prototype`** — the loosest profile, minimal escalation
  noise. `check` ignores the `Lazurite.toml [doctor] profile` and always
  defaults to `strict`, so the loosest profile is passed explicitly to avoid
  punishing an in-progress edit.
- **`--allow-version-mismatch`** — a runtime-pin drift in `Lazurite.toml` never
  blocks an unrelated edit.

`check` exits non-zero **only on error-severity diagnostics** — warnings and
hints never fire the gate, so a half-written file isn't punished for being
incomplete, only for being *broken*. On an error the hook prints the exact
`path:line:col: error [CODE]: message` lines to stderr and exits `2`, which
Claude Code surfaces to the agent and halts the loop **before the next
action**. The result: no broken `.lzi`/`.lzx` survives a single step.

What level 1 deliberately does **not** catch (single-file by design): missing
`uses`, a referenced handler `.go` that doesn't exist yet, registry env
cross-checks, coverage. Those are completeness concerns — level 2 owns them.

## Level 2 — acceptance gate on stop (`lazuli-gate.sh`)

Fires when the agent tries to **finish**. Runs the project-wide gate and, if
it's red, returns a `block` decision so the agent cannot declare "done":

```bash
lazuli doctor . --security-profile strict --coverage --fail-on category:TestDiscipline
```

Under the `strict` profile most discipline findings are **warnings** (a bare
`doctor` would exit 0 on them), so the explicit `--fail-on category:TestDiscipline`
is what makes the test-discipline family actually block — this mirrors the
`pnpm lazuli:doctor:gate` script. The doctor `spec` surface already exercises
the spec / actor-matrix / predicate / transition rules, so this is a meaningful
acceptance gate on its own, with no codegen required (~0.3 s).

**Infinite-loop guard.** When a `Stop` hook blocks, Claude Code re-enters the
`Stop` event with `stop_hook_active=true`. The gate checks that flag and
allows the stop on the *second* pass, so a gate the agent genuinely can't
satisfy can't trap the session forever. (Without this guard, block → continue
→ still red → block → … loops until the context limit.)

### Handler tests + coverage gate (on by default; auto-skipped until you have handlers)

Once the project has handler `.go` files, the gate **also** enforces Go
coverage — the "coverage forces iterations" mechanism. A coverage number is
only a real gate if falling below it *blocks*, so the gate runs:

```bash
lazuli generate go . --out dist/go                                   # generate first…
lazuli test . --layer handler --coverage \
  --fail-on coverage:handler_go=${LAZULI_GATE_HANDLER_COVERAGE:-90}  # …then gate on coverage %
```

`lazuli doctor`'s `--coverage` is only a *report* (it never changes doctor's
exit code); the `test` runner is the surface that owns the
`coverage:<metric>=<N>` gate, so coverage can only *block* from here. The bar
defaults to **90** (`handler_go`), matching the `tdd-strict`
`[doctor.coverage]` handler threshold.

Three robustness rules keep this from ever wedging a legitimately-early
session:

- **Auto-skipped while there are no handler `.go` files** (`app/features/**/handlers/*.go`).
  A fresh project with only `.lzi`/`.lzx` isn't punished — the coverage gate
  switches itself on the moment you write your first handler.
- **Codegen failure only warns.** `lazuli test --layer handler` shells out to
  `go test` against the **generated** `dist/go/` tree, so the gate regenerates
  first; if that codegen fails (often an offline `go mod`), the gate prints a
  warning and *skips* this round rather than blocking on an environment hiccup.
- **Disable / tune via env.** `LAZULI_GATE_RUN_TESTS=0` turns the whole tier
  off; `LAZULI_GATE_HANDLER_COVERAGE=<pct>` moves the bar. Expect multi-second
  latency on each stop once this tier is active (codegen + `go test`).

---

## Tuning

**Make the gate stricter / looser via `Lazurite.toml`.** The hooks honor your
manifest — they do not hard-code policy beyond the `--fail-on` clause:

```toml
[doctor]
profile = "strict"        # prototype | strict | production | iron-hand

[doctor.coverage]
preset = "tdd-strict"     # tdd-strict | tdd-mature | tdd-iron-hand | …
```

- Bump `profile` to `production` or `iron-hand` to escalate warnings → errors
  (then even a bare `doctor` blocks; the gate gets stricter automatically).
- Bump the coverage `preset` (or add per-layer `[doctor.coverage.<layer>]`
  overrides) to raise block thresholds. Note: doctor's `--coverage` report is
  *informational* and does not change doctor's exit code — the coverage *block*
  comes from the handler-tests tier (above), which gates on the `test` runner,
  the surface that owns the `coverage:<metric>=<N>` gate. Override the gate's
  bar without touching the manifest via `LAZULI_GATE_HANDLER_COVERAGE`.

**Add more hard-blocked categories.** Edit the `--fail-on` clauses in
`lazuli-gate.sh` (they're repeatable):

```bash
lazuli doctor . --security-profile strict --coverage \
  --fail-on category:TestDiscipline \
  --fail-on category:Security \
  --fail-on error
```

**Change the per-edit strictness.** Edit the `--security-profile` flag in
`lazuli-check.sh` (e.g. to `strict` if you want the inner loop to be pickier).

**Disable a hook** — three options, least to most invasive:

1. *Per session, drop the slow tier:* set `LAZULI_GATE_RUN_TESTS=0` to keep the
   gate on the fast doctor-only path (skips the handler codegen + coverage run).
2. *Skip one event:* delete that event's block from
   [`../settings.json`](../settings.json) (remove the `PostToolUse` entry to
   drop the per-edit check, or the `Stop` entry to drop the acceptance gate).
3. *Disable everything:* delete or rename `../settings.json`.

The scripts also **fail open** by design: if there's no global `lazuli` on
`PATH`, or no `jq`/`node` to parse the payload, they exit 0 rather than wedge
every edit. So a misconfigured machine degrades to "no validation," never to
"can't work."

---

## Cross-platform notes

- Scripts are `bash` (`#!/usr/bin/env bash`, `set -euo pipefail`). On Windows
  Claude Code runs them through Git Bash (the `"shell": "bash"` in
  `settings.json`). Make sure Git Bash is on `PATH`, or set
  `CLAUDE_CODE_GIT_BASH_PATH`.
- Paths use `${CLAUDE_PROJECT_DIR}` (the stable project root that Claude Code
  interpolates), not the session cwd — so the hooks resolve correctly even
  after the agent `cd`s into a subdirectory.
- JSON parsing prefers `jq` and falls back to `node` (every Lazurite project
  ships Node ≥ 20), so neither tool being absent breaks the harness.
