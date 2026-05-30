#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# lazuli-gate.sh — Stop-hook acceptance gate (the executable "definition of
# done"). Lazuli's equivalent of the article's post-task test + coverage
# harness: the agent CANNOT declare itself finished while `lazuli doctor`
# (and, opt-in, `lazuli test`) is red.
#
# Fired by .claude/settings.json on the Stop event. Runs the project-wide
# acceptance gate; if it fails, returns `{"decision":"block", ...}` so Claude
# Code refuses to stop and forces the agent to keep fixing. An infinite-loop
# guard (`stop_hook_active`) lets the agent off the hook after one blocked
# round so a genuinely-stuck gate can't trap the session forever.
#
# Tiers (see .claude/hooks/README.md for the full rationale + tuning):
#   * Default (fast, ~0.3 s): `lazuli doctor . --security-profile strict
#     --coverage --fail-on category:TestDiscipline`. Under the `strict`
#     profile most discipline findings are WARNINGS (exit 0 by default), so we
#     pass an explicit `--fail-on` to make the test-discipline family actually
#     block — mirroring the `pnpm lazuli:doctor:gate` script. The doctor `spec`
#     surface already exercises the spec/actor/predicate/transition rules, so
#     this alone is a meaningful acceptance gate without codegen.
#   * Opt-in handler tests: set LAZULI_GATE_RUN_TESTS=1 to also run
#     `lazuli generate` + `lazuli test . --layer handler`. This is OFF by
#     default because `lazuli test --layer handler` shells out to `go test`
#     against the GENERATED `dist/go/` tree and fails spuriously if codegen
#     hasn't run — so when enabled we always generate first.
# ---------------------------------------------------------------------------
set -euo pipefail

# ---------------------------------------------------------------------------
# Read the Stop payload from stdin ONCE. Claude Code delivers hook state as a
# JSON object on stdin (NOT as shell/env vars), including `stop_hook_active` —
# the field we need for the loop guard below. Parse it with jq, falling back to
# node (every Lazurite project ships node), mirroring lazuli-check.sh.
# ---------------------------------------------------------------------------
payload="$(cat)"

extract_field() {
  # $1 = top-level JSON field name. jq first, then node, else empty.
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "${payload}" | jq -r ".$1 // empty"
  elif command -v node >/dev/null 2>&1; then
    printf '%s' "${payload}" | FIELD="$1" node -e '
      let raw = "";
      process.stdin.on("data", (d) => (raw += d));
      process.stdin.on("end", () => {
        try {
          const j = JSON.parse(raw);
          const v = j[process.env.FIELD];
          process.stdout.write(v == null ? "" : String(v));
        } catch {
          process.stdout.write("");
        }
      });
    '
  else
    printf ''
  fi
}

# ---------------------------------------------------------------------------
# Infinite-loop guard. When a Stop hook blocks, Claude Code re-enters the Stop
# event with `stop_hook_active: true` in the payload. If we blocked last round,
# allow the stop this time rather than looping forever on a gate the agent
# can't satisfy.
# ---------------------------------------------------------------------------
if [[ "$(extract_field stop_hook_active || true)" == "true" ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Emit a block decision (exit 0 + JSON on stdout). Claude Code parses this,
# refuses to stop, and shows `reason` to the agent. Prefer jq for safe JSON
# encoding; fall back to node (always present in a Lazurite project).
# ---------------------------------------------------------------------------
emit_block() {
  local reason="$1"
  if command -v jq >/dev/null 2>&1; then
    jq -n --arg r "${reason}" '{decision: "block", reason: $r}'
  elif command -v node >/dev/null 2>&1; then
    REASON="${reason}" node -e '
      process.stdout.write(JSON.stringify({
        decision: "block",
        reason: process.env.REASON || "",
      }));
    '
  else
    # No JSON encoder available — fall back to the exit-2 + stderr blocking
    # path, which also prevents the stop (weaker, but still blocks).
    echo "${reason}" >&2
    exit 2
  fi
  exit 0
}

# If there is no global `lazuli` on PATH we cannot gate — allow the stop
# rather than wedge every conversation. (Same PATH assumption as package.json.)
if ! command -v lazuli >/dev/null 2>&1; then
  exit 0
fi

failures=()

# ---------------------------------------------------------------------------
# Gate 1 — project-wide doctor (always). Strict profile + coverage report +
# test-discipline hard-block. Capture output so a failure can quote the
# summary back to the agent.
# ---------------------------------------------------------------------------
set +e
doctor_output="$(lazuli doctor . \
  --security-profile strict \
  --coverage \
  --fail-on category:TestDiscipline 2>&1)"
doctor_status=$?
set -e

if [[ ${doctor_status} -ne 0 ]]; then
  # Keep the reason compact: the headline + the last ~30 lines of doctor
  # output (where the summary / gate verdict lives).
  doctor_tail="$(printf '%s\n' "${doctor_output}" | tail -n 30)"
  failures+=("lazuli doctor (strict + coverage, --fail-on category:TestDiscipline) FAILED:
${doctor_tail}")
fi

# ---------------------------------------------------------------------------
# Gate 2 — handler tests (opt-in via LAZULI_GATE_RUN_TESTS=1). Generates the
# Go tree first so `go test ./...` has a module to run against, then runs the
# handler layer. Left off by default to keep the Stop gate fast and free of
# spurious "no main module" failures on an ungenerated checkout.
# ---------------------------------------------------------------------------
if [[ "${LAZULI_GATE_RUN_TESTS:-0}" == "1" ]]; then
  set +e
  gen_output="$(lazuli generate go . --out dist/go 2>&1)"
  gen_status=$?
  set -e
  if [[ ${gen_status} -ne 0 ]]; then
    gen_tail="$(printf '%s\n' "${gen_output}" | tail -n 20)"
    failures+=("lazuli generate go (prerequisite for handler tests) FAILED:
${gen_tail}")
  else
    set +e
    test_output="$(lazuli test . --layer handler 2>&1)"
    test_status=$?
    set -e
    if [[ ${test_status} -ne 0 ]]; then
      test_tail="$(printf '%s\n' "${test_output}" | tail -n 30)"
      failures+=("lazuli test --layer handler FAILED:
${test_tail}")
    fi
  fi
fi

# ---------------------------------------------------------------------------
# Verdict. All gates green → allow the stop (silent exit 0). Any gate red →
# block the stop with a reason enumerating exactly what failed.
# ---------------------------------------------------------------------------
if [[ ${#failures[@]} -eq 0 ]]; then
  exit 0
fi

reason="Acceptance gate is RED — cannot finish yet. Fix the following, then I will re-check on the next stop:

"
for f in "${failures[@]}"; do
  reason+="• ${f}

"
done
reason+="Run \`pnpm lazuli:doctor\` (and \`pnpm lazuli:test\` if you enabled handler tests) locally to reproduce. Address the named rule codes / failing tests — do not suppress without a documented reason."

emit_block "${reason}"
