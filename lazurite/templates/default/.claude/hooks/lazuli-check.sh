#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# lazuli-check.sh — PostToolUse per-edit checker (the "no broken .lzi survives
# one step" loop). Lazuli's equivalent of `tsc --noEmit` on save.
#
# Fired by .claude/settings.json on Write|Edit|MultiEdit. Reads the tool
# invocation JSON on stdin, extracts the edited file, and — only for
# `.lzi`/`.lzx` sources — runs the fast single-file Lazuli check. On a
# parse/analyzer ERROR it prints a fix-oriented message to stderr and exits 2,
# which Claude Code surfaces to the agent as a system message and stops the
# agentic loop *before the next step* so the break is fixed immediately.
#
# Design notes (see .claude/hooks/README.md for the full rationale):
#   * `lazuli check <file>` is ~30 ms, single-file, needs no project context,
#     and exits non-zero ONLY on error-severity diagnostics (warnings never
#     block an in-progress edit). That makes it the right per-keystroke gate.
#   * `--security-profile prototype` = loosest profile, minimal escalation
#     noise. `check` ignores the Lazurite.toml [doctor] profile and always
#     defaults to `strict`, so we pass `prototype` explicitly here.
#   * `--allow-version-mismatch` so a runtime-pin drift in Lazurite.toml never
#     blocks an unrelated edit.
#   * Cross-file completeness (missing `uses`, missing handler `.go`, registry
#     env, coverage) is intentionally NOT this hook's job — the Stop-hook gate
#     (lazuli-gate.sh) owns that. This hook is the fast inner loop only.
# ---------------------------------------------------------------------------
set -euo pipefail

# Read the entire hook payload from stdin once.
payload="$(cat)"

# ---------------------------------------------------------------------------
# Extract .tool_input.file_path from the payload. Prefer jq; fall back to node
# (every Lazurite project ships node >= 20) so the hook is robust on a machine
# without jq on PATH. If neither is available we cannot parse — fail OPEN
# (exit 0) so a missing tool never wedges every edit.
# ---------------------------------------------------------------------------
extract_file_path() {
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$payload" | jq -r '.tool_input.file_path // empty'
  elif command -v node >/dev/null 2>&1; then
    printf '%s' "$payload" | node -e '
      let raw = "";
      process.stdin.on("data", (d) => (raw += d));
      process.stdin.on("end", () => {
        try {
          const j = JSON.parse(raw);
          process.stdout.write((j.tool_input && j.tool_input.file_path) || "");
        } catch {
          process.stdout.write("");
        }
      });
    '
  else
    printf ''
  fi
}

file="$(extract_file_path || true)"

# No file path on the payload (or unparseable) → nothing to check.
if [[ -z "${file}" ]]; then
  exit 0
fi

# Only Lazuli sources are checkable. Everything else (Go handlers, SQL,
# templates, JSON catalogs, config) is out of scope for `lazuli check`.
case "${file}" in
  *.lzi | *.lzx) ;;
  *) exit 0 ;;
esac

# The file may have been deleted/renamed between the tool call and the hook
# (rare, but Edit on a moved path). If it's gone, there's nothing to check.
if [[ ! -f "${file}" ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Resolve the Lazuli entry point. Prefer the project's wrapped pnpm script
# semantics, but call the binary directly with the single edited file: the
# pnpm `lazuli:check` script targets the whole project (`lazuli check .`),
# whereas the per-edit loop wants JUST this file for speed. A global `lazuli`
# on PATH is assumed (same assumption package.json makes); if it's absent we
# fail OPEN so the lack of a binary never blocks editing.
# ---------------------------------------------------------------------------
if ! command -v lazuli >/dev/null 2>&1; then
  exit 0
fi

# Run the fast single-file check. Capture combined output so we can echo the
# exact diagnostic lines (`path:line:col: severity [code]: message`) back to
# the agent verbatim on failure.
set +e
check_output="$(lazuli check "${file}" --security-profile prototype --allow-version-mismatch 2>&1)"
check_status=$?
set -e

if [[ ${check_status} -eq 0 ]]; then
  # Clean (or warnings-only — those don't fail the exit). Proceed silently.
  exit 0
fi

# ---------------------------------------------------------------------------
# Error-severity diagnostic(s). Block the step: print a fix-oriented message
# to STDERR and exit 2. Claude Code shows stderr to the agent and halts the
# loop before the next action, so the broken `.lzi`/`.lzx` is repaired now.
# ---------------------------------------------------------------------------
{
  echo "BLOCKED: \`lazuli check\` failed on ${file} (parse/analyzer error)."
  echo ""
  echo "${check_output}"
  echo ""
  echo "Fix the error above in ${file} before continuing. The diagnostic names"
  echo "the rule code (path:line:col: error [CODE]: message) — address the CODE,"
  echo "do not work around it. Re-running the edit will re-trigger this check."
} >&2

exit 2
