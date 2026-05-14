# Context Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `runtime/go/lazuli/` top-level context and request metadata glue (`context.go`, `ctx.go`)

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `context.go` | 22 | _none_ | **wire** | — | — |
| `ctx.go` | 29 | _none_ | **wire** | — | — |

---

## Summary

**2/2 files (100.0%) are wire-clean.** The context topic is intentionally framework-specific vocabulary glue: `context.go` stamps generated `.lzi` source metadata onto stdlib `context.Context`, while `ctx.go` defines the runtime request envelope consumed by commands, queries, auth, cache, rate limits, and event emission. `context_test.go` covers `WithSource` / `SourceTagFromContext` attach, empty, and overwrite behavior; `ctx.go` has no dedicated tests because it is currently a data shape.

### Top 3 risks for downstream product ports

1. **Dual context surfaces** — Runtime APIs mix stdlib `context.Context` and `*lazuli.Ctx`. Pleiades/Atelier/Erudito codegen must consistently decide when source tags live on `Ctx.Context` versus a bare transport/job context, or tracing and typed error metadata can be dropped.

2. **Nil embedded context tolerance** — Several tests and helpers construct `*lazuli.Ctx` directly. Callers that forget to initialize `Ctx.Context` need defensive runtime paths, especially in auth and OAuth helpers that mutate or read the embedded context.

3. **Request metadata completeness** — `Ctx` already carries `RequestID`, `TraceID`, actor, user, tenant, and `Now`, but transport population is split across HTTP dev-session helpers and middleware. Product ports need one generated construction path so auth, tenancy, rate limits, audit, and event emission observe the same values.

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
| Downstream-blocker risk | Low (framework context-shape migration risks only; no wire-thin rewrite required) |
