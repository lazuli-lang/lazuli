---
id: 0006
title: doctor:allow highlighting — color the suppression directive in the grammar
type: prd
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: true
track: ship/tell
test_gate: "npm run test:grammar:lazuli"
agent: unassigned
---

# PRD — doctor:allow highlighting

## Problem
A `# doctor:allow <CODE> — reason "<text>"` comment is a load-bearing directive: it tells the doctor (and any human/LLM reading the file) that a lint was deliberately suppressed and why. But the VSCode TextMate grammar renders it as undifferentiated comment color — the same dim gray as `# TODO` or `# this is a hack`. The single flat `comments` rule (`editors/vscode/syntaxes/lazuli.tmLanguage.json` ~lines 337–344: `{"name":"comment.line.number-sign.lazuli","match":"#.*$"}`) swallows the whole line. So the one comment shape that has semantic weight looks identical to the throwaway ones. The user explicitly asked for this as a secondary improvement.

This is an inconsistency, not just an aesthetic gap: the grammar ALREADY highlights `reason` text inside `scope override` blocks via the `scope-block` rule (~lines 808–828). Suppression directives deserve the same treatment.

## Goal
`# doctor:allow VOCAB-TESTS-MISSING-001 — reason "covered by integration suite"` renders with the `#` as punctuation, `doctor:allow` as a control keyword, the diagnostic code as an entity name, the `reason` keyword distinct, and the quoted reason as a string — so the directive reads as the first-class thing it is. Plain comments keep their existing single color.

## Non-goals
- No change to doctor behavior, parsing, or the `allow_comment.rs` matcher — that opt-out semantics work is spec 0007. This is grammar-only.
- No new directive forms. Canonical form stays `# doctor:allow <CODE> — reason "<text>"`.
- Do NOT touch the generated `#kw-*` rules (above `_kw_generated_end`, ~line 336) — they are emitted by `cargo xtask gen-tmlanguage`; no regeneration is needed for this change.
- No LSP / semantic-token work; this is the static TextMate layer only (agent-first parity: the CLI/doctor already names the directive in plaintext, so CLI fidelity is unaffected).

## Evidence (pilot audit 2026-05-31)
- Canonical directive form is used 85× across 31 pilot files, codes like `VOCAB-TESTS-MISSING-001`. Every use carries a `— reason "..."` tail today.
- `comments` repository rule is a single flat match → directive renders flat.
- `scope-block` rule is the precedent: it already scopes `reason` + the quoted text inside `scope override`.
- The `comments` rule sits AFTER the generated fence (`_kw_generated_end`), so it is hand-editable.

## Users
- The agent/human authoring `.lzi`/`.lzx` who needs suppressions to visually stand out from chatter (catch an un-reasoned or stale allow on sight).
- Reviewers scanning a diff for newly-added suppressions.

## Success criteria
1. A `doctor:allow` comment highlights its `#`, `doctor:allow`, `<CODE>`, `reason`, and quoted reason as distinct scopes.
2. A plain `# comment` is unchanged (single `comment.line.number-sign.lazuli`).
3. `npm run test:grammar:lazuli` green with an updated `.snap` that includes a `doctor:allow` fixture line.
4. The idiom doc `docs/lazuli_way/comment-hygiene.md` carries the "doctor:allow is a first-class, highlighted directive" note.

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.
