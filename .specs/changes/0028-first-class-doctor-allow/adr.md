---
id: 0028
title: @doctor.allow as a first-class line annotation captured into a module-level waiver registry
type: adr
status: accepted
created: 2026-06-02
supersedes: —
---

# ADR — `@doctor.allow(CODE, reason: "...")` is a parsed line annotation; the parser captures all waivers into a `Module.doctor_allows` registry the doctor reads instead of re-scanning `#`.

## Context
- Waivers live in `#` comments today. `crates/lazuli_syntax/src/parser/common.rs:53` (`is_trivia`) discards every `#` line; the doctor recovers waivers by re-reading the raw file (`crates/lazuli_doctor/src/allow_comment.rs`), consumed in ~30 rule sites (grep `source_contains_doctor_allow` / `file_contains_doctor_allow`).
- The `@` sigil is already a parsed first-class thing in `.lzi`: decorators (`@policy.x`, `@fn.x`), catalog atoms (`@role`/`@scope`/`@actor`), keyword-catalog rows built via `decorator()` / `catalog_atom()` in `crates/lazuli_keywords/src/registry/builders.rs`. A new `@doctor.allow` row fits the existing taxonomy and the parity test that proves every keyword is parsed.
- 0006 already highlights the `# doctor:allow` comment form; 0007 added `DOCTOR-ALLOW-NO-REASON-001` (reason-tail detector) + `LZI-COMMENT-NOISE-001` (both source-scan based).
- The doctor cannot depend on `lazuli_cli`. It already depends on `lazuli_ir`. So the structured waiver list must ride on the IR `Module`, and the scan-shim stays inside `lazuli_doctor`.
- The 30 consumers all ask the same yes/no question: "is CODE waived for this file (sometimes: at/near this construct)?" None need block-scoping today.

## Decision
1. **Surface syntax:** `@doctor.allow(CODE, reason: "...")` on its own line. It attaches to the **immediately-following non-trivia construct**; when it sits at column 0 before any feature it is **file-level**. Chosen because (a) the `@`-sigil + `name.namespace(args)` shape is already how `.lzi` writes decorators and declarative calls (`split_call_signature` + `parse_named_args` in `lzi/helpers.rs` parse exactly this), so it reuses live machinery and the parity test; (b) one-line-above-target is unambiguous and needs no block-close token; (c) it reads as an annotation, not prose — the whole point.
2. **Capture, don't interpret:** the parser recognizes the annotation, parses `(CODE, reason: "...")`, and pushes a `DoctorAllow { code, reason, span, scope }` onto a module-level `Vec`. It does NOT validate the code against the rule registry (the doctor owns rule identity) and does NOT alter any construct. `scope` is `File` or `Construct { line }` (the source line of the attached construct), the only two the consumers need.
3. **Contract home:** `DoctorAllow` + `Vec<DoctorAllow>` live in `lazuli_ir` (`Module.doctor_allows`). This is the FROZEN seam 0029 builds on. Doctor reads it; nobody outside doctor needs to.
4. **Doctor reads the registry:** a new `allow_registry` module in `lazuli_doctor` exposes `module_allows(module, code) -> bool` and `module_allow_reason(module, code) -> Option<&str>` over `Module.doctor_allows`. The existing `source_contains_doctor_allow(source, code)` / `file_contains_doctor_allow(path, code)` are KEPT as a back-compat bridge that consults BOTH the node registry (when a module is in scope) AND the legacy `#` scan — so all 30 consumers keep their current call shape and migrate behind the API, not at every call site.
5. **Back-compat window:** the legacy `# doctor:allow <CODE>` comment is STILL honored (the parser ALSO captures comment-form waivers into the same `Module.doctor_allows`, marked `legacy: true`). `DOCTOR-ALLOW-LEGACY-COMMENT-001` (advisory, LziHygiene) nudges migration; `lazuli fix --rule DOCTOR-ALLOW-LEGACY-COMMENT-001` rewrites comment→node mechanically. Comment removal is a FUTURE spec (not 0028, not 0029).
6. **Reason policy:** `reason` is OPTIONAL in the grammar but REQUIRED-for-error-waivers by `DOCTOR-ALLOW-NO-REASON-001`, which 0007 already implemented over the comment form and which we re-point to also read the node registry. Recommended for all waivers; only enforced (error→error) where the waived rule is error-severity.

## Alternatives considered
- **`waive <CODE> "reason"` as a bare keyword statement** — rejected: a new top-level statement keyword needs its own parser path and context rules in every block; the `@`-decorator path already exists and already round-trips through the parity test. More moving parts for no ergonomic win.
- **Trailing per-line annotation `... # @doctor.allow(...)` / `... @[allow]`** — rejected: trailing annotations require every construct parser to look right, and the "which construct does this waive" question gets murky on multi-line constructs. Own-line-above is dumber and unambiguous.
- **A general decorator/attribute framework (`@doctor.*`, `@meta.*`)** — rejected: astronautics. One annotation solves the real problem; a framework invites scope creep and a parser DSL nobody asked for.
- **Storing waivers in `<feature>.ctx.md` context files** — rejected: a waiver must sit AT the waived construct to be reviewable and to scope correctly; punting it to a sidecar file divorces it from what it waives.
- **Hard-cut the comment form now** — rejected: breaks both pilots' existing waivers on day one and couples 0028 to a migration that belongs in its own window. Honor-both is the safe seam.

## Consequences
**We accept:** two waiver sources for one deprecation window (node + legacy comment), both feeding one registry — slightly more capture code and a `legacy` flag. We accept that `source_contains_doctor_allow` keeps its name while its body grows a registry consult (a thin shim, documented). We accept that file-level vs construct-level scope is coarse (line-keyed, not block-ranged) — matches what consumers need, no more.
**We gain:** waivers are queryable structured data, distinguishable from prose — which UNBLOCKS 0029. One frozen IR seam (`Module.doctor_allows`) the lint stage builds on. The 30 consumers migrate behind one API. `lazuli fix` makes the comment→node move a one-shot.
**We watch:** if a real rule needs block-RANGED waiver scoping (not just file/construct), reopen the `scope` shape. If the parity test or xtask keyword-reference freshness starts fighting the `@doctor.allow` row, the registry-entry shape is wrong — fix the row, not the test.
