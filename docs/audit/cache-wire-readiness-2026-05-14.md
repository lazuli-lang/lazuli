# Cache Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/cache/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `adapter.go` | 33 | _none_ | **wire** | — | — |
| `builders.go` | 31 | _none_ | **wire** | — | — |
| `contract.go` | 50 | _none_ | **wire** | — | — |
| `redis.go` | 202 | `github.com/redis/go-redis/v9` | **wire** | — | — |
| `tags.go` | 16 | _none_ | **wire** | — | — |

---

## Summary

**5/5 files (100.0%) are wire-clean.** The cache sub-bucket is mostly typed contract/glue, and the only large file (`redis.go`) already wires the mature `github.com/redis/go-redis/v9` client rather than reimplementing Redis protocol behavior.

### Top 3 risks for downstream product ports

1. **Dual cache surfaces during migration** — `runtime/go/lazuli/cache.go` still carries the legacy in-process query cache while this bucket declares the newer typed `Backend` / `QuerySpec` contract. Until codegen consistently consumes `runtime/go/lazuli/cache/`, Pleiades/Atelier/Erudito or the Hostpoint port could see different cache semantics depending on which path a generated query uses.

2. **Redis tag index retention** — `redis.go` stores tag sets via Redis `SADD` but does not give those tag index keys a TTL. Expired cache entries can leave stale key names in tag sets until the next tag invalidation. This is not a wire-thin violation because Redis operations are delegated to go-redis, but high-churn tagged queries may accumulate fan-out overhead.

3. **Byte-only backend contract** — `Backend.Get` / `Put` exchange `[]byte`, while the legacy query cache stores `any`. Product ports need a single generated serialization policy before switching query result caching to the typed backend; otherwise cache misses, hit decoding, and schema changes may diverge by feature.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 5 |
| Wire-clean | 5 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (integration/migration risks only; no wire-thin rewrite required) |
