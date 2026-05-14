# Lazuli Analyzer Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_analyzer/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---:|---|---|
| `src/lib.rs` | 1964 | 3 (`lazuli_ir`, `lazuli_syntax`, `thiserror`) | **wire** | Large but framework-specific: this is the syntax-to-IR lowering layer for legacy `.lzi`, canonical-indent feature skeletons, `.lzx`, auth, jobs, webhooks, notifications, agents, policies, resources, queries, and type references. It wires local AST and IR crates plus `thiserror`; it does not reimplement JSON, templating, dates, parser combinators, or other commodity Rust libraries. |
| `src/source_map.rs` | 54 | 1 (`lazuli_ir`) | **wire** | Small source-location resolver over `lazuli_ir::SourceMap`; the line-offset indexing is project glue for diagnostics and codegen source tags, not a commodity library replacement. |

---

## Summary

**2/2 files (100.0%) are wire-clean.** The analyzer crate is not wire-thin in the Go-runtime sense of wrapping mature third-party infrastructure, but Rust DSL crates are expected to contain framework-specific compiler work. Both files sit on the correct side of that boundary: they translate Lazuli syntax structures into Lazuli IR structures, preserve spans/source locations, and use local compiler crates rather than reimplementing general-purpose libraries.

### Top 3 framework risks

1. **`src/lib.rs` is a monolithic lowering surface** - at 1964 effective production LOC, it concentrates nearly all analyzer behavior in one file. This is not a wire-thin violation, but future changes would be easier to audit if canonical feature lowering, legacy aggregate lowering, LZX lowering, auth/runtime-unit lowering, and agent lowering moved into focused submodules.

2. **Several mini-parsers live in analyzer helpers** - type references, `@cap.*` arguments, cache TTL literals, path strings, tool references, and eval predicates are currently string-lifted during lowering. They are Lazuli vocabulary, so no obvious third-party rewrite is warranted, but the long-term owner should be the syntax crate or a shared closed-vocabulary parser once those grammars stabilize.

3. **Analyzer currently bridges old and canonical syntax paths** - `lower_document` still supports the legacy aggregate parser while `lower_feature_skeleton` handles canonical-indent slices. The dual path is deliberate during migration, but downstream diagnostics and codegen can drift if new IR fields are added to one lowering path and not the other.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 2 |
| Wire-clean | 2 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Effective production LOC | 2018 |
| Downstream-blocker risk | Low (no commodity reimplementation found; maintainability risk is module organization and parser ownership) |
