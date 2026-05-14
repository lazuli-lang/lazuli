# Core Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `core` top-level runtime declarations and framework execution glue under `runtime/go/lazuli/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `api.go` | 20 | 0 (_none_) | **wire** | — | — |
| `audit.go` | 6 | 0 (_none_) | **wire** | — | — |
| `boot.go` | 48 | 1 (`github.com/jackc/pgx/v5/pgxpool`) | **wire** | — | — |
| `buildinfo_runtime.go` | 16 | 0 (_none_) | **wire** | — | — |
| `cache.go` | 163 | 0 (_none_) | **rewrite-as-wire** | `github.com/hashicorp/golang-lru/v2/expirable` | M (<= 200 LOC) |
| `env.go` | 64 | 0 (_none_) | **wire** | — | — |
| `run.go` | 263 | 1 (`github.com/jackc/pgx/v5`) | **wire** | — | — |
| `types.go` | 62 | 0 (_none_) | **wire** | — | — |
| `resource.go` | 47 | 0 (_none_) | **wire** | — | — |

### cache.go note

`cache.go` is framework-owned query-cache vocabulary, but the implementation crosses the CLAUDE.md rewrite threshold: 163 effective LOC, zero external imports, and a known library exists for the generic LRU + TTL concern. The Lazuli-specific parts are `CacheSpec`, tenant-aware key construction, JSON argument hashing, `Stats`, and `FlushCache`; the list/map eviction machinery should be replaced with a narrow wrapper around `github.com/hashicorp/golang-lru/v2/expirable` or the legacy cache should be retired in favor of `runtime/go/lazuli/cache/`.

---

## Summary

**8/9 files (88.9%) are wire-clean.** The core top-level runtime group is mostly DSL declaration structs, context boot policy, pgx-backed query execution, and semantic aliases; the only wire-thin violation is the legacy process-local query cache. Test coverage exists for `BuildInfo`, `LoadEnv`, observability context consumers, and cache consumers through HTTP/command paths, but there is no focused unit coverage for the LRU/TTL edge cases in `cache.go` or for the SQL string assembly paths in `run.go`.

### Top 3 risks for downstream product ports

1. **Legacy query-cache implementation drift** - `cache.go` hand-rolls eviction, expiry, and stats while `runtime/go/lazuli/cache/` already defines the newer typed cache backend contract. Pleiades, Atelier, and Erudito could observe different cache semantics depending on which generated query path they consume.

2. **Generic SQL assembly remains a central blast radius** - `run.go` is wire-clean because it uses `pgx/v5` for scanning, but it still owns Lazuli's default SELECT/filter/order/lookup lowering. Any mismatch between codegen field names and runtime quoting affects every product port that relies on generated list/lookup queries.

3. **Top-level semantic aliases are intentionally weak** - `types.go` keeps `Email`, `Money`, `JSON`, and capability refs as aliases for pgx/json ergonomics. That is appropriate wire glue for v0, but product ports must keep validator/adaptor wiring active because the runtime types do not enforce these semantic contracts by themselves.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 9 |
| Wire-clean | 8 (88.9%) |
| Rewrite-as-wire | 1 (`cache.go`) |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 1 (CORE-1) |
| Downstream-blocker risk | Medium (`cache.go` should converge with the typed cache backend before broader product ports rely on cached query semantics) |
