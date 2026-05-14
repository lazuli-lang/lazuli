# Rust LSP Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_lsp/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---|---|---|
| `src/lib.rs` | 11,017 | `lazuli_analyzer`, `lazuli_ir`, `lazuli_syntax`, `tokio`, `tower_lsp` | **questionable** | Single-file LSP server, diagnostics, formatter, hovers, completions, and closed catalogs. It wires mature LSP/runtime crates and Lazuli's parser/analyzer instead of reimplementing protocol or async plumbing, but the file is a high-blast-radius framework-specific monolith. |

---

## Non-Wire Notes

### `src/lib.rs` — questionable

The file is far beyond the usual wire-thin size threshold, but it is not a clean rewrite-as-wire candidate under the Rust-specific rules. The large body is mostly Lazuli DSL vocabulary: canonical-order checks, security-policy diagnostics, capability contracts, LZX route checks, formatter rules, hover text, and completion catalogs. Those rules are framework-specific and do not have an obvious commodity Rust library target. The important boundary is that the LSP protocol itself is delegated to `tower-lsp`, async IO/state uses `tokio`, parsing delegates to `lazuli_syntax::parse_document`, analyzer-backed diagnostics delegate to `lazuli_analyzer::lower_document`, and reserved event names delegate to `lazuli_ir`.

---

## Summary

**0/1 files (0.0%) are strictly wire-clean; 1/1 files (100.0%) avoid rewrite-as-wire findings.** The crate does not violate the founding principle by reimplementing a commodity LSP server, async runtime, parser framework, JSON-RPC stack, date library, templater, or similar general-purpose facility. Its risk is architectural concentration: nearly all language-server behavior lives in one production file, with many line-oriented DSL checks that can drift from parser/analyzer semantics.

### Top 3 framework risks

1. **Single-file blast radius** — diagnostics, formatting, hover text, completions, and server wiring all share `src/lib.rs`. Small feature cells can accidentally touch unrelated LSP behavior, and review has to reason across thousands of lines.

2. **Parser/analyzer drift** — some diagnostics delegate to `lazuli_syntax` and `lazuli_analyzer`, while many canonical diagnostics are bespoke text scans. That is acceptable for language-specific authoring help, but it creates a second interpretation layer that can lag compiler truth.

3. **Closed catalog duplication** — keyword/completion/detail catalogs live in the LSP crate. They are thin data tables, not commodity reimplementation, but they can diverge from the compiler, doctor checks, docs, or codegen unless future cells keep shared vocabulary centralized.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 1 |
| Wire-clean | 0 (0.0%) |
| Questionable | 1 |
| Rewrite-as-wire | 0 |
| Delete-candidate | 0 |
| Eff. LOC audited | 11,017 |
| External dependency crates used | 5 |
| Downstream-blocker risk | Medium (no wire-thin rewrite required, but the monolithic LSP file should be split or generated only under orchestrated review) |
