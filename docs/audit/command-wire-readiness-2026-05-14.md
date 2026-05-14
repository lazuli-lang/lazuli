# Command Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `command` top-level runtime command execution, effect declarations, approval builder, and lowered command metadata under `runtime/go/lazuli/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `handle.go` | 407 | `github.com/jackc/pgx/v5` | **wire** | — | — |
| `effect.go` | 52 | _none_ | **wire** | — | — |
| `command.go` | 86 | `lazuli.dev/runtime/lazuli/jobs` | **wire** | — | — |
| `approval.go` | 17 | _none_ | **wire** | — | — |

---

## Summary

**4/4 files (100.0%) are wire-clean.** The command topic is mostly framework vocabulary and execution glue. The only large file (`handle.go`) already wires `pgx/v5` for transactional database work and row scanning instead of replacing the database driver or ORM layer with custom machinery. `effect.go`, `command.go`, and `approval.go` are small closed DSL/runtime contracts. Test coverage is uneven: `approval_test.go` directly covers the approval builder, while command dispatch/effect behavior is mostly exercised indirectly through registry, HTTP dispatch, and surrounding runtime tests.

### Top 3 risks for downstream product ports

1. **Large command execution surface in `handle.go`** — Policy checks, validator dispatch, transaction management, SQL generation, event publication, cache invalidation, and boot wiring all live in one top-level file. This is not a wire-thin violation because it delegates DB behavior to `pgx/v5`, but Pleiades/Atelier/Erudito will all depend on the same broad execution path, so regressions have high blast radius.

2. **Reflection-based binding resolution** — `resolveSource`, `readPath`, and `rowToMap` bridge generated DSL shapes to Go structs with reflection. That is legitimate framework-specific glue, but schema drift or field-name mismatches can surface at runtime instead of compile time until codegen emits typed accessors.

3. **Partial command metadata enforcement** — `command.go` carries approval, timeout, retry, idempotency, external call, and deprecation metadata, but this topic's execution path currently enforces only policy, validators, effects, emits, and cache invalidation. Product ports should not assume the declared metadata is operational until the corresponding runtime/codegen cells wire each directive.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 4 |
| Wire-clean | 4 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Medium (execution coverage and metadata enforcement risks; no wire-thin rewrite required) |
