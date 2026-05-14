# Lazuli IR Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_ir/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---|---|---|
| `src/lib.rs` | 2348 | 1 (`serde`) | **wire** | IR ABI structs/enums plus `serde` derives and a few small vocabulary helpers. No parser, templater, JSON implementation, date library, cache, transport, or runtime primitive is reimplemented here. |

---

## Summary

**1/1 files (100.0%) are wire-clean for Rust IR purposes.** The crate is intentionally Lazuli-specific vocabulary: typed IR records, closed catalogs, sidecar source-map shape, app/registry/profile declarations, experience declarations, and AI/test/policy surfaces. Its large LOC count is not a wire-thin violation because the only external concern is JSON ABI serialization, which is delegated to `serde`; the rest is framework-owned schema.

### Top 3 framework risks

1. **Large monolithic ABI file** - `src/lib.rs` is easy to append to but harder to review by domain. Splitting by IR subdomain could improve review locality later, but this is a maintainability risk rather than a wire-thin violation.

2. **Stringly adapter-parsed fields are deliberate but numerous** - durations, paths, cache keys, template references, and provider-ish knobs are often preserved as authored strings for analyzers/runtime adapters to interpret. That preserves Lazuli vocabulary boundaries, but downstream consumers need consistent doctor/codegen checks so these strings do not become hidden mini-languages.

3. **Built-in trace/event catalogs live beside pure type definitions** - the small helper functions around trace events are still framework vocabulary and do not reimplement a library, but they are the most behavior-like part of the crate. If that catalog grows substantially, it should stay declarative or move to a focused submodule.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 1 |
| Effective LOC audited | 2348 |
| Wire-clean | 1 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| External dependency crates used | 1 (`serde`) |
| Downstream-blocker risk | Low (schema review/locality risks only; no rewrite-as-wire work required) |
