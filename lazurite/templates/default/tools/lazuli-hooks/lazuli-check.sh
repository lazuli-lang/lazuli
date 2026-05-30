#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# lazuli-check.sh — PostToolUse per-edit checker (the "no broken .lzi survives
# one step" loop). Lazuli's equivalent of `tsc --noEmit` on save.
#
# AGENT-AGNOSTIC. The same script is wired from BOTH agents — their hook
# models are compatible (same events, same stdin-JSON-on-stdin, same exit-2
# block protocol):
#   * Claude Code → .claude/settings.json  (PostToolUse matcher Write|Edit|MultiEdit)
#   * Codex CLI   → .codex/hooks.json       (PostToolUse matcher apply_patch)
# The only real difference is how each names the edited file on stdin, which
# `extract_changed_files` below absorbs:
#   * Claude passes the path directly as `tool_input.file_path`.
#   * Codex edits via apply_patch; the touched paths live inside the patch text
#     at `tool_input.command` (`*** Add/Update File:` / `*** Move to:` markers).
#
# On a parse/analyzer ERROR it prints a fix-oriented message to stderr and
# exits 2 — both agents surface stderr and halt the loop *before the next step*
# so the break is fixed immediately. Warnings never block.
#
# Design notes (see README.md for the full rationale):
#   * `lazuli check <file>` is ~30 ms, single-file, needs no project context,
#     and exits non-zero ONLY on error-severity diagnostics — the right
#     per-keystroke gate.
#   * `--security-profile prototype` = loosest profile, minimal escalation
#     noise. `check` ignores the Lazurite.toml [doctor] profile and defaults to
#     `strict`, so we pass `prototype` explicitly.
#   * `--allow-version-mismatch` so a runtime-pin drift never blocks an edit.
#   * Cross-file completeness (missing `uses`, handler `.go`, coverage) is NOT
#     this hook's job — the Stop gate (lazuli-gate.sh) owns that.
# ---------------------------------------------------------------------------
set -euo pipefail

# Read the entire hook payload from stdin once (both agents deliver a JSON
# object on stdin).
payload="$(cat)"

# ---------------------------------------------------------------------------
# Resolve the project root so relative apply_patch paths (Codex) resolve, and
# so the check runs from a stable directory regardless of session cwd. Order:
# explicit agent env → git toplevel → cwd. CLAUDE_PROJECT_DIR is set by Claude
# Code; Codex runs at the session cwd, so git toplevel is the reliable anchor.
# ---------------------------------------------------------------------------
project_root() {
  if [[ -n "${CLAUDE_PROJECT_DIR:-}" ]]; then printf '%s' "${CLAUDE_PROJECT_DIR}"; return; fi
  if [[ -n "${CODEX_PROJECT_DIR:-}" ]]; then printf '%s' "${CODEX_PROJECT_DIR}"; return; fi
  git rev-parse --show-toplevel 2>/dev/null || printf '.'
}
cd "$(project_root)" 2>/dev/null || true

# ---------------------------------------------------------------------------
# Extract every edited file path from the payload, across both agent dialects.
# Prefer node (every Lazurite project ships node >= 20) because it parses the
# JSON AND regexes the apply_patch body cleanly; fall back to jq for the simple
# Claude `file_path` case. If neither is present we cannot parse — fail OPEN.
# ---------------------------------------------------------------------------
extract_changed_files() {
  if command -v node >/dev/null 2>&1; then
    printf '%s' "${payload}" | node -e '
      let raw = "";
      process.stdin.on("data", (d) => (raw += d));
      process.stdin.on("end", () => {
        const out = [];
        try {
          const j = JSON.parse(raw);
          const ti = j.tool_input || {};
          // Claude Write/Edit/MultiEdit:
          if (typeof ti.file_path === "string" && ti.file_path) out.push(ti.file_path);
          // Codex apply_patch: paths live in the patch text. Capture Add /
          // Update / Move-to targets; skip Delete (the file is gone).
          const cmd = typeof ti.command === "string" ? ti.command : "";
          if (cmd) {
            const re = /^\*\*\*\s+(?:Add File|Update File|Move to):\s+(.+?)\s*$/gm;
            let m;
            while ((m = re.exec(cmd)) !== null) out.push(m[1]);
          }
        } catch (_) { /* unparseable → emit nothing → fail open */ }
        process.stdout.write([...new Set(out)].join("\n"));
      });
    '
  elif command -v jq >/dev/null 2>&1; then
    printf '%s' "${payload}" | jq -r '.tool_input.file_path // empty'
  else
    printf ''
  fi
}

# ---------------------------------------------------------------------------
# Resolve the Lazuli entry point. A global `lazuli` on PATH is assumed (same
# assumption package.json's `lazuli:*` scripts make). If it's absent we fail
# OPEN so the lack of a binary never blocks editing.
# ---------------------------------------------------------------------------
if ! command -v lazuli >/dev/null 2>&1; then
  exit 0
fi

# Collect the .lzi/.lzx files that actually exist on disk now. (A file may have
# been deleted/renamed between the tool call and the hook; skip if it's gone.)
checked_any=0
failures=""
while IFS= read -r file; do
  [[ -z "${file}" ]] && continue
  case "${file}" in
    *.lzi | *.lzx) ;;
    *) continue ;;
  esac
  [[ -f "${file}" ]] || continue
  checked_any=1
  set +e
  out="$(lazuli check "${file}" --security-profile prototype --allow-version-mismatch 2>&1)"
  status=$?
  set -e
  if [[ ${status} -ne 0 ]]; then
    failures+="── ${file} ──
${out}

"
  fi
done <<EOF
$(extract_changed_files || true)
EOF

# No checkable Lazuli source in this edit (or nothing parseable) → nothing to do.
if [[ ${checked_any} -eq 0 || -z "${failures}" ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Error-severity diagnostic(s). Block the step: print a fix-oriented message to
# STDERR and exit 2. Both Claude Code and Codex show stderr to the agent and
# halt the loop before the next action, so the broken source is repaired now.
# ---------------------------------------------------------------------------
{
  echo "BLOCKED: \`lazuli check\` failed (parse/analyzer error) on the file(s) just edited."
  echo ""
  printf '%s' "${failures}"
  echo "Fix the error(s) above before continuing. Each diagnostic names the rule"
  echo "code (path:line:col: error [CODE]: message) — address the CODE, do not"
  echo "work around it. Re-running the edit will re-trigger this check."
} >&2

exit 2
