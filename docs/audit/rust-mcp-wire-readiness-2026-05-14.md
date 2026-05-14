# Rust MCP Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_mcp/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---|---|---|---|
| `src/lib.rs` | 16 | `lazuli_ir` | **wire** | Skeleton MCP crate surface: publishes the server identifier and delegates supported IR schema truth directly to `lazuli_ir::LZIR_SCHEMA`. Inline tests only pin that delegation and the current 0.x schema range. |

---

## Summary

**1/1 files (100.0%) are wire-clean.** The crate currently contains only a minimal MCP identity/schema compatibility surface and no server protocol implementation. The only dependency is the internal `lazuli_ir` crate, and the crate does not reimplement JSON-RPC, MCP transport, schema serialization, parsing, templating, date/time handling, or any other commodity Rust library concern.

### Top framework risks

1. **MCP remains mostly skeleton-only** — the crate documents intended write/read boundaries, but actual MCP transport/tool wiring is not present here yet. Future implementation should wire a mature protocol/serialization stack rather than hand-rolling JSON-RPC framing.

2. **Schema compatibility is delegated but coarse** — `SUPPORTED_LZIR_SCHEMA` correctly tracks `lazuli_ir::LZIR_SCHEMA`, but this crate does not yet expose per-tool feature negotiation for optional MCP projections like `tools` or `expose`.

3. **Boundary comments are doing policy work** — the single-write-surface rule is captured in crate docs rather than enforced by executable MCP tool registration yet. That is acceptable for the current skeleton, but future additions should keep the write surface closed in code.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 1 |
| Wire-clean | 1 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (skeleton/readiness risk only; no wire-thin rewrite required) |
