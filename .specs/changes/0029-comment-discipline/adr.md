---
id: 0029
title: Canonical-channel policy + LZI-COMMENT-PROSE-001 (prose flagged, narrow carve-outs)
type: adr
status: accepted
created: 2026-06-02
supersedes: —
---

# ADR — `.lzi`/`.lzx` prose comments are flagged (`LZI-COMMENT-PROSE-001`, LziHygiene, WARNING default / ERROR iron-hand); a written channel policy + a `lazuli fix` codemod redirect the text to its canonical home.

## Context
- The root failure: every text-bearing intent already has a structured home, but agents emit it as `#` prose anyway. Homes that exist TODAY: `<feature>.ctx.md` context files (rationale/design narrative); `purpose "..."` (feature/iron-hand context vocab, see `crates/lazuli_syntax/src/parser/lzi/iron_hand_context.rs`); resource/field `doc` + `description`; `@doctor.allow(...)` (waivers, from 0028).
- `LZI-COMMENT-NOISE-001` (0007, `crates/lazuli_doctor/src/lzi_hygiene/lzi_comment_noise.rs`) already catches the two extreme shapes (decorative dividers; comment-dominant files) and is advisory/never-gates. It does NOT catch a single prose comment above a construct — the common case.
- Severity escalation infra exists: `lazuli_doctor_config` `DoctorProfile { Prototype, Production, IronHand }` + `doctor_severity_for` / `resolve_<cat>_severity`. A rule can default WARNING and escalate to ERROR under iron-hand.
- The gate (`crates/lazuli_cli/src/doctor/gate.rs`) blocks only concrete-bug categories; `LziHygiene` is explicitly NON-blocking (gate.rs:110). So this rule never refuse-emits regardless of severity — exactly what we want for a hygiene nudge.
- 0028 froze `Module.doctor_allows`; the carve-out for `@doctor.allow` lines reads from there, not from a string match.

## Decision
**Channel policy (the decision table, written into `docs/lazuli_way/comment-hygiene.md`):**
| Text kind | Canonical home |
|---|---|
| Rationale / design narrative / "why we did X" | `<feature>.ctx.md` |
| A construct's intent / one-liner | its `purpose` / `doc` / `description` field |
| A doctor waiver + reason | `@doctor.allow(CODE, reason: "...")` (0028) |
| Section visual separators / banners | NOT allowed (already caught by `LZI-COMMENT-NOISE-001`) |
| A bare `#` comment | Only legitimate for: a single optional file-header line (license/owner) and short structured marker labels (see carve-outs). Anything that reads as a sentence belongs in a channel above. |

**Rule `LZI-COMMENT-PROSE-001` (category LziHygiene; WARNING default, ERROR under iron-hand; NEVER gates — LziHygiene is non-blocking):** fires on each `#` comment line in `.lzi`/`.lzx` whose body is PROSE, by a simple explainable heuristic — body has ≥ N words (default 4) OR ends with sentence punctuation (`.`/`!`/`?`) OR contains an inter-word space run consistent with a sentence — AND is NOT a carve-out.

**Carve-outs (do NOT fire):**
1. A `@doctor.allow(...)` line (it is a node now, not a comment — but the legacy `# doctor:allow` comment form is also exempted this window, read via `Module.doctor_allows` legacy entries).
2. The FIRST line of the file when it is a single short header comment (license/owner) — at most one, only at file top.
3. Short structured marker labels: a comment whose body is `<=` a small word budget AND looks like a label, e.g. `# TODO:`/`# FIXME:`/`# NOTE:` followed by few words, or a single identifier-ish token. (These are operational markers, not narrative.)
4. Anything `LZI-COMMENT-NOISE-001` already owns (decorative dividers) — not double-flagged.

**Codemod `lazuli fix --rule LZI-COMMENT-PROSE-001`:** mechanical relocations/removals only — (a) delete a comment that merely restates the construct on the next line (exact/near-duplicate of the construct's name/keyword); (b) delete an empty/whitespace-only `#` line. SEMANTIC moves (prose → `purpose`/`.ctx.md`) are NOT auto-performed; the rule reports them for the author/LLM to relocate, and the message names the target channel.

## Alternatives considered
- **Ban ALL `#` comments outright** — rejected: a file-header line and `# TODO:` markers are legitimately useful and banning them invites a flood of `@doctor.allow` waivers just to keep them. Narrow carve-outs keep the rule honest.
- **Make it ERROR by default** — rejected: a hygiene lint that errors by default fights every existing pilot on first run and trains people to `--no-gate`. WARNING default + iron-hand ERROR matches the established `DoctorProfile` posture (config_noise discipline).
- **An LLM/NLP "is this prose" classifier** — rejected: not explainable, not testable, not idempotent. A word-count + punctuation heuristic an agent can reason about beats a black box.
- **Auto-lift prose into `purpose`/`.ctx.md`** — rejected for now: semantic placement is a judgement call; a wrong auto-move corrupts intent. Report + name the channel; let the author move it. The codemod only does the safe mechanical deletes.
- **Fold it into `LZI-COMMENT-NOISE-001`** — rejected: noise-001 is file-level (ratio/dividers); this is per-comment-line with a severity profile and a channel-naming message. Separate codes keep both teachable and independently suppressible.

## Consequences
**We accept:** a heuristic will occasionally misjudge a borderline comment (false positive/negative). Mitigated by WARNING-default + `@doctor.allow` escape + tuning against real pilots before commit. We accept two hygiene rules (NOISE + PROSE) co-owning `.lzi` comment quality — they have disjoint triggers and one teach doc. We accept the codemod is conservative (mechanical deletes only), leaving semantic moves to the author.
**We gain:** the root behavior finally has both a written policy AND an enforcing rule — the LLM is told where text goes (scaffold guidance) and told when it got it wrong (the rule). Design surfaces trend signal-dense.
**We watch:** the false-positive rate on real pilots after 0028's `lazuli fix`. If the heuristic is noisy, narrow the trigger (raise the word budget) before raising severity. If iron-hand ERROR starts blocking nothing (because LziHygiene never gates), that's expected — the value is the visible WARNING + the agent guidance, not a gate.
