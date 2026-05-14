# Rust OpenAPI Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_openapi/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---|---|---|
| `src/lib.rs` | 770 | `lazuli_ir` | **questionable** | Single-file OpenAPI 3.1 emitter over Lazuli IR, including command/API/agent/webhook path emission, schema projection, Lazuli extension fields, and a tiny hand-written YAML line emitter. |

---

## Per-File Notes

### `src/lib.rs`

This file is large and does not wire a mature OpenAPI or YAML emitter directly, but the bulk of the code is framework-specific projection from `lazuli_ir` vocabulary into an OpenAPI artifact. The custom `YamlEmitter` is a small line/indent writer rather than a general YAML parser, templater, or schema library replacement, so this is not a clear `rewrite-as-wire` case under the Rust audit rules. The main concern is maintainability: all OpenAPI path, operation, component schema, webhook extension, retry/replay/DLQ, and problem-response emission lives in one file.

---

## Summary

**1/1 files (100.0%) are acceptable for Rust wire-thin discipline; 0/1 files are strictly plain `wire` because `src/lib.rs` is intentionally framework-specific and therefore marked `questionable`.** The crate is not reimplementing a well-known Rust library such as a JSON parser, date library, general YAML parser, templater, or HTTP framework. It is a Lazuli IR-to-OpenAPI artifact emitter with a compact local YAML writer.

### Top 3 Framework Risks

1. **Single-file emitter growth** - at 770 effective production LOC, OpenAPI path emission, schema emission, extension metadata, and YAML formatting are all coupled in `src/lib.rs`. Future changes may become hard to review unless the crate is split by artifact concern.

2. **Hand-written YAML surface** - the local `YamlEmitter` is intentionally tiny, but quoting/escaping coverage is narrow. This is not a wire-thin violation today, but unusual feature names, enum variants, paths, or descriptions may expose YAML validity edge cases.

3. **OpenAPI completeness depends on IR drift discipline** - the emitter manually tracks Lazuli IR concepts such as commands, `api` blocks, agent HTTP exposure, webhooks, semantic types, and extensions. New IR fields can silently be omitted unless tests are added with each language feature.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 1 |
| Wire-clean / acceptable | 1 (100.0%) |
| Strict `wire` verdicts | 0 |
| Questionable | 1 |
| Rewrite-as-wire | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Medium-low (maintainability/serialization edge risk only; no wire-thin rewrite required) |
