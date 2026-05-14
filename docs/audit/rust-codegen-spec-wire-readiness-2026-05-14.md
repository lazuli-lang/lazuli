# Rust Codegen Spec Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_codegen_spec/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---:|---|---|
| `src/lib.rs` | 278 | 1 (`serde`) | **wire** | Runtime spec DTOs and a temporary `customer_spike()` fixture; serialization is delegated to `serde`, and the rest is Lazuli-specific schema/vocabulary rather than a reimplementation of commodity Rust behavior. |

---

## Summary

**1/1 files (100.0%) are wire-clean.** The crate defines the minimal runtime spec consumed by the Go and TS code generators. Its only external dependency is `serde`, which is appropriate for derive-based interchange structs. The large file size comes from explicit framework vocabulary and the hand-built `customer_spike()` manifest, not from reimplementing a JSON parser, templater, date library, CLI parser, or other mature Rust ecosystem concern.

### Top 3 framework risks

1. **Fixture-as-spec gravity** -- `customer_spike()` is useful for the current runtime spike, but it hardcodes one feature shape in the crate that should eventually become a parser/lowering output. The file remains wire-clean today because it is data construction, but future growth should move toward generated or lowered specs rather than more hand-authored fixtures.

2. **Spec/codegen contract is still narrow** -- the crate intentionally projects only the runtime spike surface: resources, commands, queries, cache, filters, search, emits, and lookup keys. That is not a wire-thin violation, but downstream ports may hit missing vocabulary before the full `lazuli_ir::Module` lowering exists.

3. **Single-file accumulation risk** -- keeping all DTOs and the spike fixture in `lib.rs` is acceptable at this size, but additional framework concepts should be split by module once they are real spec surface. The risk is readability and contract drift, not reimplementation bloat.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 1 |
| Wire-clean | 1 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Total effective LOC | 278 |
| External dependency crates used | 1 (`serde`) |
| Downstream-blocker risk | Low (spec coverage and fixture migration risks only; no wire-thin rewrite required) |
