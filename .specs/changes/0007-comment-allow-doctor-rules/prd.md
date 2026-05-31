---
id: 0007
title: comment/allow doctor rules — DOCTOR-ALLOW-NO-REASON-001 + LZI-COMMENT-NOISE-001
type: prd
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: true
track: prove
test_gate: "cargo test -p lazuli_doctor allow_no_reason && cargo test -p lazuli_doctor lzi_comment_noise"
agent: unassigned
---

# PRD — comment/allow doctor rules

## Problem
Two comment-hygiene gaps the doctor doesn't see today.

**(a) Un-reasoned suppression.** The shared opt-out helper `source_contains_doctor_allow` (`crates/lazuli_doctor/src/allow_comment.rs` ~lines 68–80) matches `# doctor:allow <CODE>` and IGNORES the `— reason "..."` tail entirely — the test `matches_without_reason_tail` (~line 93) asserts a bare allow opts out. So an agent (or human) can silence any advisory lint with a single bare `# doctor:allow CODE`, no justification, and the doctor accepts it silently. That is the "AI silences a lint without explaining" loophole: suppression with no audit trail.

**(b) Comment noise / decorative dividers in `.lzi`/`.lzx`.** There is no doctor signal for comment-to-semantic ratio or decorative ruler lines (`# ----`, `// ====`) in feature/experience files. The framework already has the heuristic for config files — `config_noise.rs` (`CONFIG-NOISE-001`, advisory, never gates) — but it's scoped to TOML-shaped config, not the `.lzi`/`.lzx` surface where AI-generated drift accumulates.

## Why now (or why ever)
- (a) is a real, exploitable hole the maker explicitly cares about: a suppression with no reason is worse than no suppression, because it looks deliberate. All 85 current pilot uses DO carry a reason, so the rule is **clean on today's pilots** — it's a preventive guard against future un-reasoned allows (especially AI-authored ones). Cheap to add now while the pattern is fresh and `allow_comment.rs` is the obvious home.
- (b) The pilots' `.lzi` are actually CLEAN today: zero decorative dividers. (The 382 `// ----` rulers live in ONE TypeScript client file, which is out of scope for an `.lzi`/`.lzx` rule.) So this rule is **preventive, scoped to flag future drift**, generalizing an existing, proven heuristic rather than inventing one. Its "why now" is prevention + the AI-noise concern the maker raised — stated honestly, not as a present-tense fire.

## Goal
- `DOCTOR-ALLOW-NO-REASON-001` (advisory): fire when a `# doctor:allow <CODE>` carries no `— reason "..."` tail. Names the fix in its message: add `— reason "<why>"`.
- `LZI-COMMENT-NOISE-001` (advisory, never gates): comment-to-semantic ratio + decorative-divider lint for `.lzi`/`.lzx`, in a new `lzi_hygiene/` rule family alongside `file_size_001.rs`. Honors `# doctor:allow`.

## Non-goals
- No change to the bare-allow opt-out *semantics* for other rules — `source_contains_doctor_allow` keeps treating a bare allow as a valid opt-out for the rule it names. `DOCTOR-ALLOW-NO-REASON-001` is a *separate advisory* about the allow itself, not a hard gate that voids the opt-out. (An author can even `# doctor:allow DOCTOR-ALLOW-NO-REASON-001 — reason "..."`, which is self-documenting and acceptable.)
- No grammar/highlighting work — that's spec 0006 (companion).
- No new gating severity. Both rules are advisory; `LZI-COMMENT-NOISE-001` NEVER gates (matches `CONFIG-NOISE-001`'s discipline).
- No retro-fix of the TS client's 382 rulers (out of scope; not an `.lzi`/`.lzx` file).
- No comment-stripping autofix.

## Evidence (pilot audit 2026-05-31)
- `allow_comment.rs` matcher ignores the reason tail; `matches_without_reason_tail` asserts bare allow opts out.
- 85 `doctor:allow` uses across 31 pilot files; ALL carry a reason → rule (a) is clean on current pilots.
- `config_noise.rs` (`CONFIG-NOISE-001`): advisory, `comment_lines > semantic_lines` heuristic, `ratio()`, never gates — the proven shape to generalize.
- Pilots' `.lzi` have zero decorative dividers; 382 `// ----` rulers are in one TS client (out of scope) → rule (b) is preventive.

## Users
- The maker / reviewer who wants every suppression justified and every `.lzi` to stay legible as AI authors more of them.
- The doctor itself, which gains an audit trail for suppressions.

## Success criteria
1. `cargo test -p lazuli_doctor allow_no_reason` green: rule fires on `# doctor:allow CODE` (no tail), does NOT fire on `# doctor:allow CODE — reason "..."`.
2. `cargo test -p lazuli_doctor lzi_comment_noise` green: rule fires on a synthetic noisy/decorative-divider `.lzi`, does NOT fire on a clean one, and is suppressible via `# doctor:allow LZI-COMMENT-NOISE-001 — reason "..."`.
3. Both pilots stay clean: `lazuli doctor` surfaces zero new findings on hostpoint + pauta-web (all 85 allows are reasoned; `.lzi` are clean).
4. `docs/lazuli_way/comment-hygiene.md` filled (co-filled with 0006) with both rule rows in idiom-doc shape.

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.
