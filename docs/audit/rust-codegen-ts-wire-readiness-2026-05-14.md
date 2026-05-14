# Rust codegen-ts Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_codegen_ts/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---|---|---|
| `src/lib.rs` | 506 | 2 (`lazuli_ir`, `serde_json`) | **wire** | Emits a Vite/React frontend scaffold and serializes the Lazuli IR into generated TypeScript. The raw string templates are framework output, not a reimplementation of a known Rust library concern. |
| `src/runtime.rs` | 262 | 1 (`lazuli_codegen_spec`) | **wire** | Emits runtime-form TypeScript command/query/resource bindings from the canonical runtime spec. Helper code is naming and source-formatting glue around Lazuli vocabulary, not commodity behavior that should be delegated. |

---

## Summary

**2/2 files (100.0%) are wire-clean.** This crate is codegen surface by design: it maps `lazuli_ir::Module` and `lazuli_codegen_spec::RuntimeFeature` into TypeScript files. The files are larger than ideal, but both are framework-specific emitters with explicit IR/spec dependencies. No file reimplements a JSON parser, templater, date library, TypeScript compiler, or other mature Rust library target.

### Top 3 framework risks

1. **Frontend scaffold breadth can accrete product opinion** - `src/lib.rs` currently emits package metadata, Vite config, React app shell, generated schema types, and CSS. That is acceptable as scaffold output, but future additions should stay distro/template-shaped instead of becoming a product-specific UI framework inside the compiler crate.

2. **Runtime spike wording is stale by design** - `src/runtime.rs` still emits comments describing the output as matching a hand-written runtime spike. That is not a wire-thin violation, but it can confuse downstream readers once runtime TS generation becomes the canonical path.

3. **Name casing helpers are local policy** - `pascal_case`, `lower_camel`, and command/query naming encode Lazuli conventions directly. Keep them covered by focused tests when expanding the runtime spec so generated TS names remain stable across product ports.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 2 |
| Effective LOC audited | 768 |
| Wire-clean | 2 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Downstream-blocker risk | Low (codegen/template drift risks only; no wire-thin rewrite required) |
