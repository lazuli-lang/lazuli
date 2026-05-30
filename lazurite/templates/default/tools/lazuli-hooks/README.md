# Validation harness — agent hooks (Claude Code **and** Codex)

This directory holds a **two-level validation harness** that runs automatically
during AI coding sessions. It is the Lazuli equivalent of the well-known
"post-edit `tsc --noEmit` + a `test`/coverage acceptance gate" pattern.

The scripts are **agent-agnostic**. One set of scripts, wired from whichever
agent you use:

| Agent | Wiring file | Mechanism |
|---|---|---|
| **Claude Code** | [`../../.claude/settings.json`](../../.claude/settings.json) | `hooks.PostToolUse` (matcher `Write\|Edit\|MultiEdit`) + `hooks.Stop` |
| **Codex CLI** | [`../../.codex/hooks.json`](../../.codex/hooks.json) | `hooks.PostToolUse` (matcher `apply_patch`) + `hooks.Stop` |

Both agents share the same hook model — `PostToolUse` / `Stop` events, JSON on
stdin, and the same block protocol (`{"decision":"block","reason":…}` on stdout
or exit 2 + stderr) — so the same two scripts drive both. The scripts absorb the
only real difference: how each agent names the edited file (see Level 1).

> **Philosophy.** The quality of what an agent delivers is proportional to the
> quality of the validation environment around it. An agent that physically
> *cannot* take the next step while a file is broken — and *cannot stop* while
> the acceptance gate is red — converges on correct work. These hooks are that
> environment, checked into the repo so it travels with the project.

The harness has two independent levels:

| Level | Event | Script | Runs | Blocks via |
|---|---|---|---|---|
| 1. Per-edit check | `PostToolUse` | `lazuli-check.sh` | `lazuli check <edited file>` (fast, single-file) | **exit 2 + stderr** → loop halts before the next step until the parse/analyzer error is fixed |
| 2. Acceptance gate | `Stop` | `lazuli-gate.sh` | `lazuli doctor` (strict + coverage + test-discipline block) **+** `lazuli test --layer handler` coverage gate once handlers exist | **`{"decision":"block","reason":…}`** → the agent refuses to stop and keeps fixing |

---

## Enabling per agent

**Claude Code** — nothing to do. `.claude/settings.json` is picked up
automatically when you open the project.

**Codex CLI** — Codex requires you to **trust** non-managed command hooks once
per project. Run `/hooks` in the Codex TUI and approve the two hooks (or accept
the trust prompt on first fire). Launch `codex` from the **project root** so the
relative `tools/lazuli-hooks/…` command resolves (the scripts also re-anchor to
the git toplevel internally, so the actual checks run from the right directory
regardless).

---

## Level 1 — per-edit check (`lazuli-check.sh`)

Fires after **every** edit. It reads the tool payload on stdin, extracts the
edited file path(s), and — **only** for `.lzi` / `.lzx` sources — runs:

```bash
lazuli check <edited-file> --security-profile prototype --allow-version-mismatch
```

**Both agent dialects** are handled by `extract_changed_files`:

- **Claude Code** passes the path directly as `tool_input.file_path`.
- **Codex** edits via `apply_patch`; the touched paths live inside the patch
  text at `tool_input.command`. The script parses the `*** Add File:` /
  `*** Update File:` / `*** Move to:` markers (and skips `*** Delete File:`),
  so a multi-file patch checks every `.lzi`/`.lzx` it touched.

Why these flags: `lazuli check <file>` is ~30 ms and needs no project context,
so it's cheap on every edit. `--security-profile prototype` is the loosest
profile (minimal escalation noise; `check` otherwise defaults to `strict`).
`--allow-version-mismatch` keeps a runtime-pin drift from blocking an unrelated
edit. `check` exits non-zero **only on error-severity diagnostics** — warnings
never fire the gate, so a half-written file isn't punished for being incomplete,
only for being *broken*. On an error the hook prints the exact
`path:line:col: error [CODE]: message` to stderr and exits `2`.

What level 1 deliberately does **not** catch (single-file by design): missing
`uses`, a referenced handler `.go` that doesn't exist yet, registry env, or
coverage. Those are completeness concerns — level 2 owns them.

## Level 2 — acceptance gate on stop (`lazuli-gate.sh`)

Fires when the agent tries to **finish**. Runs the project-wide gate and, if
red, returns a `block` decision so the agent cannot declare "done":

```bash
lazuli doctor . --security-profile strict --coverage --fail-on category:TestDiscipline
```

Under `strict` most discipline findings are **warnings** (a bare `doctor` would
exit 0), so the explicit `--fail-on category:TestDiscipline` is what makes the
test-discipline family actually block. The doctor `spec` surface exercises the
spec / actor-matrix / predicate / transition rules, so this is a meaningful
acceptance gate on its own, no codegen required (~0.3 s).

**Infinite-loop guard.** When a `Stop` hook blocks, the agent re-enters the
`Stop` event with `stop_hook_active=true` on stdin. The gate checks that flag
and allows the stop on the *second* pass, so a gate the agent genuinely can't
satisfy can't trap the session forever.

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
exit code); the `test` runner owns the `coverage:<metric>=<N>` gate, so coverage
can only *block* from here. The bar defaults to **90** (`handler_go`).

Three robustness rules keep this from ever wedging a legitimately-early session:

- **Auto-skipped while there are no handler `.go` files** (`app/features/**/handlers/*.go`).
- **Codegen failure only warns** (offline `go mod` etc.) instead of blocking.
- **Disable / tune via env.** `LAZULI_GATE_RUN_TESTS=0` turns the tier off;
  `LAZULI_GATE_HANDLER_COVERAGE=<pct>` moves the bar. Expect multi-second
  latency on each stop once this tier is active (codegen + `go test`).

---

## Tuning

**Stricter / looser via `Lazurite.toml`** — the hooks honor your manifest:

```toml
[doctor]
profile = "strict"        # prototype | strict | production | iron-hand

[doctor.coverage]
preset = "tdd-strict"     # tdd-strict | tdd-mature | tdd-iron-hand | …
```

Bump `profile` to `production`/`iron-hand` to escalate warnings → errors (then
even a bare `doctor` blocks). The coverage *block* comes from the handler tier
above (gated on the `test` runner); `LAZULI_GATE_HANDLER_COVERAGE` overrides the
bar without touching the manifest.

**Add more hard-blocked categories** — the `--fail-on` clause in
`lazuli-gate.sh` is repeatable (`--fail-on category:Security --fail-on error`).

**Disable a level** — remove its entry from `.claude/settings.json` /
`.codex/hooks.json`, or delete that wiring file entirely.

The scripts **fail open** by design: no global `lazuli` on `PATH`, or no
`jq`/`node` to parse the payload → they exit 0 rather than wedge every edit. A
misconfigured machine degrades to "no validation," never to "can't work."

---

## Other agents / universal fallback

Any agent whose hook model matches (JSON-on-stdin `PostToolUse`/`Stop`, exit-2 /
`decision:block`) can reuse these scripts — point its wiring at
`tools/lazuli-hooks/lazuli-{check,gate}.sh`. For agents *without* hooks (or for
human commits), wire the same checks as git hooks:

```bash
# .git/hooks/pre-commit  (or a Husky/lefthook entry)
lazuli check . --security-profile prototype || exit 1
# .git/hooks/pre-push
lazuli doctor . --security-profile strict --fail-on category:TestDiscipline || exit 1
```

That gives a truly agent-independent backstop at commit/push time.

---

## Cross-platform notes

- Scripts are `bash` (`#!/usr/bin/env bash`, `set -euo pipefail`). On Windows,
  Claude Code runs them through Git Bash (`"shell": "bash"` in `settings.json`);
  ensure Git Bash is on `PATH` (or set `CLAUDE_CODE_GIT_BASH_PATH`). Codex
  invokes the `bash …` command directly.
- The repo's `.gitattributes` pins `*.sh` to `eol=lf` so a Windows checkout
  can't rewrite the hooks to CRLF (which breaks `bash` shebangs).
- JSON parsing prefers `jq` and falls back to `node` (every Lazurite project
  ships Node ≥ 20), so neither tool being absent breaks the harness.
