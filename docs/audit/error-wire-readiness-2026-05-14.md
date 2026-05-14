# Error Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit SHA:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `error` top-level typed error envelopes and stable error-code vocabulary under `runtime/go/lazuli/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `error.go` | 90 | 0 | **wire** | — | — |
| `error_adapter.go` | 14 | 0 | **wire** | — | — |
| `error_field.go` | 38 | 0 | **wire** | — | — |
| `error_lib_bug.go` | 13 | 0 | **wire** | — | — |
| `error_policy.go` | 14 | 0 | **wire** | — | — |
| `error_tenant.go` | 13 | 0 | **wire** | — | — |

---

## Summary

**6/6 files (100.0%) are wire-clean by the CLAUDE.md LOC/import test.** These top-level files are framework vocabulary: small typed envelopes, enum spellings, standard `error`/`Unwrap` behavior, and stable code constants consumed by handlers and the observability panic boundary. No file crosses the >100 effective LOC / zero external imports threshold, and no file reimplements a known Go error library concern. Test coverage exists in `runtime/go/lazuli/error_test.go` for wrapping, `errors.Is`/`errors.As`, enum string output, typed error formatting, and source-tag propagation.

### Top 3 risks for the Pleiades / Atelier / Erudito ports

1. **Typed error shape is still marked experimental** — downstream generated handlers can consume these envelopes now, but Pleiades/Atelier/Erudito should avoid baking unversioned assumptions about `ErrorBase`, `Surface`, and subtype fields into public client contracts before the pilot stabilizes them.

2. **Flat `Error` and typed suberrors coexist** — `handle.go` still returns the older flat `Error` path in several places while `observability/panic.go` understands typed errors. Codegen should choose one boundary policy per emitted endpoint so clients do not see mixed payload shapes for the same class of failure.

3. **Source metadata depends on codegen discipline** — `ErrorBaseFromContext` is thin and correct, but it only enriches errors when generated handlers attach `SourceTag` with `WithSource`. Missing source tags would reduce AI/debug routing quality without failing tests at this layer.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 6 |
| Wire-clean | 6 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (vocabulary/API stabilization only; no wire-thin rewrite required) |
