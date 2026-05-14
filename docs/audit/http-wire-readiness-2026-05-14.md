# HTTP Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** Top-level HTTP runtime helpers in `runtime/go/lazuli/` (`Mux`, cookies, CSRF, middleware, panic recovery, request IDs).

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `http.go` | 148 | _none_ | **questionable** | see note | — |
| `http_cookies.go` | 62 | _none_ | **wire** | — | — |
| `http_csrf.go` | 20 | _none_ | **wire** | — | — |
| `http_middleware.go` | 12 | _none_ | **wire** | — | — |
| `http_recover.go` | 26 | _none_ | **wire** | — | — |
| `http_request_id.go` | 39 | _none_ | **wire** | — | — |

### http.go note

`http.go` trips the mechanical threshold (148 eff. LOC, zero external imports), but the code is the framework's top-level `net/http` wiring: generated command/query registries are mounted onto Go 1.22+ `ServeMux`, request bodies are handed to typed dispatchers, Lazuli errors are encoded as JSON envelopes, and request context is bridged into `Ctx`. There is no mature OSS library that should replace this vocabulary bridge without also replacing Lazuli's generated runtime contract. Verdict: **questionable but acceptable** for v0; the main cleanup opportunity is to keep future middleware additions out of this file so it does not become a second router framework.

---

## Summary

**6/6 files (100.0%) have no rewrite-as-wire or delete-candidate findings.** One file (`http.go`) is mechanically questionable because it is over 100 eff. LOC with no external imports, but the code is genuinely framework-specific glue over `net/http`; the smaller helpers remain thin stdlib wrappers. Test coverage exists for cookies, middleware chaining, panic recovery, and request IDs; there are no direct tests for `Mux` routing or the `CrossOriginProtection` guard wiring.

### Top 3 risks for downstream product ports

1. **`http.go` centralization pressure** — Pleiades, Atelier, and Erudito will all consume this package through `lazuli.dev/runtime/lazuli`, so adding auth, rate limiting, observability, and content negotiation directly into `Mux` would turn a framework bridge into a custom HTTP stack. Keep those concerns as `Middleware` or bucket-level adapters.

2. **CSRF depends on current Go runtime surface** — `http_csrf.go` is intentionally a thin wrapper around `net/http.CrossOriginProtection`. Product ports pinned to an older Go toolchain will fail at compile time rather than degrade gracefully; this should be treated as a toolchain requirement, not patched with a custom CSRF implementation.

3. **Partial direct HTTP coverage** — Cookies, recovery, request IDs, and middleware order have focused tests, but `Mux` command/query route dispatch and CSRF allowlist behavior are not directly covered. That leaves generated product ports as the first broad integration test for top-level routing behavior.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 6 |
| Wire-clean | 6 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 1 (`http.go`) |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (framework-specific routing glue; no rewrite required) |
