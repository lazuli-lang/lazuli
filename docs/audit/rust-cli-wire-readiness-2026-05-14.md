# Rust CLI Crate Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source SHA:** `dfebe8072ce23fd309bc63c7761b0cb6d7f99a2e`
- **Crate path:** `crates/lazuli_cli/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Notes |
|---|---:|---:|---|---|
| `src/app_manifest.rs` | 1977 | 1 | **wire** | Lazuli app/registry/profile/workspace manifest vocabulary parser into `lazuli_ir`; large, but framework-specific rather than a commodity parser clone. |
| `src/debug.rs` | 329 | 2 | **wire** | Debug bundle assembly around JSON envelopes and Lazuli IR source lookup. |
| `src/dev.rs` | 214 | 2 | **wire** | Wires `notify` plus `go run` process control for `lazuli dev`. |
| `src/doctor.rs` | 13371 | 8 | **wire** | Large rule engine over Lazuli syntax/IR/LSP diagnostics; not a commodity-library concern. |
| `src/doctor/vocab/mod.rs` | 4 | 0 | **wire** | Module declarations only. |
| `src/doctor/vocab/vocab_audit_001.rs` | 243 | 1 | **wire** | Lazuli vocabulary invariant checks over `lazuli_ir`. |
| `src/doctor/vocab/vocab_derived_read_001.rs` | 417 | 1 | **wire** | Lazuli-specific derived-read vocabulary checks over `lazuli_ir`. |
| `src/doctor/vocab/vocab_event_payload_001.rs` | 284 | 1 | **wire** | Lazuli event payload invariant checks over `lazuli_ir`. |
| `src/doctor/vocab/vocab_union_001.rs` | 251 | 1 | **wire** | Lazuli union vocabulary checks over `lazuli_ir`. |
| `src/examples_bundle.rs` | 313 | 3 | **wire** | Curated example JSONL/validation wrapper using `serde_json` and `toml`. |
| `src/lazurite_manifest.rs` | 394 | 2 | **wire** | TOML manifest model/validation wired through `serde` + `toml`. |
| `src/main.rs` | 6707 | 17 | **wire** | CLI command surface and projections wired through clap plus Lazuli crates; high LOC is command breadth and inline tests, not reimplemented infrastructure. |
| `src/migrate.rs` | 389 | 0 | **rewrite-as-wire** | Homemade migration discovery, apply/rollback ledger, and `lazuli_schema_migrations` table around `psql`; should wire the chosen migration toolchain instead. |
| `src/profile.rs` | 259 | 1 | **wire** | Lazuli semantic attribution over pprof-like labels, with serde output structs. |
| `src/seed.rs` | 189 | 0 | **wire** | Small command runner for seed scripts; below rewrite threshold and no obvious commodity engine is being reimplemented. |
| `src/templates.rs` | 3 | 1 | **wire** | `include_dir` template bundle declaration. |
| `src/upgrade.rs` | 250 | 2 | **wire** | Upgrade recipe metadata and smoke flow backed by TOML parsing; framework-specific. |

---

## Non-Wire Notes

`src/migrate.rs` crosses the wire-thin line: it is >200 effective LOC, has zero dependency-crate imports, and implements migration state tracking, file discovery, apply/rollback, status, SQL literal escaping, and `psql` process handling itself. Lazuli's existing architecture decision points migrations at Atlas/declarative diff; the CLI should become a thin adapter over that chosen toolchain or a mature Rust migration crate, not own a parallel migration engine.

---

## Summary

**16/17 files (94.1%) are wire-clean.** The CLI crate is mostly Lazuli-specific command dispatch, diagnostics, manifest vocabulary, IR projections, and bundle validation. Large Rust files here are not automatically violations because this crate is the framework's authoring surface; Rust-side DSL and doctor code legitimately lives close to the compiler/analyzer crates and often has no direct commodity equivalent.

### Top 3 framework risks

1. **Migration path has a real wire-thin violation** — `src/migrate.rs` duplicates migration-runner responsibility while the project has already selected Atlas-style declarative migration flow. This is the only rewrite-as-wire finding.

2. **CLI monolith concentration** — `src/main.rs` and `src/doctor.rs` are very large. They are not library-reimplementation problems, but they raise review and conflict risk for future command/projection/doctor work.

3. **Manifest parsing is framework-specific but still parallel grammar** — `src/app_manifest.rs` manually parses top-level Lazuli manifest forms. That is acceptable for this audit, but it should stay aligned with `lazuli_syntax`/IR decisions so app/registry/profile syntax does not drift into a second language front end.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 17 |
| Effective LOC audited | 25,594 |
| Wire-clean | 16 (94.1%) |
| Rewrite-as-wire | 1 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Downstream-blocker risk | Medium (`migrate.rs` should be replaced with a thin Atlas/migrator adapter before product-port migration workflows depend on it) |
