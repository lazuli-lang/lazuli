# Guards Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** guards — top-level policy, validator, rate-limit, and retention guard helpers in `runtime/go/lazuli/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `policy.go` | 16 | _none_ | **wire** | — | — |
| `ratelimit.go` | 281 | `golang.org/x/time/rate` | **wire** | — | — |
| `retention.go` | 185 | `pgx/v5`, `pgx/v5/pgxpool` | **wire** | — | — |
| `validator.go` | 32 | _none_ | **wire** | — | — |

---

## Summary

**4/4 files (100.0%) are wire-clean.** The guards topic is mostly framework contract glue: policy atoms, validator references, generated-code helper APIs, DSL rate-limit parsing, and retention scans over registered Lazuli resources. The two larger files already wire mature libraries for their commodity concerns (`golang.org/x/time/rate` for token buckets and `pgx/v5` for PostgreSQL transactions/identifier quoting), while the remaining custom code translates Lazuli vocabulary into those calls. Test coverage exists for `ratelimit.go` and `retention.go`; `policy.go` and `validator.go` do not have dedicated test files, but their exported types/functions are small registry and struct surfaces.

### Top 3 risks for downstream product ports

1. **Global in-memory rate limiter state** — `ratelimit.go` uses a process-local `defaultRateLimitStore`, so Pleiades/Atelier/Erudito deployments with multiple instances will enforce limits per process unless codegen or app wiring swaps to a shared limiter later. This is not a wire-thin violation because token-bucket mechanics come from `x/time/rate`, but distributed semantics remain a product-port risk.

2. **Retention schema conventions are implicit** — `retention.go` derives table and column names from Lazuli resource metadata and assumes `deleted_at` plus snake_case resource/PII fields. The file correctly wires `pgx/v5`, but framework users can hit runtime SQL drift if generated migrations and retention metadata diverge.

3. **Validator registry is unsynchronized after init** — `validator.go` keeps the registry as a plain package map. That is acceptable for init-time registration, but downstream products should avoid dynamic validator registration after serving begins unless the runtime later hardens the registry lifecycle.

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
| Downstream-blocker risk | Low (framework-bridge risks only; no wire-thin rewrite required) |
