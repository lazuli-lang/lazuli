---
id: 0007
title: comment/allow doctor rules — DOCTOR-ALLOW-NO-REASON-001 + LZI-COMMENT-NOISE-001
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Two advisory comment-hygiene rules: a reason-tail check on suppressions, and an `.lzi`/`.lzx` noise heuristic generalized from CONFIG-NOISE-001

## Context
- The opt-out helper `source_contains_doctor_allow` (`crates/lazuli_doctor/src/allow_comment.rs`) deliberately matches a bare `# doctor:allow <CODE>` and ignores any `reason` tail, so a single line silences an advisory lint with no recorded justification. The helper's own doc-comment frames the canonical form WITH a reason; the matcher just doesn't require it.
- The maker's stated concern: AI agents will suppress lints to make the build pass, and an un-reasoned suppression is a silent loss of intent. The rule must surface that without breaking the opt-out mechanism other rules rely on.
- A proven noise heuristic already exists for config files: `config_noise.rs` (`ConfigNoiseMetrics`, `fires()` when `comment_lines > semantic_lines`, `ratio()`, advisory, never gates). The `.lzi`/`.lzx` surface has no equivalent, even though it's where AI-authored comment drift will land.
- A `lzi_hygiene/` rule family is the natural home (the prompt cites `file_size_001.rs` as a sibling there). Grouping `LZI-COMMENT-NOISE-001` alongside file-size keeps file-level `.lzi` hygiene rules together.

## Decision
- **(a) `DOCTOR-ALLOW-NO-REASON-001` is a SEPARATE advisory, not a change to opt-out semantics.** Add a reason-tail detector next to `source_contains_doctor_allow` (e.g. `allow_has_reason(line) -> bool` matching `\b(reason)\b\s*"` after the code, accepting `—`/`--`/whitespace as the separator). The opt-out helper is unchanged: a bare allow still opts the named rule out. The new rule scans for `# doctor:allow <CODE>` lines that lack the reason tail and emits an advisory. This keeps existing reasoned suppressions silent and existing bare suppressions working — but now *flagged*.
- **(b) `LZI-COMMENT-NOISE-001` generalizes `CONFIG-NOISE-001` into `lzi_hygiene/`.** Reuse the comment-vs-semantic counting shape (full-line `#` comments + trailing inline `#`/`//`) and add a decorative-divider detector (a line whose comment body is a run of repeated `-`, `=`, `*`, `#`, `/` rulers past a length threshold). Advisory, NEVER gates — identical severity discipline to `CONFIG-NOISE-001`. Honors `# doctor:allow LZI-COMMENT-NOISE-001` via the existing helper.
- **Both rules are advisory and clean on current pilots.** Ship them as preventive guards; the PRD states honestly that (a) has zero current hits and (b) the `.lzi` are clean today.
- **Teach both in the shared `docs/lazuli_way/comment-hygiene.md`** (co-filled with 0006; append rule rows, don't overwrite 0006's highlight note).

## Alternatives considered
- **Make a bare allow VOID the opt-out (hard-fail until a reason is added)** — rejected: that turns an advisory concern into a build-breaker and would retroactively break any bare allow; too blunt, and conflicts with the helper's documented contract that a bare allow is valid. Advisory-with-a-named-fix is the right pressure.
- **Fold `LZI-COMMENT-NOISE-001` into the existing `config_noise.rs`** — rejected: that module is scoped to TOML config (`Lazurite.toml`); mixing `.lzi`/`.lzx` semantics into it muddies its purpose. Generalize the *shape* into `lzi_hygiene/`, keep config noise where it is.
- **Gate on comment ratio** — rejected: ratio is a soft heuristic; gating on it would punish legitimately well-documented features. Mirror `CONFIG-NOISE-001` and never gate.
- **A divider-only rule (no ratio)** — rejected: the ratio half is the cheap reuse win and catches a different failure mode (wall-of-comment AI drift) than dividers (decorative rulers). Both, advisory.

## Consequences
**We accept:** two new advisory rules that fire on nothing in the current pilots (preventive); a small reason-tail parser living beside the opt-out helper that must stay aligned with the 0006 grammar regex; a new `lzi_hygiene/` sibling module.
**We gain:** an audit trail for suppressions (no more silent un-reasoned allows) and a preventive ceiling on `.lzi`/`.lzx` comment drift as AI authors more features — both taught in `comment-hygiene.md` and bound to named rule codes.
**We watch:** false positives on `LZI-COMMENT-NOISE-001` for genuinely dense-but-justified features → the `# doctor:allow` escape and never-gates severity contain the blast radius; revisit the divider threshold if a pilot trips it legitimately.
