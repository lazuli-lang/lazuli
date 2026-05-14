# Persistence Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** persistence top-level runtime files (`runtime/go/lazuli/db.go`, `runtime/go/lazuli/query.go`) that provide Postgres pool wiring and generated query metadata.

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `db.go` | 92 | `github.com/jackc/pgx/v5`, `.../pgconn`, `.../pgxpool` | **wire** | — | — |
| `query.go` | 82 | _none_ | **wire** | — | — |

---

## Summary

**2/2 files (100.0%) are wire-clean.** `db.go` is a thin wrapper around `pgx/v5` pool/transaction primitives, while `query.go` is framework-specific metadata and type-erasure for generated query declarations. No file exceeds the >100 LOC / zero external import threshold, and neither file reimplements a known OSS persistence library. Test coverage is direct for `db.go` pool defaults and parse errors via `db_test.go`; `query.go` has no dedicated unit test but is exercised indirectly through registry and query execution paths.

### Top 3 risks for downstream product ports

1. **Process-global DB pool lifecycle** — `DB()` panics until `Boot`/`SetDB` installs a pool. Pleiades, Atelier, and Erudito generated entrypoints must preserve the boot ordering, especially in tests and multi-app processes.

2. **Hard-coded pgx pool defaults** — `db.go` fixes connection counts, lifetime, idle time, health checks, and simple protocol mode in code. This is still wire-thin, but high-concurrency products may need a generated/configured override path rather than patching runtime defaults.

3. **Query metadata/runner coupling** — `query.go` is only declarative, but its fields are tightly consumed by `run.go`, `register.go`, and HTTP dispatch. Schema changes to `FilterRule`, `SearchSpec`, `LookupKey`, or cache fields need coordinated emitter/runtime updates so all product ports keep the same query semantics.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 2 |
| Wire-clean | 2 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (configuration/lifecycle risks only; no wire-thin rewrite required) |
