---
id: 0028
title: First-class @doctor.allow waiver node
type: prd
stage: 1 of 3
status: ready
created: 2026-06-02
---

# PRD — First-class @doctor.allow waiver node

## Problem
A doctor waiver today is a COMMENT: `# doctor:allow <CODE> — reason "..."`, recovered by re-scanning the raw `.lzi`/`.lzx` source (`crates/lazuli_doctor/src/allow_comment.rs::source_contains_doctor_allow`, consumed in ~30 rule sites). The parser throws every `#` line away as trivia (`crates/lazuli_syntax/src/parser/common.rs:53` `is_trivia` = blank-or-`#`). So a waiver is indistinguishable from prose to the AST/IR, to an LLM authoring `.lzi`, and to any tool that wants to query "what is waived and why." This blocks the whole comment-discipline effort (0029): you cannot ban prose comments while waivers ARE prose comments. It also means a waiver has no structure — the reason is an un-queryable substring, severity-blind, with no enforcement that error-severity waivers justify themselves.

## Why now (or why ever)
0029 (comment-discipline lint) is gated on this: comments can only be flagged once waivers have a non-comment home. Without 0028, 0029 either can't ship or must carve a permanent "but `# doctor:allow` is fine" hole that defeats the discipline. Concretely broken if never built: the LLM keeps emitting `#` prose into `.lzi` (the root pain the user reported), and the framework can never tell a waiver from noise.

## Outcome — done means
- A first-class annotation `@doctor.allow(CODE, reason: "...")` is parsable on its own line, attached to the construct that follows it (or file-level when at column 0 before any feature). It is NOT trivia.
- The parser captures every waiver into a structured side-channel on the parsed module: `Vec<DoctorAllow { code, reason: Option<String>, span, target_line }>`, surfaced through the IR `Module`.
- `lazuli_doctor` reads waivers from this structured list (a new `allow_registry` seam) instead of re-scanning `#` — for ALL ~30 current consumers, via the existing `source_contains_doctor_allow`-shaped API kept as a thin shim over the new registry.
- Back-compat: the legacy `# doctor:allow <CODE>` comment form is STILL honored during a deprecation window (both sources feed the same registry); a `DOCTOR-ALLOW-LEGACY-COMMENT-001` advisory nudges migration, and `lazuli fix` rewrites comment→node mechanically.
- `reason` is REQUIRED for waivers of error-severity rules (enforced by `DOCTOR-ALLOW-NO-REASON-001`, re-pointed at the node), recommended otherwise.
- Keyword catalog carries `@doctor.allow`; parser↔registry parity (`cargo test -p lazuli_keywords`) green; highlighting recognizes the node (extends 0006).
- `cargo test --workspace` green; both pilots `lazuli generate go .` pass the gate + `go build`; `docs/lazuli_way/comment-hygiene.md` teaches the node form; a doctor diagnostic enforces reason-on-error-waivers.

## Non-goals
- Banning the comment form. That is 0029's job; 0028 only adds the node + keeps the comment honored.
- Per-line `// nolint`-style trailing annotations. Waivers are their own line above the target (dumber to parse, unambiguous).
- A general-purpose annotation/decorator framework. ONE annotation: `@doctor.allow`. No `@doctor.expect`, no `@doctor.severity`.
- Scoping a waiver to a sub-range / block-close syntax. A waiver applies to the immediately-following construct OR the file (the two cases the 30 consumers actually need).
- Changing what any individual rule decides to honor (a hard-error rule that ignores waivers today keeps ignoring them).
- IDE/LSP rename/refactor of waivers.

## User stories
- As the LLM authoring `.lzi`, I write `@doctor.allow(LZI-FILE-SIZE-001, reason: "generated table")` and a tool can tell it apart from a comment.
- As a doctor rule, I ask the structured registry "is CODE waived for this file/construct, with what reason" instead of grepping `#`.
- As 0029's lint, I can flag every `#` prose line because waivers no longer live in `#`.
- As the maker, I run `lazuli fix` once and every legacy `# doctor:allow` becomes a node, no hand-editing.

## Constraints
- Must run unattended; `cargo test --workspace` is the only judge.
- Cannot break the ~30 existing consumers of `source_contains_doctor_allow` — they keep compiling and behaving via the shim.
- Cannot break either pilot's current waivers (legacy comment form stays honored this window).
- `lazuli_doctor` cannot depend on `lazuli_cli`; the registry seam lives in a crate doctor already depends on (`lazuli_ir` for the parsed waivers; the scan-shim stays in `lazuli_doctor`).

## Open questions
None. Surface syntax decided in ADR (`@doctor.allow(CODE, reason: "...")`). Attachment decided (following-construct or file-level). Deprecation window decided (comment honored + advisory + `lazuli fix`, removal deferred to a future spec).
