---
id: 0028
title: First-class @doctor.allow waiver node
type: techspec
status: ready
created: 2026-06-02
depends_on: []
parallel_safe: true
test_gate: "cargo test --workspace"
agent: unassigned
---

# TechSpec — First-class @doctor.allow waiver node

## Approach
Add ONE annotation, `@doctor.allow(CODE, reason: "...")`, parsed on its own line via the call-signature machinery already in `lzi/helpers.rs` (`split_call_signature` + `parse_named_args`). The parser captures every waiver (node form AND legacy `# doctor:allow` comment form) into a new `Module.doctor_allows: Vec<DoctorAllow>` in `lazuli_ir` — the frozen seam. `lazuli_doctor` gains an `allow_registry` module reading that Vec; the existing `source_contains_doctor_allow`/`file_contains_doctor_allow` become a thin bridge that ORs the registry with the legacy scan, so all ~30 consumers migrate behind the API with zero call-site edits. A `DOCTOR-ALLOW-LEGACY-COMMENT-001` advisory + a `lazuli fix` codemod move pilots off the comment form without removing comment support this window. Dumb and functional: no new statement grammar, no block scoping, two scope cases only (`File` / `Construct{line}`).

## Surface
**Create:**
- `crates/lazuli_ir/src/nodes/doctor_allow.rs` — the FROZEN contract: `DoctorAllow`, `DoctorAllowScope`, re-exported from `lazuli_ir`.
- `crates/lazuli_syntax/src/parser/lzi/doctor_allow.rs` — recognizes `@doctor.allow(...)` lines, parses `(CODE, reason: "...")`, returns `DoctorAllow`. Also a `scan_legacy_comment_allows(source) -> Vec<DoctorAllow>` lifting the legacy `# doctor:allow` form (mark `legacy: true`).
- `crates/lazuli_doctor/src/allow_registry.rs` — `module_allows(module, code) -> bool`, `module_allow_reason(module, code) -> Option<&str>`, `module_allows_at(module, code, line) -> bool` over `Module.doctor_allows`.
- `crates/lazuli_doctor/src/lzi_hygiene/legacy_comment_allow_001.rs` — `DOCTOR-ALLOW-LEGACY-COMMENT-001` (advisory, LziHygiene, NEVER gates): fires on each `# doctor:allow` comment-form waiver present in the source, message points at `@doctor.allow(...)` + `lazuli fix`.
- `crates/lazuli_cli/src/commands/fix/doctor_allow_comment.rs` — codemod: rewrite each `# doctor:allow <CODE> [— reason "..."]` line into `@doctor.allow(<CODE>, reason: "...")` (drop the `# ` prefix, normalize the reason tail; when no reason, emit `@doctor.allow(<CODE>)`). Idempotent (skip lines already in node form).

**Modify:**
- `crates/lazuli_ir/src/nodes/mod.rs` + `lib.rs` — `pub mod doctor_allow; pub use`.
- `crates/lazuli_ir/src/nodes/feature.rs` (or the `Module` struct home — grep `pub struct Module`) — add `pub doctor_allows: Vec<DoctorAllow>` (default empty; `#[serde(default)]` if Module derives serde).
- `crates/lazuli_syntax/src/parser/lzi/mod.rs` — in the top-level line loop, when a non-trivia line begins with `@doctor.allow`, route to `doctor_allow::parse` instead of treating it as a construct; record the captured `DoctorAllow` (scope = `File` if before any feature/at col 0, else `Construct { line }` = the next non-trivia construct's line). After the parse pass, also run `scan_legacy_comment_allows` over the raw source and extend the list. DO NOT add `@doctor.allow` lines to `is_trivia` (they are NOT trivia).
- `crates/lazuli_keywords/src/registry/sections/*.rs` (the section that holds `@`-decorator rows — grep `decorator(`) — add a `decorator("@doctor.allow", "...")` (or a dedicated builder) row so the parity test (`cargo test -p lazuli_keywords`) sees it parsed.
- `crates/lazuli_syntax/src/parser/highlight.rs` — recognize `@doctor.allow(...)` as the decorator/annotation token (extend the 0006 `# doctor:allow` highlight path so the NODE form highlights too; keep the comment-form highlight).
- `crates/lazuli_doctor/src/lib.rs` — `pub mod allow_registry;` + register `DOCTOR-ALLOW-LEGACY-COMMENT-001` (advisory; excluded from gate aggregation, mirror `LZI-COMMENT-NOISE-001`).
- `crates/lazuli_doctor/src/allow_comment.rs` — keep `source_contains_doctor_allow` / `file_contains_doctor_allow` signatures; ADD an internal note + a registry-aware variant used by rules that have a `Module` in hand. The pure-source scan stays (legacy + tests), and `scan_legacy_comment_allows` shares its line matcher (factor the `# doctor:allow <CODE>` line recognizer into one function used by both the scanner and the codemod).
- `crates/lazuli_doctor/src/allow_no_reason.rs` — extend `DOCTOR-ALLOW-NO-REASON-001` to ALSO read node-form waivers: a node `@doctor.allow(CODE)` with no `reason:` fires the same advisory; `@doctor.allow(CODE, reason: "x")` does not. Keep the comment-form behavior (0007).
- `crates/lazuli_cli/src/commands/fix/mod.rs` (or the `lazuli fix` dispatcher — grep `fn fix` / `Commands::Fix`) — wire the codemod under `lazuli fix` (rule-scoped: `--rule DOCTOR-ALLOW-LEGACY-COMMENT-001`).

**Teach:**
- `docs/lazuli_way/comment-hygiene.md` — append a section: the node form `@doctor.allow(CODE, reason: "...")` is the canonical waiver; the `#` comment form is deprecated (run `lazuli fix`); reason required for error-severity rules. Name `DOCTOR-ALLOW-LEGACY-COMMENT-001` + `DOCTOR-ALLOW-NO-REASON-001`.
- scaffold `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — bullet: "Waive a doctor finding with `@doctor.allow(CODE, reason: \"why\")` on the line above the construct — NOT a `#` comment (deprecated; `lazuli fix` migrates)."

## Contracts
**FROZEN — `lazuli_ir` (0029 builds on this; do not change shape after this spec lands):**
```rust
// crates/lazuli_ir/src/nodes/doctor_allow.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorAllow {
    pub code: String,                 // the rule code, verbatim (case preserved)
    pub reason: Option<String>,       // the reason: "..." value; None when omitted
    pub scope: DoctorAllowScope,
    pub legacy: bool,                 // true = recovered from a `# doctor:allow` comment
    pub span: Option<lazuli_ir::source_map::Span>, // node-form span; None for legacy scan
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorAllowScope {
    File,                  // applies to the whole file (col-0, before any feature)
    Construct { line: usize }, // 1-based source line of the construct it waives
}
```
`Module` gains `pub doctor_allows: Vec<DoctorAllow>`.

**`lazuli_doctor::allow_registry` (the read API every rule uses going forward):**
- `module_allows(module: &Module, code: &str) -> bool` — case-insensitive on `code` (match the legacy scanner). True if any `DoctorAllow` in the module has this code (any scope).
- `module_allow_reason(module: &Module, code: &str) -> Option<&str>` — first matching waiver's reason.
- `module_allows_at(module: &Module, code: &str, line: usize) -> bool` — true for a `File`-scoped waiver OR a `Construct{line}` matching `line`.

**Bridge (unchanged signatures, ~30 consumers):** `source_contains_doctor_allow(source, code)` and `file_contains_doctor_allow(path, code)` keep returning `bool` and keep honoring the legacy comment scan. Consumers needing node-awareness migrate to `allow_registry` over time; the bridge guarantees no regression.

**Grammar:** `@doctor.allow(<CODE>)` or `@doctor.allow(<CODE>, reason: "<text>")`. `<CODE>` is an unquoted token `[A-Za-z0-9._@-]+`. Whitespace around `(`/`,`/`:` is insignificant. A malformed annotation (`@doctor.allow` with no parens, unterminated quote) is a parse ERROR with a span (reuse `line_error`), NOT silent.

## Plan — for the executing agent
1. Read `crates/lazuli_ir/src/nodes/feature.rs` (`Module` struct) + `nodes/mod.rs` for the add point; `lzi/helpers.rs` (`split_call_signature`, `parse_named_args`) and `lzi/mod.rs` (top-level line loop + `is_trivia` usage); `allow_comment.rs`, `allow_no_reason.rs`; `registry/builders.rs` + the section with `decorator(` rows; `highlight.rs` for the 0006 `doctor:allow` path; the `lazuli fix` dispatcher.
2. Create `lazuli_ir` `doctor_allow.rs` with the FROZEN structs; wire `Module.doctor_allows` (default empty). `cargo build -p lazuli_ir`.
3. Create `lzi/doctor_allow.rs`: `parse(line, rest) -> Result<DoctorAllow>` reusing `split_call_signature` + `parse_named_args`; `scan_legacy_comment_allows(source) -> Vec<DoctorAllow>` reusing the shared `# doctor:allow` line recognizer. Unit-test both.
4. Wire capture into `lzi/mod.rs`: detect `@doctor.allow` head before construct dispatch; compute scope (File vs next-construct line); push to module list. Then extend with the legacy scan. Add a parse-error test for malformed annotations.
5. Add the keyword-catalog row (`@doctor.allow`) in the right section; run `cargo test -p lazuli_keywords` (parity) and fix the row until green. Run the xtask keyword-reference freshness regen if keywords changed (`cargo run -p xtask -- <keyword-reference task>`; grep xtask for the task name) and commit the regenerated reference.
6. Extend `highlight.rs` so the node form highlights as the annotation/decorator token; add a `highlight_tests.rs` case.
7. Create `allow_registry.rs` in `lazuli_doctor`; grow `source_contains_doctor_allow`'s shared recognizer into one function used by scanner + codemod; keep signatures stable. `cargo test -p lazuli_doctor`.
8. Extend `DOCTOR-ALLOW-NO-REASON-001` to read node-form waivers (fire on `@doctor.allow(CODE)` no-reason). Add tests.
9. Create `DOCTOR-ALLOW-LEGACY-COMMENT-001` (advisory, LziHygiene, never gates); register it; exclude from gate aggregation (mirror `LZI-COMMENT-NOISE-001`); add the diagnostics-registry bridge entry (a new doctor code requires the registry bridge — grep where rule codes are registered) and the `module_headers` trigger-cue on the new rule module.
10. Create the `lazuli fix` codemod `doctor_allow_comment.rs`; wire under `lazuli fix --rule DOCTOR-ALLOW-LEGACY-COMMENT-001`; idempotent. Test on a synthetic `.lzi`.
11. Run `cargo test --workspace` (FULL sweep). Fix any latent break.
12. MIGRATE pilots: run `lazuli fix --rule DOCTOR-ALLOW-LEGACY-COMMENT-001` on hostpoint + pauta-web (rewrites their existing `# doctor:allow` waivers to nodes); then `lazuli generate go .` (gate passes) + `go build ./...` in each. Confirm the migrated waivers still suppress what they suppressed (zero NEW blocking findings).
13. TEACH: fill `docs/lazuli_way/comment-hygiene.md` section + add the scaffold bullet to BOTH `.tmpl` files (byte-identical edited region, per 0001's mirror rule).

## Tests first (TDD)
- [ ] `lazuli_ir::doctor_allow::frozen_shape` — `DoctorAllow`/`DoctorAllowScope` construct + equality (locks the contract).
- [ ] `lzi::doctor_allow::parses_code_and_reason` — `@doctor.allow(LZI-FILE-SIZE-001, reason: "x")` → `{code, reason: Some("x")}`.
- [ ] `lzi::doctor_allow::parses_code_only` — `@doctor.allow(X-1)` → `{code: "X-1", reason: None}`.
- [ ] `lzi::doctor_allow::malformed_is_error` — `@doctor.allow` (no parens) / unterminated quote → `Err` with a span.
- [ ] `lzi::doctor_allow::not_trivia` — a file that is ONLY `@doctor.allow(...)` lines parses them into `doctor_allows`, not discarded.
- [ ] `lzi::doctor_allow::file_vs_construct_scope` — col-0 before any feature → `File`; above a `feature`/construct → `Construct{line}` at that construct's line.
- [ ] `lzi::doctor_allow::legacy_comment_captured` — `# doctor:allow X-1 — reason "y"` is captured with `legacy: true, reason: Some("y")`.
- [ ] `allow_registry::module_allows_matches_code` — registry returns true for a captured code (case-insensitive), false otherwise.
- [ ] `allow_registry::file_scope_covers_any_line` + `construct_scope_is_line_keyed`.
- [ ] `allow_comment::bridge_ors_node_and_comment` — `source_contains_doctor_allow` still true for legacy comment (no regression).
- [ ] `keywords::parity_includes_doctor_allow` — the parity sweep (`cargo test -p lazuli_keywords`) recognizes `@doctor.allow`.
- [ ] `allow_no_reason::node_bare_fires` — `@doctor.allow(X-1)` fires; `@doctor.allow(X-1, reason: "ok")` does not.
- [ ] `legacy_comment_allow_001::fires_on_comment_form` + `silent_on_node_form` + `never_gates`.
- [ ] `fix::doctor_allow_comment::rewrites_comment_to_node` + `idempotent_on_node_form`.
- [ ] highlight: `highlight_tests` case proving `@doctor.allow(...)` tokenizes as the annotation/decorator scope.

## Gate
`cargo test --workspace` green **and** `cargo test -p lazuli_keywords` (parity) green **and**: hostpoint + pauta-web run `lazuli fix --rule DOCTOR-ALLOW-LEGACY-COMMENT-001` (waivers become nodes), then `lazuli generate go .` passes the doctor gate and `go build ./...` succeeds in both, with the previously-suppressed findings STILL suppressed (no new blocking errors) **and** `docs/lazuli_way/comment-hygiene.md` + both scaffold `.tmpl`s updated **and** xtask keyword-reference is fresh (the keyword-reference freshness check passes).

### Definition of Done (the repo's governing rule — embedded)
1. BUILD — implemented + `cargo test --workspace` green (FULL sweep, not per-crate).
2. MIGRATE — hostpoint + pauta-web: `lazuli fix` migrates waivers → `lazuli generate go .` gate-passes → `go build ./...` clean.
3. TEACH — `docs/lazuli_way/comment-hygiene.md` teaches the node form; scaffold CLAUDE.md/AGENTS.md bullet added.
4. ENFORCE — `DOCTOR-ALLOW-LEGACY-COMMENT-001` fires on the comment form; `DOCTOR-ALLOW-NO-REASON-001` fires on a node bare-allow.
Plus: parser↔registry parity (`cargo test -p lazuli_keywords`); xtask keyword-reference freshness (keywords changed); diagnostics-registry bridge for the new doctor code; module_headers trigger-cue on the new rule module.

## Risks & rollback
- Capturing `@doctor.allow` as non-trivia breaks a parser that assumed every `@`-line is a decorator on a construct → mitigation: route `@doctor.allow` BEFORE the generic decorator path; add the `not_trivia` + `file_vs_construct_scope` tests; run the full `lazuli_syntax` suite.
- The bridge changing `source_contains_doctor_allow` behavior regresses one of the 30 consumers → mitigation: keep the legacy scan as the lower bound (OR semantics — registry can only ADD trues); `bridge_ors_node_and_comment` test; full-workspace sweep is the backstop.
- `lazuli fix` mangles a hand-aligned `.lzi` → mitigation: codemod is line-local (rewrites only the matched `# doctor:allow` line), idempotent, and tested; pilots are regenerated/diffed after.
- Keyword-reference / parity test churn → mitigation: regen via xtask in the same commit; the freshness check is in the gate.

**Rollback:** `git revert` the commit. `Module.doctor_allows` defaults empty and the bridge keeps the legacy scan, so reverting drops the node form with no pilot impact (pilots still have working comment-form waivers unless `lazuli fix` was committed against them — revert those pilot commits too).
