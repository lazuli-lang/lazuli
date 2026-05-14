# Lifecycle Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `runtime/go/lazuli/lifecycle.go` top-level graceful shutdown helper consumed by generated Go entrypoints.

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `lifecycle.go` | 24 | _none_ | **wire** | — | — |

---

## Summary

**1/1 files (100.0%) are wire-clean.** `lifecycle.go` is a 24 LOC wrapper around Go stdlib signal handling and `net/http.Server.Shutdown`; it does not reimplement a known OSS lifecycle library. Test coverage exists in `lifecycle_test.go`, which injects a simulated `SIGINT` path and verifies the server shutdown hook runs.

### Top 3 risks for downstream product ports

1. **Generated-main contract is implicit** - The file comment says generated `main.go` uses `DefaultGracePeriod`, but no checked fixture currently references `WaitForShutdown` directly in this worktree. Pleiades/Atelier/Erudito need codegen to keep using the same exported helper instead of each generated entrypoint inventing its own signal loop.

2. **Only HTTP server shutdown is modeled** - The helper shuts down one `*http.Server`. Products with River workers, plugin servers, DB pools, or background resources will need a small orchestrated lifecycle layer above this function so graceful shutdown covers more than the HTTP listener.

3. **No nil/invalid grace guard** - Passing a nil server would panic and a zero or negative grace period would produce immediate cancellation behavior. Generated code should keep supplying a real server and `DefaultGracePeriod` unless a product explicitly configures another positive duration.

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
| Downstream-blocker risk | Low (stdlib wire is sound; remaining risks are generated-main integration and broader resource orchestration) |
