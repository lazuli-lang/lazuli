# Rust Codegen-Go Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_codegen_go/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---|---|---|
| `lib.rs` | 279 | 1 (`lazuli_ir`) | **wire** | Public API plus legacy demo emitter; framework-specific Go output glue. |
| `runtime.rs` | 600 | 2 (`lazuli_codegen_spec`, `lazuli_ir`) | **wire** | Runtime-form emitter over Lazuli specs; no generic Go/runtime subsystem is reimplemented. |
| `emitter/mod.rs` | 29 | _none_ | **wire** | Module index only. |
| `emitter/api.rs` | 281 | 1 (`lazuli_ir`) | **wire** | API contract codegen from IR to Go literals. |
| `emitter/audit.rs` | 137 | 1 (`lazuli_ir`) | **wire** | Audit metadata/DDL emission for Lazuli runtime contracts. |
| `emitter/auth.rs` | 446 | 1 (`lazuli_ir`) | **wire** | Emits auth registration and policy wiring; auth mechanics stay in Go runtime. |
| `emitter/auth_session.rs` | 205 | 1 (`lazuli_ir`) | **wire** | Emits session schema/runtime contract declarations. |
| `emitter/casing.rs` | 82 | _none_ | **wire** | Small identifier casing helpers for generated Go names. |
| `emitter/check.rs` | 645 | 1 (`lazuli_ir`) | **wire** | Go emission readiness checks against Lazuli IR; not a generic validator library. |
| `emitter/command.rs` | 701 | 1 (`lazuli_ir`) | **wire** | Command codegen and runtime registration glue. |
| `emitter/cross_feature.rs` | 55 | 1 (`lazuli_ir`) | **wire** | Small cross-feature index for import/type resolution. |
| `emitter/deps.rs` | 11 | _none_ | **wire** | Go dependency constants. |
| `emitter/enums.rs` | 193 | 1 (`lazuli_ir`) | **wire** | Enum-to-Go constant/value emission. |
| `emitter/error_envelope.rs` | 247 | 1 (`lazuli_ir`) | **wire** | Error envelope wrapping codegen; runtime error behavior remains delegated. |
| `emitter/events.rs` | 519 | 1 (`lazuli_ir`) | **wire** | Event contract and payload codegen. |
| `emitter/handlers.rs` | 1372 | 1 (`lazuli_ir`) | **wire** | Extension stub generation and path planning; large but framework-specific. |
| `emitter/imports.rs` | 59 | _none_ | **wire** | Small deterministic Go import-set printer. |
| `emitter/job.rs` | 462 | 1 (`lazuli_ir`) | **wire** | Job handler contract codegen; execution semantics stay in runtime/jobs. |
| `emitter/lint.rs` | 49 | _none_ | **wire** | Focused generated-file sanity checks. |
| `emitter/migration.rs` | 236 | 1 (`lazuli_ir`) | **wire** | Tenant migration contract codegen, not a migration engine. |
| `emitter/migration_ddl.rs` | 486 | 1 (`lazuli_ir`) | **wire** | Lazuli resource-to-DDL emission; Atlas remains the intended migration engine. |
| `emitter/module.rs` | 569 | 1 (`lazuli_ir`) | **wire** | Orchestrates per-feature emitters and generated file assembly. |
| `emitter/notification.rs` | 361 | 1 (`lazuli_ir`) | **wire** | Notification contract codegen; dispatch stays in runtime/plugin layers. |
| `emitter/patterns.rs` | 16 | _none_ | **wire** | Generated-code marker constants. |
| `emitter/printer.rs` | 94 | _none_ | **wire** | Minimal indentation writer for emitted Go snippets. |
| `emitter/query.rs` | 816 | 1 (`lazuli_ir`) | **wire** | Query contract/handler codegen; database behavior is delegated to Go runtime libraries. |
| `emitter/resource.rs` | 432 | 1 (`lazuli_ir`) | **wire** | Resource model and registration codegen. |
| `emitter/root.rs` | 321 | 1 (`lazuli_ir`) | **wire** | Root `go.mod`, app, and `main.go` emission glue. |
| `emitter/storage.rs` | 244 | 1 (`lazuli_ir`) | **wire** | Storage contract codegen; object-store mechanics stay outside this crate. |
| `emitter/translation.rs` | 78 | 1 (`lazuli_ir`) | **wire** | i18n catalog/embed glue emission. |
| `emitter/types.rs` | 144 | 1 (`lazuli_ir`) | **wire** | Lazuli type-to-Go type mapping. |
| `emitter/webhook.rs` | 264 | 1 (`lazuli_ir`) | **wire** | Webhook contract codegen; verifier/provider behavior is not implemented here. |

---

## Summary

**32/32 files (100.0%) are wire-clean for Rust codegen purposes.** The crate is large, but the size is concentrated in Lazuli-specific IR-to-Go emission and readiness checks. No source file has the risky shape this audit is meant to catch: >200 effective LOC, zero non-std crate imports, and an obvious reimplementation of a known Rust library such as a JSON parser, templater, date library, HTTP stack, SQL driver, or migration engine.

### Top 3 framework risks

1. **Emitter size and cohesion risk** - several emitters (`handlers.rs`, `query.rs`, `command.rs`, `check.rs`, `runtime.rs`) are large enough that regressions can hide in local string-generation branches. This is a maintainability/testing risk, not a wire-thin violation.

2. **Manual DDL string emission must stay vocabulary-limited** - `emitter/migration_ddl.rs` correctly emits Lazuli resource DDL instead of becoming a migration tool. It should keep delegating migration planning/diffing to Atlas rather than expanding toward schema-management behavior.

3. **Legacy/demo emission still coexists with v1 emitters** - `lib.rs` retains older demo generation next to the runtime-backed pipeline. That is acceptable today, but future cells should avoid growing the legacy path into a parallel framework surface.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 32 |
| Effective LOC audited | 10,433 |
| Wire-clean | 32 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Downstream-blocker risk | Low (maintainability risks only; no wire-thin rewrite required) |
